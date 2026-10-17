use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::orchestrator::dispatch::{dispatch_action, ActionDispatch};
use crate::routes::actions::owned_session;
use crate::scenarios;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ActionRequest {
    pub text: String,
}

pub async fn submit_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<ActionRequest>,
) -> Response {
    if body.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "action text required").into_response();
    }

    let client_key = format!("action:{session_id}");
    if let Err(resp) = state
        .awp
        .check(&headers, &client_key, "submit_action")
        .await
    {
        return resp;
    }

    let record = match owned_session(&state, &headers, &session_id).await {
        Ok(record) => record,
        Err(resp) => return resp,
    };

    let action = scenarios::pick_action(&body.text).unwrap_or("handle");

    dispatch_action(ActionDispatch {
        state: &state,
        session_id,
        user_id: record.user_id,
        text: body.text,
        action,
        scenario: record.scenario,
    })
}