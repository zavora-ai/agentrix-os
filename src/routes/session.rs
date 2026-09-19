use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use adk_session::CreateRequest;

use crate::auth;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SessionResponse {
    pub session_id: String,
    pub user_id: String,
    pub authenticated: bool,
}

pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<SessionResponse> {
    let authenticated_user = state
        .auth
        .as_ref()
        .and_then(|a| auth::extract_user_id(&headers, &a.jwt_secret));
    let user_id = authenticated_user
        .map(|u| u.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let record = state.sessions.create_for_user(user_id).await;

    let _ = state
        .session_service
        .create(CreateRequest {
            app_name: "agentrix-os".into(),
            user_id: record.user_id.clone(),
            session_id: Some(record.session_id.clone()),
            state: Default::default(),
        })
        .await;

    Json(SessionResponse {
        session_id: record.session_id,
        user_id: record.user_id,
        authenticated: authenticated_user.is_some(),
    })
}