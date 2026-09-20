use std::sync::Arc;

use adk_agent::LlmAgentBuilder;

use crate::agents::gemini;

pub async fn build(
    api_key: &str,
    model_name: &str,
    news: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("research_agent", news);

    let agent = LlmAgentBuilder::new("research_agent")
        .description("Background research on topics of interest")
        .model(model)
        .instruction(
            r#"You are a background research agent for Agentrix OS.
Research ABC Corp (or trending tech company) using news tools.
Output a concise brief: company, funding, team size, key risks — 4-6 sentences."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.3),
            max_output_tokens: Some(2048),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}