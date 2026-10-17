//! `POST /api/sessions/{sid}/chat` — a conversational turn with the Mother Agent (S1-T5).
//!
//! Same pipeline as `submit_intent`, plus the turn (and the Mother's reply) is appended to the
//! session's chat history so follow-ups have context. Streams the standard field events.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::mother::{handle_intent, Entry, MotherRequest};
use crate::routes::actions::caller_session;
use crate::state::{AppState, ChatTurn};

#[derive(Deserialize)]
pub struct ChatRequest {
    pub text: String,
}

pub async fn chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<ChatRequest>,
) -> Response {
    if body.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "chat text required").into_response();
    }
    let client_key = format!("chat:{session_id}");
    if let Err(resp) = state.awp.check(&headers, &client_key, "chat_mother").await {
        return resp;
    }
    let record = match caller_session(&state, &headers, &session_id).await {
        Ok(record) => record,
        Err(resp) => return resp,
    };

    handle_intent(MotherRequest {
        state: &state,
        session_id,
        user_id: record.user_id,
        text: body.text,
        entry: Entry::Chat,
    })
    .await
}

#[derive(Serialize)]
pub struct ChatHistoryResponse {
    pub session_id: String,
    pub turns: Vec<ChatTurn>,
}

/// `GET /api/sessions/{sid}/chat` — hydrate the chat panel after refresh.
pub async fn history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ChatHistoryResponse>, Response> {
    let client_key = format!("chat:{session_id}");
    state
        .awp
        .check(&headers, &client_key, "chat_mother")
        .await
        .map_err(|r| r)?;
    let record = caller_session(&state, &headers, &session_id).await?;
    Ok(Json(ChatHistoryResponse {
        session_id: record.session_id,
        turns: record.chat_history,
    }))
}
