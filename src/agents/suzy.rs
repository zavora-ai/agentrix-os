//! Suzy coordinator — dynamic HTML summaries from session context.

use std::sync::Arc;

use adk_agent::LlmAgentBuilder;
use adk_core::{Content, SessionId, UserId};
use adk_model::gemini::GeminiModel;
use adk_runner::Runner;
use futures::StreamExt;

use crate::state::SessionRecord;

pub async fn build(api_key: &str, model_name: &str) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = Arc::new(GeminiModel::new(api_key, model_name)?);

    let agent = LlmAgentBuilder::new("suzy_coordinator")
        .description("Summarize completed scenario work for the user")
        .model(model)
        .instruction(
            r#"You are Suzy — warm, confident, quietly witty voice of Agentrix OS.

Given session context (scenario, cards, artifacts), write a 2–3 sentence summary in HTML.
Use <b> for emphasis on key numbers, names, and next actions.
Be concise and actionable. Do not use markdown.
Output ONLY the HTML content (no wrapper tags like <p>)."#,
        )
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.35),
            max_output_tokens: Some(512),
            ..Default::default()
        })
        .build()?;

    Ok(Arc::new(agent))
}

pub fn session_context(record: &SessionRecord) -> String {
    let mut lines = vec![
        format!(
            "Scenario: {}",
            record.scenario.as_deref().unwrap_or("unknown")
        ),
        format!(
            "Original intent: {}",
            record.origin_text.as_deref().unwrap_or("")
        ),
    ];

    for card in &record.cards {
        if card.removed {
            continue;
        }
        let title = card
            .card
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("card");
        let status = card.status.as_str();
        if let Some(resolve) = &card.resolve {
            lines.push(format!("{title} ({status}): {resolve}"));
        } else {
            lines.push(format!("{title} ({status})"));
        }
    }

    let a = &record.artifacts;
    if a.xlsx.is_some() {
        lines.push("Excel artifact ready".into());
    }
    if a.docx.is_some() {
        lines.push("Document artifact ready".into());
    }
    if a.pptx.is_some() {
        lines.push("Slides artifact ready".into());
    }
    if a.combined_pptx.is_some() {
        lines.push("Combined deck artifact ready".into());
    }

    lines.join("\n")
}

fn text_from_parts(content: Option<Content>) -> String {
    content
        .map(|c| {
            c.parts
                .iter()
                .filter_map(|p| p.text().map(str::to_string))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Generate a dynamic Suzy summary. Returns plain HTML string.
pub async fn summarize(
    runner: &Runner,
    user_id: &str,
    session_id: &str,
    record: &SessionRecord,
) -> anyhow::Result<String> {
    let context = session_context(record);
    let prompt = format!(
        "Summarize this completed session for the user:\n\n{context}\n\nWrite the HTML summary now."
    );

    crate::agents::ensure_runner_session(runner, user_id, session_id).await;
    let mut stream = runner
        .run(
            UserId::try_from(user_id)?,
            SessionId::try_from(session_id)?,
            Content::new("user").with_text(&prompt),
        )
        .await?;

    let mut html = String::new();
    while let Some(chunk) = stream.next().await {
        let event = chunk?;
        if event.author.contains("suzy") || event.author == "model" || !event.author.is_empty() {
            let text = text_from_parts(event.llm_response.content);
            if !text.is_empty() {
                html = text;
            }
        }
    }

    let trimmed = html.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!("empty suzy summary");
    }
    Ok(trimmed)
}