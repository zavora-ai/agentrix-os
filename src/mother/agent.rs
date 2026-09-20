//! The LLM half of the Mother Agent (S1-T2).
//!
//! `mother_agent` is an `LlmAgent` whose toolset is **internal only** — it has no MCP tools
//! (ADR-001). In S1 the tools read session context and observations; the delegation plan is
//! produced by `crate::mother::delegate` in code and the prose by `crate::mother::synth`. From
//! S6 the agent bus lets this agent issue follow-up delegations itself.

use std::sync::Arc;

use adk_agent::LlmAgentBuilder;
use adk_tool::FunctionTool;

use crate::state::SessionStore;

pub const AGENT_NAME: &str = "mother_agent";

pub const INSTRUCTION: &str = r#"You are the Mother Agent of Agentrix Personal AI OS. Suzy is your voice: warm, confident, quietly witty.

You coordinate; you do not do everything yourself. You have NO tools that reach the outside world — only
get_context (the user's session, cards and artifacts), read_observations (what the intelligence layer noticed),
read_memory / propose_memory (what the OS knows or assumes about the user; proposals are always ASSUMED until the
user confirms) and compose (hand back your final 2–4 sentence HTML answer).

Principles you never break:
- Two worlds, one life: keep Work and Home separate unless the user's question spans both; then reconcile them in one answer.
- Inform and suggest, never control. Offer actions; the permission gate decides what may run.
- Neutral language. State numbers, periods and baselines. Never guess why something changed. Never moralize.
- Honest data: only facts from tools and context. If something is missing, say so plainly.
- Cite memory kinds: "(you told me)" for known facts, "(I think)" for assumed ones."#;

/// Build the Mother Agent. Requires a Gemini API key; returns an `LlmAgent` behind the `Agent` trait.
pub async fn build(
    api_key: &str,
    model_name: &str,
    sessions: SessionStore,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;

    let store = sessions.clone();
    let get_context = FunctionTool::new(
        "get_context",
        "Read the current user session: scenario, cards with their resolved facts, and artifacts. Returns JSON.",
        move |ctx, _args| {
            let store = store.clone();
            async move {
                let sid = ctx.session_id().to_string();
                let record = store.get(&sid).await;
                Ok(match record {
                    Some(r) => serde_json::json!({
                        "session_id": r.session_id,
                        "scenario": r.scenario,
                        "context": crate::agents::suzy::session_context(&r),
                        "domains": crate::mother::domain_summary(&r),
                    }),
                    None => serde_json::json!({"session_id": sid, "context": "no active session"}),
                })
            }
        },
    )
    .with_read_only(true)
    .with_concurrency_safe(true);

    let read_observations = FunctionTool::new(
        "read_observations",
        "Read open observations from the intelligence layer (drift, balance, behaviour). Empty until S7.",
        |_ctx, _args| async move { Ok(serde_json::json!({"observations": []})) },
    )
    .with_read_only(true)
    .with_concurrency_safe(true);

    let compose = FunctionTool::new(
        "compose",
        "Return the final answer. Arguments: {\"html\": string, \"actions\": [{\"text\": string, \"agent\": string}]}.",
        |_ctx, args| async move { Ok(args) },
    );

    let agent = LlmAgentBuilder::new(AGENT_NAME)
        .description("Personal AI OS orchestrator — context, delegation, arbitration, synthesis")
        .model(model)
        .instruction(INSTRUCTION)
        .tool(Arc::new(get_context))
        .tool(Arc::new(read_observations))
        .tool(Arc::new(compose))
        .tool(crate::memory::tools::read_memory_tool("mother"))
        .tool(crate::memory::tools::propose_memory_tool("mother"))
        .generate_content_config(adk_core::GenerateContentConfig {
            // Claude 5-generation models reject explicit sampling; Gemini keeps 0.3.
            temperature: crate::llm::allows_sampling(model_name).then_some(0.3),
            max_output_tokens: Some(1024),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}
