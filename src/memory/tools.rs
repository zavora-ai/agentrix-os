//! Memory tools for agents (S3-T3): `read_memory` (scoped) and `propose_memory` (assumed only).
//!
//! Every agent gets both through `crate::agents::gemini::filtered_for_agent`, behind the
//! permission gate like any MCP tool: `[builtin_effects]` in `mcp_allowlists.toml` classifies
//! `read_memory` as read and `propose_memory` as write_local, so an observe agent cannot write
//! memory and pause stops it. The scope comes from the agent's world in `mcp_allowlists.toml`.
//! The Mother Agent reads across worlds.

use std::sync::Arc;

use adk_tool::FunctionTool;

use crate::domain::Domain;
use crate::memory::service::{NewItem, Provenance, Scope, Sensitivity};
use crate::memory::service_handle;

/// Names of the tools in [`MemoryTools`]; each needs a `[builtin_effects]` class.
pub const TOOL_NAMES: &[&str] = &["read_memory", "propose_memory"];

/// The sensitivity every proposal from `agent_id` gets at least, whatever the model picked:
/// health and finance agents' inferences are always encrypted at rest.
pub fn sensitivity_floor(agent_id: &str) -> Option<Sensitivity> {
    match agent_id {
        "health_agent" | "health_wellness" => Some(Sensitivity::Health),
        "money_agent" | "finance" => Some(Sensitivity::Financial),
        _ => None,
    }
}

/// A toolset holding the two memory tools for `agent_id`. Mount it behind
/// `crate::permissions::PermissionGate`, never bare.
pub struct MemoryTools {
    agent_id: String,
}

impl MemoryTools {
    pub fn for_agent(agent_id: &str) -> Arc<dyn adk_core::Toolset> {
        Arc::new(Self { agent_id: agent_id.to_string() })
    }
}

#[async_trait::async_trait]
impl adk_core::Toolset for MemoryTools {
    fn name(&self) -> &str {
        "memory"
    }

    async fn tools(
        &self,
        _ctx: Arc<dyn adk_core::ReadonlyContext>,
    ) -> adk_core::Result<Vec<Arc<dyn adk_core::Tool>>> {
        Ok(vec![read_memory_tool(&self.agent_id), propose_memory_tool(&self.agent_id)])
    }
}

pub fn read_memory_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(
        FunctionTool::new(
            "read_memory",
            "Read what the OS knows or assumes about the user within your scope. Optional {\"keys\": [\"profile.\", \"preference.meetings\"]} filters by key prefix. Each item carries kind = known | assumed | recommended — cite known as \"(you told me)\" and assumed as \"(I think)\".",
            move |ctx, args| {
                let agent = agent.clone();
                async move {
                    let keys: Option<Vec<String>> = args.get("keys").and_then(|k| serde_json::from_value(k.clone()).ok());
                    let items = service_handle()
                        .read(ctx.user_id(), Scope::for_agent(&agent), keys.as_deref())
                        .await;
                    Ok(serde_json::json!({
                        "scope": Scope::for_agent(&agent).domain.as_str(),
                        "items": items.iter().map(|i| serde_json::json!({
                            "id": i.id, "domain": i.domain, "key": i.key, "value": i.value,
                            "kind": i.kind, "confidence": i.confidence, "citation": i.kind.citation(),
                        })).collect::<Vec<_>>()
                    }))
                }
            },
        )
        .with_read_only(true)
        .with_concurrency_safe(true),
    )
}

pub fn propose_memory_tool(agent_id: &str) -> Arc<dyn adk_core::Tool> {
    let agent = agent_id.to_string();
    Arc::new(FunctionTool::new(
        "propose_memory",
        "Propose something you inferred about the user. It is stored as ASSUMED (never known) and shown to the user to confirm or correct. Arguments: {\"key\": \"preference.meetings.earliest_start\", \"value\": <json>, \"category\": \"preference|profile|goal|project|person|date|routine|interest|context\", \"confidence\": 0.0–1.0, \"domain\": \"work|home|shared\" (default: your world), \"sensitivity\": \"normal|sensitive|health|financial\"}",
        move |ctx, args| {
            let agent = agent.clone();
            async move {
                let scope = Scope::for_agent(&agent);
                let key = args.get("key").and_then(|k| k.as_str()).unwrap_or("").trim().to_string();
                if key.is_empty() || key.len() > 120 {
                    return Ok(serde_json::json!({"status": "rejected", "reason": "key required (≤120 chars)"}));
                }
                let domain = args
                    .get("domain")
                    .and_then(|d| d.as_str())
                    .and_then(Domain::parse)
                    .unwrap_or(scope.domain);
                if !scope.may_write(domain) {
                    return Ok(serde_json::json!({"status": "rejected", "reason": format!("agent in {} may not write {} memory", scope.domain, domain)}));
                }
                let value = args.get("value").cloned().unwrap_or(serde_json::Value::Null);
                if value.is_null() || value.to_string().len() > 2_000 {
                    return Ok(serde_json::json!({"status": "rejected", "reason": "value required (≤2000 chars)"}));
                }
                let category = args.get("category").and_then(|c| c.as_str()).unwrap_or("context");
                let confidence = args.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.6) as f32;
                let requested = args
                    .get("sensitivity")
                    .and_then(|s| s.as_str())
                    .and_then(Sensitivity::parse)
                    .unwrap_or_default();
                let sensitivity = match sensitivity_floor(&agent) {
                    Some(floor) if !requested.encrypted_at_rest() => floor,
                    _ => requested,
                };
                let sid = ctx.session_id().to_string();
                let item = service_handle()
                    .propose(
                        ctx.user_id(),
                        confidence,
                        NewItem {
                            domain,
                            category,
                            key: &key,
                            value,
                            sensitivity,
                            source_agent: &agent,
                            provenance: Provenance::new("agent_proposal").agent(&agent).session((!sid.is_empty()).then_some(sid.as_str())),
                        },
                    )
                    .await;
                Ok(serde_json::json!({
                    "status": if item.kind == crate::memory::service::Kind::Known { "kept_known" } else { "proposed" },
                    "id": item.id, "key": item.key, "kind": item.kind,
                    "message": "Stored as assumed; the user can confirm or correct it in the trust center."
                }))
            }
        },
    ))
}
