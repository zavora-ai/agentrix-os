//! LLM intent router — classifies natural language into scenario keys.

use std::sync::Arc;

use adk_agent::{CustomAgentBuilder, LlmConditionalAgent};
use adk_core::{Content, Event, Part};
use adk_runner::Runner;
use adk_core::{SessionId, UserId};
use futures::{stream, StreamExt};

pub const SCENARIOS: &[&str] = &[
    "deck", "morning", "lisbon", "week", "people", "live", "proactive",
];

#[derive(Debug, Clone)]
pub enum ClassifyOutcome {
    Scenario(String),
    Clarify(String),
}

fn scenario_stub(key: &'static str) -> Arc<dyn adk_core::Agent> {
    Arc::new(
        CustomAgentBuilder::new(key)
            .description(format!("Route to {key} scenario"))
            .handler(move |_ctx| {
                let key = key.to_string();
                async move {
                    let mut event = Event::new("route");
                    event.author = key.clone();
                    event.llm_response.content =
                        Some(Content::new("assistant").with_text(key));
                    Ok(Box::pin(stream::iter(vec![Ok(event)])) as adk_core::EventStream)
                }
            })
            .build()
            .expect("scenario stub"),
    )
}

fn clarify_stub() -> Arc<dyn adk_core::Agent> {
    const MSG: &str = "I'm not sure which flow you want. Try <b>Start my day</b>, <b>Build me a pitch deck</b>, or <b>Plan a trip to Lisbon</b>.";
    Arc::new(
        CustomAgentBuilder::new("clarify")
            .description("Ask a clarifying question when intent is ambiguous")
            .handler(move |_ctx| async move {
                let mut event = Event::new("clarify");
                event.author = "clarify".to_string();
                event.llm_response.content =
                    Some(Content::new("assistant").with_text(MSG));
                Ok(Box::pin(stream::iter(vec![Ok(event)])) as adk_core::EventStream)
            })
            .build()
            .expect("clarify stub"),
    )
}

pub async fn build(api_key: &str, model_name: &str) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;

    let mut builder = LlmConditionalAgent::builder("intent_router", model)
        .description("Routes user intent to an Agentrix scenario")
        .instruction(
            r#"Classify the user's intent into exactly one scenario:
- deck: pitch decks, presentations, slides, spreadsheets, documents, reports
- morning: start my day, morning routine, calendar and inbox briefing
- lisbon: travel, trips, flights, hotels, itineraries, vacations
- week: weekly summary, spending, health, productivity recap
- people: team, family, connections, catch up on people
- live: news, markets, what's happening now, headlines
- proactive: background work, things found while away, research results

Respond with ONLY the scenario name (one word from the list above).
If the intent is ambiguous, too vague, or unrelated, respond with "clarify"."#,
        );

    for key in SCENARIOS {
        builder = builder.route(*key, scenario_stub(key));
    }

    let router = builder.default_route(clarify_stub()).build()?;
    Ok(Arc::new(router))
}

fn normalize_scenario(label: &str) -> Option<String> {
    let trimmed = label.trim().to_lowercase();
    SCENARIOS
        .iter()
        .find(|s| trimmed == **s || trimmed.contains(*s))
        .map(|s| (*s).to_string())
}

fn text_from_event(event: &Event) -> String {
    event
        .llm_response
        .content
        .as_ref()
        .map(|c| {
            c.parts
                .iter()
                .filter_map(|p| match p {
                    Part::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Classify intent via the LLM router runner. Falls back to keyword routing on failure.
pub async fn classify(
    runner: &Runner,
    user_id: &str,
    session_id: &str,
    text: &str,
    keyword_fallback: impl FnOnce(&str) -> &'static str,
) -> ClassifyOutcome {
    let prompt = format!("Classify this intent:\n\n{text}");
    crate::agents::ensure_runner_session(runner, user_id, session_id).await;
    let mut stream = match runner
        .run(
            UserId::try_from(user_id).unwrap_or_else(|_| UserId::try_from("router").unwrap()),
            SessionId::try_from(session_id).unwrap_or_else(|_| {
                SessionId::try_from("router-session").unwrap()
            }),
            Content::new("user").with_text(&prompt),
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("intent router unavailable ({e}); using keyword fallback");
            return ClassifyOutcome::Scenario(keyword_fallback(text).into());
        }
    };

    let mut clarify_message = None;

    while let Some(result) = stream.next().await {
        let event = match result {
            Ok(ev) => ev,
            Err(e) => {
                tracing::warn!("intent router stream failed ({e}); using keyword fallback");
                return ClassifyOutcome::Scenario(keyword_fallback(text).into());
            }
        };

        let body = text_from_event(&event);
        if body.starts_with("[Routing to:") {
            let class = body
                .trim_start_matches("[Routing to:")
                .trim_end_matches(']')
                .trim()
                .to_lowercase();
            if class == "clarify" {
                continue;
            }
            if let Some(scenario) = normalize_scenario(&class) {
                return ClassifyOutcome::Scenario(scenario);
            }
        }

        if event.author == "clarify" {
            clarify_message = Some(body);
        } else if let Some(scenario) = normalize_scenario(&event.author) {
            return ClassifyOutcome::Scenario(scenario);
        }
    }

    if let Some(msg) = clarify_message.filter(|m| !m.is_empty()) {
        return ClassifyOutcome::Clarify(msg);
    }

    ClassifyOutcome::Scenario(keyword_fallback(text).into())
}