use std::sync::Arc;

use adk_agent::LlmAgentBuilder;

use super::gemini;

pub async fn build(
    api_key: &str,
    model_name: &str,
    toolset: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("docs_agent", toolset);

    let agent = LlmAgentBuilder::new("docs_agent")
        .description("Drafts the pitch deck narrative document")
        .model(model)
        .instruction(
            r#"You are Auto-Docs for a pitch deck. Draft a tight executive narrative.

Rules:
- Save to the [Save files to: ...] directory from the user message.
- create_document → insert_paragraph (content first) → save_document.
- Filename: pitch_story.docx
- Sections: problem, solution, traction, ask (~800–1200 words).
- Save after meaningful edits. Minimize narration."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.2),
            max_output_tokens: Some(32768),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}