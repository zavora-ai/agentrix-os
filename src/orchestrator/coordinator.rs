//! Suzy summaries and tour suggestions.

use std::sync::Arc;

use adk_runner::Runner;
use tokio::sync::mpsc::Sender;

use crate::events::mock;
use crate::events::sse::{to_event, FieldEvent};
use crate::scenarios::tour;
use crate::state::SessionStore;


pub async fn suzy_html(
    suzy_runner: Option<&Arc<Runner>>,
    store: &SessionStore,
    session_id: &str,
    user_id: &str,
    scenario: &str,
) -> String {
    let fallback = mock::suzy_summary(scenario).to_string();

    let html = if let Some(runner) = suzy_runner {
        if let Some(record) = store.get(session_id).await {
            match crate::agents::suzy::summarize(runner, user_id, session_id, &record).await {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("Suzy summary fallback ({e:#})");
                    fallback
                }
            }
        } else {
            fallback
        }
    } else {
        fallback
    };

    html
}

pub async fn emit_suzy_and_suggest(
    tx: &Sender<Result<axum::response::sse::Event, std::convert::Infallible>>,
    suzy_runner: Option<&Arc<Runner>>,
    store: &SessionStore,
    session_id: &str,
    user_id: &str,
    scenario: &str,
) {
    let html = suzy_html(suzy_runner, store, session_id, user_id, scenario).await;

    let _ = tx
        .send(Ok(to_event(&FieldEvent::SuzySummary {
            key: scenario.into(),
            html,
        })))
        .await;

    if let Some(text) = tour::action_prompt(scenario) {
        let _ = tx
            .send(Ok(to_event(&FieldEvent::Suggest {
                text: text.into(),
                kind: "action".into(),
            })))
            .await;
    }
}

pub async fn emit_tour_advance(
    tx: &Sender<Result<axum::response::sse::Event, std::convert::Infallible>>,
    current: &str,
) {
    if let Some(next) = tour::next_scenario(current) {
        if let Some(text) = tour::scenario_prompt(next) {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::Suggest {
                    text: text.into(),
                    kind: "scenario".into(),
                })))
                .await;
        }
    }
}