//! Gemini-compatible tool schema sanitizer (from docx-agent-app).

use std::sync::Arc;

pub fn wrap_toolset(toolset: Arc<dyn adk_core::Toolset>) -> Arc<dyn adk_core::Toolset> {
    Arc::new(GeminiSanitizedToolset(toolset))
}

struct GeminiSanitizedToolset(Arc<dyn adk_core::Toolset>);

#[async_trait::async_trait]
impl adk_core::Toolset for GeminiSanitizedToolset {
    fn name(&self) -> &str {
        self.0.name()
    }

    async fn tools(
        &self,
        ctx: Arc<dyn adk_core::ReadonlyContext>,
    ) -> adk_core::Result<Vec<Arc<dyn adk_core::Tool>>> {
        Ok(self
            .0
            .tools(ctx)
            .await?
            .into_iter()
            .map(|t| Arc::new(SanitizedTool(t)) as Arc<dyn adk_core::Tool>)
            .collect())
    }
}

struct SanitizedTool(Arc<dyn adk_core::Tool>);

#[async_trait::async_trait]
impl adk_core::Tool for SanitizedTool {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn description(&self) -> &str {
        self.0.description()
    }
    fn parameters_schema(&self) -> Option<serde_json::Value> {
        self.0.parameters_schema().map(|mut v| {
            deep_sanitize_for_gemini(&mut v);
            v
        })
    }
    async fn execute(
        &self,
        ctx: Arc<dyn adk_core::ToolContext>,
        args: serde_json::Value,
    ) -> adk_core::Result<serde_json::Value> {
        self.0.execute(ctx, args).await
    }
}

fn deep_sanitize_for_gemini(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for k in ["$defs", "$ref", "$schema", "definitions", "additionalProperties", "title"]
            {
                map.remove(k);
            }
            if let Some(one_of) = map.remove("oneOf").or_else(|| map.remove("anyOf")) {
                if let serde_json::Value::Array(variants) = one_of {
                    for variant in variants {
                        if let serde_json::Value::Object(v) = variant {
                            if v.get("type").and_then(|t| t.as_str()) != Some("null") {
                                for (k, val) in v {
                                    if k != "const" {
                                        map.insert(k, val);
                                    }
                                }
                                break;
                            }
                        }
                    }
                    map.entry("type").or_insert(serde_json::json!("string"));
                }
            }
            if let Some(serde_json::Value::Array(types)) = map.get("type").cloned() {
                let first = types.iter().find(|t| t.as_str() != Some("null")).cloned();
                map.insert("type".into(), first.unwrap_or(serde_json::json!("string")));
            }
            map.remove("const");
            if map.get("items").is_some_and(|i| i.is_boolean()) {
                map.insert("items".into(), serde_json::json!({"type": "string"}));
            }
            for (_, v) in map.iter_mut() {
                deep_sanitize_for_gemini(v);
            }
            if map.contains_key("required") {
                let prop_keys: Vec<String> = map
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|o| o.keys().cloned().collect())
                    .unwrap_or_default();
                match map.get_mut("required") {
                    Some(serde_json::Value::Array(req)) if !prop_keys.is_empty() => {
                        req.retain(|r| {
                            r.as_str()
                                .is_some_and(|s| prop_keys.iter().any(|k| k == s))
                        });
                    }
                    _ => {
                        map.remove("required");
                    }
                }
            }
        }
        serde_json::Value::Array(arr) => arr.iter_mut().for_each(deep_sanitize_for_gemini),
        _ => {}
    }
}

pub fn filtered(
    toolset: Arc<dyn adk_core::Toolset>,
    allowed: &[&str],
) -> Arc<dyn adk_core::Toolset> {
    Arc::new(adk_tool::FilteredToolset::new(
        toolset,
        adk_tool::string_predicate(allowed.iter().map(|s| (*s).to_string()).collect()),
    ))
}

/// Apply the MCP Registry allowlist for `agent` from `mcp_allowlists.toml`, then the
/// permission gate (ADR-003): every tool call is checked against the agent's mode and the
/// tool's effect class before it reaches MCP.
pub fn filtered_for_agent(
    agent: &str,
    toolset: Arc<dyn adk_core::Toolset>,
) -> Arc<dyn adk_core::Toolset> {
    let allowed = crate::tools::allowlist::tools_for_agent(agent);
    let refs: Vec<&str> = allowed.iter().map(|s| s.as_str()).collect();
    let filtered = filtered(wrap_toolset(toolset), &refs);
    let gated = crate::permissions::PermissionGate::wrap(agent, filtered);
    // Every agent can read memory within its world and propose assumed items (S3-T3), through
    // the same gate: `propose_memory` is write_local, so observe denies it and pause blocks it.
    let memory = crate::permissions::PermissionGate::wrap(
        agent,
        crate::memory::tools::MemoryTools::for_agent(agent),
    );
    let mut parts: Vec<Arc<dyn adk_core::Toolset>> = vec![gated, memory];
    // Built-in toolsets are opted into per agent through a pseudo `mcp_server` entry in the
    // allowlist, so their tools carry effects and pass the same gate (S4-T3: `tasks`).
    let spec = crate::tools::allowlist::catalog().spec_for(agent);
    if spec.is_some_and(|s| s.mcp_servers.iter().any(|m| m == crate::tools::tasks::TOOLSET_ID)) {
        parts.push(crate::permissions::PermissionGate::wrap(
            agent,
            crate::tools::tasks::TasksTools::for_agent(agent),
        ));
    }
    crate::tools::merge::MergedToolset::new(parts)
}