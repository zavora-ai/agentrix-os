//! Greeting agent — composes prose strictly from integration facts (no fabrication).

use std::sync::Arc;

use adk_agent::LlmAgentBuilder;
use adk_core::{Content, SessionId, UserId};
use adk_runner::Runner;
use futures::StreamExt;

use super::context::GreetingSnapshot;

pub async fn build(api_key: &str, model_name: &str) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;

    let agent = LlmAgentBuilder::new("greeting_agent")
        .description("Composes a personalized one- or two-sentence greeting from verified integration facts")
        .model(model)
        .instruction(
            r#"You are Suzy — the voice of Agentrix OS at session open.

You receive INTEGRATION_FACTS as JSON. Write exactly 1–2 sentences of plain text (no HTML, no markdown).

Rules:
- ONLY mention facts present in the JSON. Never invent meetings, emails, weather, or headlines.
- If a section is null or empty, omit it entirely.
- Warm, confident, quietly witty. No filler, no exclamation spam.
- Do not mention integrations, APIs, MCP, or JSON.
- Output ONLY the greeting body — no salutation prefix (the system adds time-of-day salutation)."#,
        )
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.25),
            max_output_tokens: Some(256),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}

pub async fn compose_from_snapshot(
    runner: &Runner,
    snapshot: &GreetingSnapshot,
    tone: Option<&str>,
) -> anyhow::Result<String> {
    let facts = serde_json::to_string(snapshot)?;
    let tone_line = tone
        .map(|t| format!("Brand tone: {t}\n"))
        .unwrap_or_default();
    let prompt = format!(
        "{tone_line}INTEGRATION_FACTS:\n{facts}\n\nWrite the greeting body now (no salutation prefix)."
    );

    let mut stream = runner
        .run(
            UserId::try_from("greeting-user")?,
            SessionId::try_from("greeting-session")?,
            Content::new("user").with_text(&prompt),
        )
        .await?;

    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        let event = chunk?;
        if let Some(content) = event.content() {
            for part in &content.parts {
                if let Some(t) = part.text() {
                    text.push_str(t);
                }
            }
        }
    }

    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!("greeting agent returned empty text");
    }
    Ok(trimmed)
}