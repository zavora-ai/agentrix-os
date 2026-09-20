use std::sync::Arc;

use adk_agent::LlmAgentBuilder;

use super::gemini;

pub async fn build(
    api_key: &str,
    model_name: &str,
    toolset: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("inbox_agent", toolset);

    let agent = LlmAgentBuilder::new("inbox_agent")
        .description("Triages inbox and flags emails that need the user")
        .model(model)
        .instruction(
            r#"You are the Needs you card for a morning briefing.

Rules:
- list_inbox to scan recent messages.
- search_emails for is:unread OR important threads if needed.
- Surface only 1–3 emails that truly need a reply; note subjects/senders.
- Minimize narration — use tools first."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.2),
            max_output_tokens: Some(8192),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}