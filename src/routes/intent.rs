use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::awp_gate;
use crate::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
use crate::orchestrator::sse_collect;
use crate::routes::actions::caller_session;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct IntentRequest {
    pub text: String,
}

pub async fn submit_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<IntentRequest>,
) -> Response {
    if body.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "intent text required").into_response();
    }

    let client_key = format!("intent:{session_id}");
    if let Err(resp) = state
        .awp
        .check(&headers, &client_key, "submit_intent")
        .await
    {
        return resp;
    }

    let record = match caller_session(&state, &headers, &session_id).await {
        Ok(record) => record,
        Err(resp) => return resp,
    };

    dispatch_intent(IntentDispatch {
        state: &state,
        session_id,
        user_id: record.user_id,
        text: body.text,
    })
    .await
}

fn a2a_intent_text(body: &serde_json::Value) -> String {
    body.get("payload")
        .and_then(|p| {
            p.get("intent")
                .or_else(|| p.get("query"))
                .or_else(|| p.get("text"))
        })
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            body.get("intent")
                .or_else(|| body.get("query"))
                .or_else(|| body.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
        })
        .trim()
        .to_string()
}

/// A2A entry — same orchestration pipeline as `POST /api/sessions/{id}/intent`.
pub async fn a2a_intent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let message_id = body
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let intent_text = a2a_intent_text(&body);
    if intent_text.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "messageId": message_id,
                "message": "payload must include intent, query, or text"
            })),
        )
            .into_response();
    }

    let client_key = awp_gate::client_key(&headers, "a2a-intent");
    if let Err(resp) = state
        .awp
        .check(&headers, &client_key, "submit_intent")
        .await
    {
        return resp;
    }

    let record = state.sessions.create().await;
    let session_id = record.session_id.clone();
    let user_id = record.user_id.clone();

    let sse_response = dispatch_intent(IntentDispatch {
        state: &state,
        session_id: session_id.clone(),
        user_id,
        text: intent_text.clone(),
    })
    .await;

    let wants_sse = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/event-stream"));

    if wants_sse {
        return sse_response;
    }

    let events = sse_collect::collect_sse_events(sse_response).await;
    let scenario = events
        .iter()
        .find(|e| e.get("type").and_then(|t| t.as_str()) == Some("scenario"))
        .and_then(|e| e.get("key").and_then(|k| k.as_str()))
        .unwrap_or("unknown");

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "completed",
            "messageId": message_id,
            "sessionId": session_id,
            "scenario": scenario,
            "intent": intent_text,
            "events": events,
            "mockOrchestration": state.runtime.uses_mock_orchestration,
            "intentEndpoint": format!("/api/sessions/{session_id}/intent")
        })),
    )
        .into_response()
}