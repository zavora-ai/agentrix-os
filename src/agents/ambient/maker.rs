use std::sync::Arc;

use adk_agent::LlmAgentBuilder;
use adk_model::gemini::GeminiModel;

pub async fn build(api_key: &str, model_name: &str) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = Arc::new(GeminiModel::new(api_key, model_name)?);

    let agent = LlmAgentBuilder::new("maker_agent")
        .description("Background creative maker")
        .model(model)
        .instruction(
            r#"You are a background maker agent for Agentrix OS.
Invent 3 small delightful outputs the user might like: e.g. weekend playlist theme,
3 logo sketch ideas, a short trip shortlist. One line each."#,
        )
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.7),
            max_output_tokens: Some(512),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}