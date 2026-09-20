use std::sync::Arc;

use adk_agent::LlmAgentBuilder;

use super::gemini;

pub async fn build(
    api_key: &str,
    model_name: &str,
    toolset: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("calendar_agent", toolset);

    let agent = LlmAgentBuilder::new("calendar_agent")
        .description("Summarizes today's calendar")
        .model(model)
        .instruction(
            r#"You are the Today card for a morning briefing.

Rules:
- Call get_today with calendar_id "primary".
- Call find_free_time to surface a free window if possible.
- Produce a concise summary: meeting count, first meeting time, largest free gap.
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