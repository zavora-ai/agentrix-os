//! MCP Registry-compatible allowlist catalog (BK-011) and Phase 2 agent contract (S0-T5).
//!
//! `mcp_allowlists.toml` is the version-controlled source of truth; optional
//! `MCP_REGISTRY_PATH` syncs entries to the registry MCP at boot.
//!
//! Phase 2 extends each `[[allowlist]]` entry — backward compatibly — with the agent's
//! `world` (work | home | shared), its default `mode` (observe | suggest | automate) and an
//! `[allowlist.effects]` table mapping every tool to an effect class (ADR-003). Missing
//! `mode` → `suggest`; missing `world` → `shared`; missing effects are reported by
//! [`AllowlistCatalog::tools_missing_effects`] (a warning in S0, a boot failure from S2).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::Context;
use serde::Deserialize;

use crate::domain::Domain;
use crate::permissions::{Effect, Mode};

static CATALOG: OnceLock<AllowlistCatalog> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
struct AllowlistFile {
    allowlist: Vec<AllowlistEntry>,
    /// Effect class per built-in tool every agent gets in-process (`read_memory`, …).
    #[serde(default)]
    builtin_effects: HashMap<String, Effect>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllowlistEntry {
    pub agent: String,
    pub mcp_server: String,
    #[serde(default)]
    pub reason: String,
    pub tools: Vec<String>,
    /// Life domain the agent belongs to (ADR-002). Default `shared`.
    #[serde(default)]
    pub world: Option<Domain>,
    /// Default authority level (ADR-003). Default `suggest`.
    #[serde(default)]
    pub mode: Option<Mode>,
    /// Effect class per tool (ADR-003). Keys must be a subset of `tools`.
    #[serde(default)]
    pub effects: HashMap<String, Effect>,
}

/// The declarative half of the agent contract (concept §6.1), derived from all allowlist
/// entries that share an `agent` id.
#[derive(Debug, Clone)]
pub struct AgentSpec {
    pub id: String,
    pub world: Domain,
    pub mode: Mode,
    pub mcp_servers: Vec<String>,
    /// tool name → effect (None when the entry has not classified it yet)
    pub tools: BTreeMap<String, Option<Effect>>,
}

impl AgentSpec {
    pub fn effect_for(&self, tool: &str) -> Option<Effect> {
        self.tools.get(tool).copied().flatten()
    }
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
    pub fn missing_effects(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|(_, e)| e.is_none())
            .map(|(t, _)| t.clone())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AllowlistCatalog {
    by_agent: HashMap<String, HashSet<String>>,
    specs: HashMap<String, AgentSpec>,
    entries: Vec<AllowlistEntry>,
    builtin: HashMap<String, Effect>,
}

/// Built-in tools mounted on every agent outside the MCP allowlist; each needs an entry in the
/// catalog's `[builtin_effects]` table.
pub const BUILTIN_TOOLS: &[&str] = crate::memory::tools::TOOL_NAMES;

impl AllowlistCatalog {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read allowlist catalog {}", path.display()))?;
        let file: AllowlistFile = toml::from_str(&raw)
            .with_context(|| format!("parse allowlist catalog {}", path.display()))?;
        Ok(Self::from_entries(file.allowlist).with_builtin_effects(file.builtin_effects))
    }

    /// Classify the built-in tools (`[builtin_effects]`).
    pub fn with_builtin_effects(mut self, builtin: HashMap<String, Effect>) -> Self {
        self.builtin = builtin;
        self
    }

    pub fn from_entries(entries: Vec<AllowlistEntry>) -> Self {
        let mut by_agent: HashMap<String, HashSet<String>> = HashMap::new();
        let mut specs: HashMap<String, AgentSpec> = HashMap::new();
        for entry in &entries {
            by_agent
                .entry(entry.agent.clone())
                .or_default()
                .extend(entry.tools.iter().cloned());
            let spec = specs.entry(entry.agent.clone()).or_insert_with(|| AgentSpec {
                id: entry.agent.clone(),
                world: entry.world.unwrap_or_default(),
                mode: entry.mode.unwrap_or_default(),
                mcp_servers: Vec::new(),
                tools: BTreeMap::new(),
            });
            if let Some(w) = entry.world {
                spec.world = w;
            }
            if let Some(m) = entry.mode {
                spec.mode = m;
            }
            if !spec.mcp_servers.contains(&entry.mcp_server) {
                spec.mcp_servers.push(entry.mcp_server.clone());
            }
            for tool in &entry.tools {
                let effect = entry.effects.get(tool).copied();
                let slot = spec.tools.entry(tool.clone()).or_insert(None);
                if effect.is_some() {
                    *slot = effect;
                }
            }
        }
        Self { by_agent, specs, entries, builtin: HashMap::new() }
    }

    /// Agent contract for `agent`, if the catalog knows it.
    pub fn spec_for(&self, agent: &str) -> Option<&AgentSpec> {
        self.specs.get(agent)
    }

    pub fn specs(&self) -> impl Iterator<Item = &AgentSpec> {
        self.specs.values()
    }

    /// Effect class of `tool` for `agent`, then of the built-in tool of that name (None when
    /// unclassified or unknown).
    pub fn effect_for(&self, agent: &str, tool: &str) -> Option<Effect> {
        self.specs
            .get(agent)
            .and_then(|s| s.effect_for(tool))
            .or_else(|| self.builtin.get(tool).copied())
    }

    /// Built-in tools with no `[builtin_effects]` entry.
    pub fn builtin_tools_missing_effects(&self) -> Vec<String> {
        BUILTIN_TOOLS
            .iter()
            .filter(|t| !self.builtin.contains_key(**t))
            .map(|t| t.to_string())
            .collect()
    }

    /// Default authority mode declared for `agent` (`suggest` when unknown).
    pub fn mode_for(&self, agent: &str) -> Mode {
        self.specs.get(agent).map(|s| s.mode).unwrap_or_default()
    }

    /// Life domain declared for `agent` (`shared` when unknown).
    pub fn world_for(&self, agent: &str) -> Domain {
        self.specs.get(agent).map(|s| s.world).unwrap_or_default()
    }

    /// `(agent, tool)` pairs listed without an effect class.
    pub fn tools_missing_effects(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .specs
            .values()
            .flat_map(|s| s.missing_effects().into_iter().map(move |t| (s.id.clone(), t)))
            .collect();
        out.sort();
        out
    }

    /// Effects declared for tools that are not in the entry's `tools` list are a config error.
    pub fn effects_for_unknown_tools(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for e in &self.entries {
            for tool in e.effects.keys() {
                if !e.tools.contains(tool) {
                    out.push((e.agent.clone(), tool.clone()));
                }
            }
        }
        for tool in self.builtin.keys() {
            if !BUILTIN_TOOLS.contains(&tool.as_str()) {
                out.push(("builtin_effects".to_string(), tool.clone()));
            }
        }
        out.sort();
        out
    }

    /// Validate the effect classification. `strict` turns missing effects into an error
    /// (S2 boot behaviour); otherwise they are only reported.
    pub fn validate_effects(&self, strict: bool) -> anyhow::Result<Vec<(String, String)>> {
        let unknown = self.effects_for_unknown_tools();
        anyhow::ensure!(
            unknown.is_empty(),
            "mcp_allowlists.toml declares effects for tools not in the allowlist: {unknown:?}"
        );
        let mut missing = self.tools_missing_effects();
        missing.extend(self.builtin_tools_missing_effects().into_iter().map(|t| ("builtin_effects".to_string(), t)));
        if strict && !missing.is_empty() {
            anyhow::bail!(
                "mcp_allowlists.toml: {} allowlisted tool(s) have no effect class (ADR-003): {:?}",
                missing.len(),
                missing
            );
        }
        Ok(missing)
    }

    pub fn tools_for_agent(&self, agent: &str) -> Vec<String> {
        self.by_agent
            .get(agent)
            .map(|set| {
                let mut tools: Vec<String> = set.iter().cloned().collect();
                tools.sort();
                tools
            })
            .unwrap_or_default()
    }

    pub fn agent_count(&self) -> usize {
        self.by_agent.len()
    }

    pub fn entries(&self) -> &[AllowlistEntry] {
        &self.entries
    }

    pub fn contains_agent(&self, agent: &str) -> bool {
        self.by_agent.contains_key(agent)
    }
}

pub fn init(path: &Path) -> anyhow::Result<()> {
    if CATALOG.get().is_some() {
        return Ok(());
    }
    let catalog = AllowlistCatalog::from_file(path)?;
    tracing::info!(
        "MCP allowlist catalog loaded: {} agents, {} entries",
        catalog.agent_count(),
        catalog.entries.len()
    );
    let missing = catalog.validate_effects(EFFECTS_REQUIRED)?;
    if !missing.is_empty() {
        tracing::warn!(
            "{} allowlisted tool(s) lack an effect class (ADR-003) — they will be treated as \
             send_external until classified: {:?}",
            missing.len(),
            missing
        );
    }
    let _ = CATALOG.set(catalog);
    Ok(())
}

/// Boot fails on an allowlisted tool without an effect class (S2-T4, ADR-003).
pub const EFFECTS_REQUIRED: bool = true;

/// Agent contract for `agent` from the global catalog.
pub fn spec_for(agent: &str) -> Option<AgentSpec> {
    catalog().spec_for(agent).cloned()
}

/// Effect of `tool` for `agent`; unclassified tools are treated as the most restrictive
/// non-financial effect so the gate errs on the side of asking.
pub fn effect_for(agent: &str, tool: &str) -> Effect {
    catalog()
        .effect_for(agent, tool)
        .unwrap_or(Effect::SendExternal)
}

pub fn catalog() -> &'static AllowlistCatalog {
    CATALOG.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcp_allowlists.toml");
        AllowlistCatalog::from_file(&path).expect("mcp_allowlists.toml missing or invalid")
    })
}

pub fn tools_for_agent(agent: &str) -> Vec<String> {
    catalog().tools_for_agent(agent)
}