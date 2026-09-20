use std::convert::Infallible;

use axum::response::sse::Sse;
use axum::response::{IntoResponse, Response};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::events::mock;
use crate::events::sse::{to_event, FieldEvent};
use crate::orchestrator::{combine, deck, lisbon, live, morning, people, proactive, week};
use crate::scenarios;
use crate::scenarios::tour;
use crate::state::AppState;

pub struct IntentDispatch<'a> {
    pub state: &'a AppState,
    pub session_id: String,
    pub user_id: String,
    pub text: String,
}

pub struct ActionDispatch<'a> {
    pub state: &'a AppState,
    pub session_id: String,
    pub user_id: String,
    pub text: String,
    pub action: &'static str,
    pub scenario: Option<String>,
}

pub fn stream_clarify(message: String) -> Response {
    let (tx, rx) = mpsc::channel::<Result<axum::response::sse::Event, Infallible>>(8);
    tokio::spawn(async move {
        let _ = tx
            .send(Ok(to_event(&FieldEvent::SuzySummary {
                key: "clarify".into(),
                html: message,
            })))
            .await;
        for key in ["morning", "deck", "lisbon"] {
            if let Some(text) = tour::scenario_prompt(key) {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Suggest {
                        text: text.into(),
                        kind: "scenario".into(),
                    })))
                    .await;
            }
        }
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });
    Sse::new(ReceiverStream::new(rx)).into_response()
}

/// Every entry point (intent, chat, voice, `/awp/a2a`) goes through the Mother Agent (ADR-001).
pub async fn dispatch_intent(req: IntentDispatch<'_>) -> Response {
    crate::mother::handle_intent(crate::mother::MotherRequest {
        state: req.state,
        session_id: req.session_id,
        user_id: req.user_id,
        text: req.text,
        entry: crate::mother::Entry::Intent,
    })
    .await
}

/// Stream one Phase 1 scenario workflow (live runner when enabled, mock otherwise).
///
/// `persist = false` runs the workflow without touching the session store; the Mother Agent
/// uses that for multi-target fan-out and persists the merged, re-indexed cards itself.
pub async fn stream_scenario(
    state: &AppState,
    scenario: &str,
    session_id: String,
    user_id: String,
    text: String,
    persist: bool,
) -> Response {
    let sessions = if persist { Some(state.sessions.clone()) } else { None };
    let scenario = scenario.to_string();
    if scenarios::intent_is_live(&scenario, state.scenario_flags) {
        let suzy = state.suzy_runner.clone();
        match scenario.as_str() {
            "deck" => {
                if let Some(runner) = state.deck_runner.clone() {
                    return Sse::new(deck::stream_deck(
                        runner,
                        user_id.clone(),
                        session_id.clone(),
                        text.clone(),
                        state.artifact_dir.clone(),
                        sessions,
                        suzy,
                    ))
                    .into_response();
                }
            }
            "morning" => {
                if let Some(runner) = state.morning_runner.clone() {
                    let has_calendar = state
                        .morning_mcp
                        .as_ref()
                        .is_some_and(|p| p.calendar.is_some());
                    let has_inbox = state
                        .morning_mcp
                        .as_ref()
                        .is_some_and(|p| p.email.is_some());
                    return Sse::new(morning::stream_morning(
                        runner,
                        user_id.clone(),
                        session_id.clone(),
                        text.clone(),
                        sessions,
                        has_calendar,
                        has_inbox,
                        suzy,
                    ))
                    .into_response();
                }
            }
            "live" => {
                if let Some(runner) = state.live_runner.clone() {
                    let has_market = state
                        .live_mcp
                        .as_ref()
                        .is_some_and(|p| p.market_data.is_some());
                    return Sse::new(live::stream_live(
                        runner,
                        user_id.clone(),
                        session_id.clone(),
                        text.clone(),
                        has_market,
                        sessions,
                        suzy,
                    ))
                    .into_response();
                }
            }
            "people" => {
                if let Some(runner) = state.people_runner.clone() {
                    let pool = state.people_mcp.as_ref();
                    return Sse::new(people::stream_people(
                        runner,
                        user_id.clone(),
                        session_id.clone(),
                        text.clone(),
                        pool.is_some_and(|p| p.slack.is_some()),
                        pool.is_some_and(|p| p.crm.is_some()),
                        pool.is_some_and(|p| p.calendar.is_some()),
                        sessions,
                        suzy,
                    ))
                    .into_response();
                }
            }
            "week" => {
                if let Some(runner) = state.week_runner.clone() {
                    let pool = state.week_mcp.as_ref();
                    return Sse::new(week::stream_week(
                        runner,
                        user_id.clone(),
                        session_id.clone(),
                        text.clone(),
                        pool.is_some_and(|p| p.banking.is_some()),
                        pool.is_some_and(|p| p.github.is_some()),
                        pool.is_some_and(|p| {
                            p.health_csv
                                .as_ref()
                                .is_some_and(|path| path.exists())
                        }),
                        sessions,
                        suzy,
                    ))
                    .into_response();
                }
            }
            "lisbon" => {
                if let Some(runner) = state.lisbon_runner.clone() {
                    let pool = state.lisbon_mcp.as_ref();
                    return Sse::new(lisbon::stream_lisbon(
                        runner,
                        user_id.clone(),
                        session_id.clone(),
                        text.clone(),
                        pool.is_some_and(|p| p.maps.is_some()),
                        pool.is_some_and(|p| p.real_estate.is_some()),
                        sessions,
                        suzy,
                    ))
                    .into_response();
                }
            }
            "proactive" => {
                return Sse::new(proactive::stream_proactive(
                    None,
                    user_id.clone(),
                    session_id.clone(),
                    text.clone(),
                    state.ambient.clone(),
                    sessions,
                    suzy,
                ))
                .into_response();
            }
            _ => {}
        }
    }

    Sse::new(mock::stream_intent_with_scenario(
        &scenario,
        &text,
        if state.coordinator_enabled {
            state.suzy_runner.clone()
        } else {
            None
        },
        sessions,
        persist.then_some(session_id),
        persist.then_some(user_id),
    ))
    .into_response()
}

pub fn dispatch_action(req: ActionDispatch<'_>) -> Response {
    if scenarios::action_is_live(
        req.action,
        req.scenario.as_deref(),
        req.state.scenario_flags.deck,
    ) {
        if req.action == "combine" {
            if let Some(runner) = req.state.combine_runner.clone() {
                return Sse::new(combine::stream_combine(
                    runner,
                    req.user_id,
                    req.session_id,
                    req.text,
                    req.state.artifact_dir.clone(),
                    req.state.sessions.clone(),
                    req.scenario.clone(),
                ))
                .into_response();
            }
        }
    }

    Sse::new(mock::stream_action(
        req.action,
        req.scenario.as_deref(),
        &req.text,
    ))
    .into_response()
}

/// Type alias for SSE stream returned by orchestrators.
pub type SseStream = ReceiverStream<Result<axum::response::sse::Event, Infallible>>;