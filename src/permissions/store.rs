//! Per-user authority settings and the global pause switch (S2-T6/T8).
//!
//! Resolution order for a tool call: per-tool override → per-agent user setting → the agent's
//! default mode from `mcp_allowlists.toml`. Settings live in memory and, when a pool is
//! configured, in `agent_permissions`.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::domain::Domain;
use crate::permissions::Mode;
use crate::tools::allowlist;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentPermission {
    pub agent_id: String,
    pub mode: Option<Mode>,
    #[serde(default)]
    pub tool_overrides: HashMap<String, Mode>,
}

/// What the trust center shows for one agent.
#[derive(Clone, Debug, Serialize)]
pub struct AgentPermissionView {
    pub agent_id: String,
    pub world: Domain,
    pub mode: Mode,
    pub default_mode: Mode,
    pub tool_overrides: HashMap<String, Mode>,
    pub tools: Vec<ToolView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolView {
    pub name: String,
    pub effect: String,
    pub mode: Mode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PauseScope {
    All,
    Work,
    Home,
}

impl PauseScope {
    pub fn covers(&self, domain: Domain) -> bool {
        match self {
            PauseScope::All => true,
            PauseScope::Work => domain == Domain::Work,
            PauseScope::Home => domain == Domain::Home,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PauseState {
    /// scope → paused until (None = indefinitely)
    pub paused: HashMap<String, Option<DateTime<Utc>>>,
}

#[derive(Default)]
struct Inner {
    /// user_id → agent_id → setting
    users: HashMap<String, HashMap<String, AgentPermission>>,
    loads: crate::db::UserLoads,
    pause: PauseState,
}

#[derive(Clone)]
pub struct PermissionStore {
    inner: Arc<RwLock<Inner>>,
    pg: Option<PgPool>,
}

impl PermissionStore {
    pub fn new(pg: Option<PgPool>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            pg,
        }
    }

    pub fn in_memory() -> Self {
        Self::new(None)
    }

    /// Load the user's settings on first touch. A database error leaves the user unloaded so a
    /// later touch reads again (until then the catalog defaults apply); a setting changed here
    /// meanwhile is newer than its stored row and wins.
    async fn ensure_loaded(&self, user_id: &str) {
        let Some(pool) = &self.pg else { return };
        if !self.inner.read().await.loads.should_load(user_id) {
            return;
        }
        let rows: Vec<(String, String, serde_json::Value)> = match sqlx::query_as(
            "SELECT agent_id, mode, tool_overrides FROM agent_permissions WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("agent_permissions load failed; retrying on a later request: {e:#}");
                self.inner.write().await.loads.failed(user_id);
                return;
            }
        };
        let mut guard = self.inner.write().await;
        if guard.loads.is_loaded(user_id) {
            return;
        }
        let user = guard.users.entry(user_id.to_string()).or_default();
        for (agent_id, mode, overrides) in rows {
            user.entry(agent_id.clone()).or_insert(AgentPermission {
                agent_id,
                mode: Mode::parse(&mode),
                tool_overrides: serde_json::from_value(overrides).unwrap_or_default(),
            });
        }
        guard.loads.succeeded(user_id);
    }

    async fn persist(&self, user_id: &str, perm: &AgentPermission) {
        let Some(pool) = &self.pg else { return };
        let overrides = serde_json::to_value(&perm.tool_overrides).unwrap_or_default();
        let mode = perm.mode.unwrap_or_else(|| allowlist::catalog().mode_for(&perm.agent_id));
        if let Err(e) = sqlx::query(
            "INSERT INTO agent_permissions (user_id, agent_id, mode, tool_overrides) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (user_id, agent_id) DO UPDATE SET mode = $3, tool_overrides = $4, updated_at = NOW()",
        )
        .bind(user_id)
        .bind(&perm.agent_id)
        .bind(mode.as_str())
        .bind(overrides)
        .execute(pool)
        .await
        {
            tracing::warn!("agent_permissions persist failed: {e:#}");
        }
    }

    /// Effective mode for `agent_id` (and optionally one of its tools) for this user.
    pub async fn mode_for(&self, user_id: &str, agent_id: &str, tool: Option<&str>) -> Mode {
        self.ensure_loaded(user_id).await;
        let guard = self.inner.read().await;
        let setting = guard.users.get(user_id).and_then(|u| u.get(agent_id));
        if let (Some(tool), Some(setting)) = (tool, setting)
            && let Some(m) = setting.tool_overrides.get(tool)
        {
            return *m;
        }
        setting
            .and_then(|s| s.mode)
            .unwrap_or_else(|| allowlist::catalog().mode_for(agent_id))
    }

    pub async fn set_mode(&self, user_id: &str, agent_id: &str, mode: Mode) -> AgentPermission {
        self.ensure_loaded(user_id).await;
        let perm = {
            let mut guard = self.inner.write().await;
            let user = guard.users.entry(user_id.to_string()).or_default();
            let perm = user.entry(agent_id.to_string()).or_insert_with(|| AgentPermission {
                agent_id: agent_id.to_string(),
                ..Default::default()
            });
            perm.mode = Some(mode);
            perm.clone()
        };
        self.persist(user_id, &perm).await;
        perm
    }

    /// Set (or clear with `None`) a per-tool override.
    pub async fn set_tool_override(&self, user_id: &str, agent_id: &str, tool: &str, mode: Option<Mode>) -> AgentPermission {
        self.ensure_loaded(user_id).await;
        let perm = {
            let mut guard = self.inner.write().await;
            let user = guard.users.entry(user_id.to_string()).or_default();
            let perm = user.entry(agent_id.to_string()).or_insert_with(|| AgentPermission {
                agent_id: agent_id.to_string(),
                ..Default::default()
            });
            match mode {
                Some(m) => {
                    perm.tool_overrides.insert(tool.to_string(), m);
                }
                None => {
                    perm.tool_overrides.remove(tool);
                }
            }
            perm.clone()
        };
        self.persist(user_id, &perm).await;
        perm
    }

    /// Every agent in the catalog with its effective settings for this user.
    pub async fn list(&self, user_id: &str) -> Vec<AgentPermissionView> {
        self.ensure_loaded(user_id).await;
        let guard = self.inner.read().await;
        let user = guard.users.get(user_id);
        let catalog = allowlist::catalog();
        let mut out: Vec<AgentPermissionView> = catalog
            .specs()
            .map(|spec| {
                let setting = user.and_then(|u| u.get(&spec.id));
                let mode = setting.and_then(|s| s.mode).unwrap_or(spec.mode);
                let overrides = setting.map(|s| s.tool_overrides.clone()).unwrap_or_default();
                let tools = spec
                    .tools
                    .iter()
                    .map(|(name, effect)| ToolView {
                        name: name.clone(),
                        effect: effect.map(|e| e.as_str().to_string()).unwrap_or_else(|| "unclassified".into()),
                        mode: overrides.get(name).copied().unwrap_or(mode),
                    })
                    .collect();
                AgentPermissionView {
                    agent_id: spec.id.clone(),
                    world: spec.world,
                    mode,
                    default_mode: spec.mode,
                    tool_overrides: overrides,
                    tools,
                }
            })
            .collect();
        out.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));
        out
    }

    // ---- pause switch (process-wide; the prototype serves one person) ----

    pub async fn pause(&self, scope: PauseScope, until: Option<DateTime<Utc>>) {
        let key = serde_json::to_value(scope).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "all".into());
        self.inner.write().await.pause.paused.insert(key, until);
    }

    pub async fn resume(&self, scope: PauseScope) {
        let key = serde_json::to_value(scope).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "all".into());
        self.inner.write().await.pause.paused.remove(&key);
    }

    pub async fn resume_all(&self) {
        self.inner.write().await.pause.paused.clear();
    }

    /// Whether writes for `domain` are currently paused (expired pauses are ignored).
    pub async fn is_paused(&self, domain: Domain) -> bool {
        let now = Utc::now();
        let guard = self.inner.read().await;
        guard.pause.paused.iter().any(|(scope, until)| {
            let active = until.map(|u| u > now).unwrap_or(true);
            if !active {
                return false;
            }
            match scope.as_str() {
                "all" => true,
                "work" => domain == Domain::Work,
                "home" => domain == Domain::Home,
                _ => false,
            }
        })
    }

    pub async fn pause_state(&self) -> PauseState {
        let now = Utc::now();
        let mut state = self.inner.read().await.pause.clone();
        state.paused.retain(|_, until| until.map(|u| u > now).unwrap_or(true));
        state
    }
}
