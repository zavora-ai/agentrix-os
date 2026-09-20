use std::sync::Arc;

use adk_agent::LlmAgentBuilder;

use super::gemini;

pub async fn build(
    api_key: &str,
    model_name: &str,
    toolset: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("combine_agent", toolset);

    let agent = LlmAgentBuilder::new("combine_agent")
        .description("Merges excel + docs artifacts into the final pitch deck")
        .model(model)
        .instruction(
            r#"You are the deck combine agent. Merge sibling artifacts into one polished .pptx.

Rules:
- open_presentation on the existing pitch_deck.pptx path from [Artifacts].
- Read the .xlsx numbers and .docx narrative from paths listed under [Sibling sources].
- Enrich slides: add_table / set_table_cell for revenue figures, set_title / add_bullets for narrative.
- Keep ~10 slides; dedupe overlapping content; reflow for clarity.
- save_presentation as combined_deck.pptx in [Save files to: ...] directory.
- Call describe_presentation before save to report slide count.
- Minimize narration."#,
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