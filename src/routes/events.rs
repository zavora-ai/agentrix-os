//! `POST /api/sessions/{sid}/events` — content-free UI signals for the ledger (S2-T2).
//!
//! The field client reports counts only: focus changes, notifications shown, cards opened per
//! domain. Nothing about *what* was shown.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::domain::Domain;
use crate::intelligence::ledger::ActivityEvent;
use crate::routes::actions::caller_session;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct UiEvent {
    /// ui_focus | ui_blur | ui_notification | ui_card_open | ui_lens | ui_gesture
    pub kind: String,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub domain: Option<Domain>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
}

#[derive(Deserialize)]
pub struct UiEventsBody {
    pub events: Vec<UiEvent>,
}

pub const UI_KINDS: &[&str] = &["ui_focus", "ui_blur", "ui_notification", "ui_card_open", "ui_lens", "ui_gesture"];

pub async fn record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<UiEventsBody>,
) -> Result<StatusCode, Response> {
    state.awp.check(&headers, &format!("events:{session_id}"), "record_ui_events").await?;
    let rec = caller_session(&state, &headers, &session_id).await?;
    if body.events.len() > 200 {
        return Err((StatusCode::BAD_REQUEST, "too many events").into_response());
    }
    for e in body.events {
        if !UI_KINDS.contains(&e.kind.as_str()) {
            continue;
        }
        let mut ev = ActivityEvent::new(&rec.user_id, e.domain.unwrap_or_default(), "ui", &e.kind)
            .meta(serde_json::json!({ "count": e.count.unwrap_or(1) }));
        if let Some(ms) = e.duration_ms {
            ev = ev.duration_ms(ms);
        }
        state.ledger.record(ev);
    }
    Ok(StatusCode::ACCEPTED)
}
