use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use adk_runner::Runner;
use adk_session::SessionService;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::agents::deck::McpPool;
use crate::domain::Domain;
use adk_awp::InMemoryEventSubscriptionService;

use crate::ambient::AmbientStore;
use crate::agents::live::LiveMcpPool;
use crate::agents::lisbon::LisbonMcpPool;
use crate::agents::morning::MorningMcpPool;
use crate::agents::people::PeopleMcpPool;
use crate::agents::week::WeekMcpPool;
use crate::scenarios::ScenarioLiveFlags;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionArtifacts {
    pub xlsx: Option<String>,
    pub docx: Option<String>,
    pub pptx: Option<String>,
    pub combined_pptx: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardRecord {
    pub index: usize,
    pub card: serde_json::Value,
    pub status: String,
    pub resolve: Option<serde_json::Value>,
    pub pinned: bool,
    pub removed: bool,
    /// Life domain of the card (ADR-002); `shared` for Phase 1 data.
    #[serde(default)]
    pub domain: Domain,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub title: String,
    pub glyph: String,
    pub agent: String,
    pub rail: String,
    /// Life domain of the agent (ADR-002); `shared` for Phase 1 data.
    #[serde(default)]
    pub domain: Domain,
}

/// One turn of the Mother chat (S1-T5). Kept short: the last [`CHAT_HISTORY_LIMIT`] turns.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatTurn {
    /// `user` | `mother`
    pub role: String,
    pub text: String,
    pub ts: chrono::DateTime<chrono::Utc>,
}

pub const CHAT_HISTORY_LIMIT: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub user_id: String,
    pub scenario: Option<String>,
    pub origin_text: Option<String>,
    pub artifacts: SessionArtifacts,
    pub cards: Vec<CardRecord>,
    pub agents_active: Vec<AgentRecord>,
    pub agents_resting: Vec<AgentRecord>,
    /// Mother chat transcript for this session (S1-T5).
    #[serde(default)]
    pub chat_history: Vec<ChatTurn>,
    /// Created by a signed-in user: `user_id` is their JWT subject and only they may act in the
    /// session. `false` for anonymous sessions, which admit whoever holds the id.
    #[serde(default)]
    pub signed_in: bool,
}

impl SessionRecord {
    fn new(session_id: String, user_id: String) -> Self {
        Self {
            session_id,
            user_id,
            scenario: None,
            origin_text: None,
            artifacts: SessionArtifacts::default(),
            cards: Vec::new(),
            agents_active: Vec::new(),
            agents_resting: Vec::new(),
            chat_history: Vec::new(),
            signed_in: false,
        }
    }

    /// Whether `caller` (the JWT subject, if any) may act in this session.
    pub fn admits(&self, caller: Option<&str>) -> bool {
        !self.signed_in || caller == Some(self.user_id.as_str())
    }
}

pub type SharedSessionService = Arc<dyn SessionService + Send + Sync>;

#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<RwLock<HashMap<String, SessionRecord>>>,
    pg: Option<PgPool>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            pg: None,
        }
    }

    pub fn with_postgres(pool: PgPool) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            pg: Some(pool),
        }
    }

    pub fn postgres_enabled(&self) -> bool {
        self.pg.is_some()
    }

    pub async fn create(&self) -> SessionRecord {
        self.create_for_user(Uuid::new_v4().to_string()).await
    }

    pub async fn create_for_user(&self, user_id: String) -> SessionRecord {
        let record = SessionRecord::new(Uuid::new_v4().to_string(), user_id);
        self.save(&record).await;
        record
    }

    /// A session owned by a signed-in user (`user_id` = JWT subject); see [`SessionRecord::admits`].
    pub async fn create_signed_in(&self, user_id: String) -> SessionRecord {
        let record = SessionRecord {
            signed_in: true,
            ..SessionRecord::new(Uuid::new_v4().to_string(), user_id)
        };
        self.save(&record).await;
        record
    }

    /// A new session for `caller` (the JWT subject): bound to them when signed in, anonymous otherwise.
    pub async fn create_for_caller(&self, caller: Option<String>) -> SessionRecord {
        match caller {
            Some(user_id) => self.create_signed_in(user_id).await,
            None => self.create().await,
        }
    }

    pub async fn get(&self, session_id: &str) -> Option<SessionRecord> {
        if let Some(pool) = &self.pg {
            if let Ok(Some(record)) = load_ui_session(pool, session_id).await {
                self.inner
                    .write()
                    .await
                    .insert(session_id.to_string(), record.clone());
                return Some(record);
            }
        }
        self.inner.read().await.get(session_id).cloned()
    }

    async fn save(&self, record: &SessionRecord) {
        self.inner
            .write()
            .await
            .insert(record.session_id.clone(), record.clone());
        if let Some(pool) = &self.pg {
            if let Err(e) = upsert_ui_session(pool, record).await {
                tracing::warn!("ui_session persist failed: {e:#}");
            }
        }
    }

    async fn mutate<F>(&self, session_id: &str, f: F)
    where
        F: FnOnce(&mut SessionRecord),
    {
        let mut guard = self.inner.write().await;
        let Some(record) = guard.get_mut(session_id) else {
            return;
        };
        f(record);
        let snapshot = record.clone();
        drop(guard);
        if let Some(pool) = &self.pg {
            if let Err(e) = upsert_ui_session(pool, &snapshot).await {
                tracing::warn!("ui_session persist failed: {e:#}");
            }
        }
    }

    pub async fn set_scenario(&self, session_id: &str, scenario: &str, origin_text: Option<&str>) {
        self.mutate(session_id, |record| {
            record.scenario = Some(scenario.into());
            if let Some(text) = origin_text {
                record.origin_text = Some(text.into());
            }
        })
        .await;
    }

    pub async fn update_artifacts(&self, session_id: &str, artifacts: SessionArtifacts) {
        self.mutate(session_id, |record| {
            record.artifacts = artifacts;
        })
        .await;
    }

    pub async fn upsert_card(
        &self,
        session_id: &str,
        index: usize,
        card: serde_json::Value,
        status: &str,
        resolve: Option<serde_json::Value>,
        pinned: bool,
    ) {
        self.mutate(session_id, |record| {
            if let Some(existing) = record.cards.iter_mut().find(|c| c.index == index) {
                existing.card = card;
                existing.status = status.into();
                if resolve.is_some() {
                    existing.resolve = resolve;
                }
                existing.pinned = pinned;
                return;
            }
            let domain = Domain::for_card(record.scenario.as_deref().unwrap_or(""), &card);
            record.cards.push(CardRecord {
                index,
                card,
                status: status.into(),
                resolve,
                pinned,
                removed: false,
                domain,
            });
            record.cards.sort_by_key(|c| c.index);
        })
        .await;
    }

    pub async fn remove_card(&self, session_id: &str, title: &str) {
        self.mutate(session_id, |record| {
            for card in &mut record.cards {
                if card.card.get("title").and_then(|t| t.as_str()) == Some(title) {
                    card.removed = true;
                }
            }
        })
        .await;
    }

    pub async fn list_cards(&self, session_id: &str) -> Option<Vec<CardRecord>> {
        self.get(session_id)
            .await
            .map(|r| r.cards.into_iter().filter(|c| !c.removed).collect())
    }

    pub async fn agent_active(&self, session_id: &str, mut agent: AgentRecord) {
        self.mutate(session_id, |record| {
            if agent.domain == Domain::Shared {
                agent.domain = Domain::for_scenario(record.scenario.as_deref().unwrap_or(""));
            }
            record.agents_resting.retain(|a| a.id != agent.id);
            if !record.agents_active.iter().any(|a| a.id == agent.id) {
                record.agents_active.push(agent);
            }
        })
        .await;
    }

    pub async fn agent_snooze(&self, session_id: &str, agent: AgentRecord) {
        self.mutate(session_id, |record| {
            record.agents_active.retain(|a| a.id != agent.id);
            record.agents_resting.retain(|a| a.id != agent.id);
            record.agents_resting.push(AgentRecord {
                rail: "resting".into(),
                ..agent
            });
        })
        .await;
    }

    pub async fn agent_wake(&self, session_id: &str, agent_id: &str) -> Option<AgentRecord> {
        let mut guard = self.inner.write().await;
        let record = guard.get_mut(session_id)?;
        let idx = record.agents_resting.iter().position(|a| a.id == agent_id)?;
        let mut agent = record.agents_resting.remove(idx);
        agent.rail = "active".into();
        record.agents_active.retain(|a| a.id != agent_id);
        record.agents_active.push(agent.clone());
        let snapshot = record.clone();
        drop(guard);
        if let Some(pool) = &self.pg {
            if let Err(e) = upsert_ui_session(pool, &snapshot).await {
                tracing::warn!("ui_session persist failed: {e:#}");
            }
        }
        Some(agent)
    }

    /// Append a Mother chat turn, keeping the last [`CHAT_HISTORY_LIMIT`] turns.
    pub async fn append_chat(&self, session_id: &str, role: &str, text: &str) {
        let turn = ChatTurn {
            role: role.into(),
            text: text.into(),
            ts: chrono::Utc::now(),
        };
        self.mutate(session_id, |record| {
            record.chat_history.push(turn);
            if record.chat_history.len() > CHAT_HISTORY_LIMIT {
                let drop = record.chat_history.len() - CHAT_HISTORY_LIMIT;
                record.chat_history.drain(0..drop);
            }
        })
        .await;
    }

    pub async fn chat_history(&self, session_id: &str) -> Vec<ChatTurn> {
        self.get(session_id).await.map(|r| r.chat_history).unwrap_or_default()
    }

    pub async fn list_agents(&self, session_id: &str) -> Option<(Vec<AgentRecord>, Vec<AgentRecord>)> {
        let record = self.get(session_id).await?;
        Some((record.agents_active, record.agents_resting))
    }
}

async fn load_ui_session(pool: &PgPool, session_id: &str) -> anyhow::Result<Option<SessionRecord>> {
    let row = sqlx::query_as::<_, UiSessionRow>(
        "SELECT session_id, user_id, scenario, origin_text, artifacts, cards, agents_active, agents_resting, chat_history, signed_in FROM ui_sessions WHERE session_id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SessionRecord {
        session_id: r.session_id,
        user_id: r.user_id,
        scenario: r.scenario,
        origin_text: r.origin_text,
        artifacts: serde_json::from_value(r.artifacts).unwrap_or_default(),
        cards: serde_json::from_value(r.cards).unwrap_or_default(),
        agents_active: serde_json::from_value(r.agents_active).unwrap_or_default(),
        agents_resting: serde_json::from_value(r.agents_resting).unwrap_or_default(),
        chat_history: serde_json::from_value(r.chat_history).unwrap_or_default(),
        signed_in: r.signed_in,
    }))
}

async fn upsert_ui_session(pool: &PgPool, record: &SessionRecord) -> anyhow::Result<()> {
    let artifacts = serde_json::to_value(&record.artifacts)?;
    let cards = serde_json::to_value(&record.cards)?;
    let agents_active = serde_json::to_value(&record.agents_active)?;
    let agents_resting = serde_json::to_value(&record.agents_resting)?;
    let chat_history = serde_json::to_value(&record.chat_history)?;

    sqlx::query(
        // `signed_in` is never cleared: an owner-bound session stays bound.
        "INSERT INTO ui_sessions (session_id, user_id, scenario, origin_text, artifacts, cards, agents_active, agents_resting, chat_history, signed_in) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT (session_id) DO UPDATE SET user_id = $2, scenario = $3, origin_text = $4, artifacts = $5, cards = $6, agents_active = $7, agents_resting = $8, chat_history = $9, signed_in = ui_sessions.signed_in OR $10, updated_at = NOW()",
    )
    .bind(&record.session_id)
    .bind(&record.user_id)
    .bind(&record.scenario)
    .bind(&record.origin_text)
    .bind(artifacts)
    .bind(cards)
    .bind(agents_active)
    .bind(agents_resting)
    .bind(chat_history)
    .bind(record.signed_in)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct UiSessionRow {
    session_id: String,
    user_id: String,
    scenario: Option<String>,
    origin_text: Option<String>,
    artifacts: serde_json::Value,
    cards: serde_json::Value,
    agents_active: serde_json::Value,
    agents_resting: serde_json::Value,
    chat_history: serde_json::Value,
    signed_in: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RuntimeStatus {
    pub milestone: &'static str,
    /// Phase 2 sprint the running build corresponds to (e.g. `P2-S0`).
    pub phase: &'static str,
    pub agents_enabled: bool,
    pub postgres_enabled: bool,
    pub auth_enabled: bool,
    pub voice_enabled: bool,
    pub coordinator_enabled: bool,
    pub uses_mock_orchestration: bool,
    pub scenarios: ScenarioLiveFlags,
    pub mcp_worksheet: bool,
    pub mcp_docx: bool,
    pub mcp_slides: bool,
    pub mcp_news: bool,
    pub allow_demo_mode: bool,
    pub public_domain: String,
    pub signup_endpoint: Option<String>,
    pub linkedin_partner_id: Option<String>,
    pub linkedin_conversion_id: Option<u64>,
}

#[derive(Clone)]
pub struct AppState {
    pub runtime: RuntimeStatus,
    pub sessions: SessionStore,
    pub deck_runner: Option<Arc<Runner>>,
    pub combine_runner: Option<Arc<Runner>>,
    pub morning_runner: Option<Arc<Runner>>,
    pub live_runner: Option<Arc<Runner>>,
    pub people_runner: Option<Arc<Runner>>,
    pub week_runner: Option<Arc<Runner>>,
    pub lisbon_runner: Option<Arc<Runner>>,
    pub router_runner: Option<Arc<Runner>>,
    pub suzy_runner: Option<Arc<Runner>>,
    /// LLM half of the Mother Agent (S1-T2); `None` without an API key.
    pub mother_runner: Option<Arc<Runner>>,
    /// Content-free activity ledger (S2-T1). Same instance the permission gate writes to.
    pub ledger: crate::intelligence::LedgerService,
    /// Per-user authority modes and the pause switch (S2-T8).
    pub permissions: crate::permissions::PermissionStore,
    /// Actions waiting for approval (S2-T7).
    pub pending: crate::permissions::PendingActions,
    /// Audit log of every executed / queued / denied effect (S2-T6).
    pub audit: crate::permissions::AuditLog,
    /// Personal memory — known · assumed · recommended (S3).
    pub memory: crate::memory::MemoryService,
    /// Consents per category and world (S11-T4, pulled forward to team sprint A).
    pub consents: crate::memory::consent::ConsentStore,
    /// Tasks shared by Work Productivity and Home Personal Productivity (S4-T3).
    pub tasks: crate::tools::tasks::TaskStore,
    pub session_service: SharedSessionService,
    pub auth: Option<Arc<crate::auth::AuthState>>,
    pub awp: Arc<crate::awp_gate::AwpGate>,
    pub event_service: Arc<InMemoryEventSubscriptionService>,
    pub artifact_dir: PathBuf,
    pub scenario_flags: ScenarioLiveFlags,
    pub coordinator_enabled: bool,
    pub deck_mcp: Option<Arc<McpPool>>,
    pub morning_mcp: Option<Arc<MorningMcpPool>>,
    pub live_mcp: Option<Arc<LiveMcpPool>>,
    pub people_mcp: Option<Arc<PeopleMcpPool>>,
    pub week_mcp: Option<Arc<WeekMcpPool>>,
    pub lisbon_mcp: Option<Arc<LisbonMcpPool>>,
    pub ambient: AmbientStore,
    pub ambient_enabled: bool,
    pub greeting_runner: Option<Arc<Runner>>,
    pub brand_greeting_body: String,
    pub brand_tone: Option<String>,
    pub voice: crate::voice::VoiceState,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime: RuntimeStatus,
        sessions: SessionStore,
        artifact_dir: PathBuf,
        scenario_flags: ScenarioLiveFlags,
        coordinator_enabled: bool,
        deck_runner: Option<Arc<Runner>>,
        combine_runner: Option<Arc<Runner>>,
        morning_runner: Option<Arc<Runner>>,
        live_runner: Option<Arc<Runner>>,
        people_runner: Option<Arc<Runner>>,
        week_runner: Option<Arc<Runner>>,
        lisbon_runner: Option<Arc<Runner>>,
        router_runner: Option<Arc<Runner>>,
        suzy_runner: Option<Arc<Runner>>,
        session_service: SharedSessionService,
        auth: Option<Arc<crate::auth::AuthState>>,
        awp: Arc<crate::awp_gate::AwpGate>,
        event_service: Arc<InMemoryEventSubscriptionService>,
        deck_mcp: Option<Arc<McpPool>>,
        morning_mcp: Option<Arc<MorningMcpPool>>,
        live_mcp: Option<Arc<LiveMcpPool>>,
        people_mcp: Option<Arc<PeopleMcpPool>>,
        week_mcp: Option<Arc<WeekMcpPool>>,
        lisbon_mcp: Option<Arc<LisbonMcpPool>>,
        ambient: AmbientStore,
        ambient_enabled: bool,
        greeting_runner: Option<Arc<Runner>>,
        brand_greeting_body: String,
        brand_tone: Option<String>,
        voice: crate::voice::VoiceState,
    ) -> Self {
        Self {
            runtime,
            sessions,
            deck_runner,
            combine_runner,
            morning_runner,
            live_runner,
            people_runner,
            week_runner,
            lisbon_runner,
            router_runner,
            suzy_runner,
            mother_runner: None,
            ledger: crate::permissions::gate::services().ledger.clone(),
            permissions: crate::permissions::gate::services().permissions.clone(),
            pending: crate::permissions::gate::services().pending.clone(),
            audit: crate::permissions::gate::services().audit.clone(),
            memory: crate::memory::service_handle().clone(),
            consents: crate::memory::consent::handle().clone(),
            tasks: crate::tools::tasks::store_handle().clone(),
            session_service,
            auth,
            awp,
            event_service,
            artifact_dir,
            scenario_flags,
            coordinator_enabled,
            deck_mcp,
            morning_mcp,
            live_mcp,
            people_mcp,
            week_mcp,
            lisbon_mcp,
            ambient,
            ambient_enabled,
            greeting_runner,
            brand_greeting_body,
            brand_tone,
            voice,
        }
    }
}