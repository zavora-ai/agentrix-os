//! Memory API (S3-T5): list, remember, confirm / correct, forget, export, purge.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::Domain;
use crate::memory::service::{Kind, NewItem, Provenance, Scope, Sensitivity};
use crate::routes::actions::resolve_user;
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub session_id: Option<String>,
    pub kind: Option<String>,
    pub domain: Option<Domain>,
    pub prefix: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "memory", "manage_memory").await?;
    let user = resolve_user(&state, &headers, q.session_id.as_deref()).await?;
    let kind = q.kind.as_deref().map(|k| Kind::parse(k).ok_or_else(|| (StatusCode::BAD_REQUEST, "bad kind").into_response())).transpose()?;
    let keys = q.prefix.clone().map(|p| vec![p]);
    let items: Vec<_> = state
        .memory
        .read(&user, Scope::MOTHER, keys.as_deref())
        .await
        .into_iter()
        .filter(|i| kind.map(|k| i.kind == k).unwrap_or(true))
        .filter(|i| q.domain.map(|d| i.domain == d).unwrap_or(true))
        .collect();
    Ok(Json(serde_json::json!({ "user_id": user, "items": items })))
}

#[derive(Deserialize)]
pub struct RememberBody {
    pub session_id: Option<String>,
    pub domain: Option<Domain>,
    pub category: Option<String>,
    pub key: String,
    pub value: serde_json::Value,
    pub sensitivity: Option<Sensitivity>,
}

/// `POST /api/memory` — the user states a fact → known.
pub async fn remember(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RememberBody>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "memory", "manage_memory").await?;
    let user = resolve_user(&state, &headers, body.session_id.as_deref()).await?;
    if body.key.trim().is_empty() || body.key.len() > 120 {
        return Err((StatusCode::BAD_REQUEST, "key required (≤120 chars)").into_response());
    }
    let item = state
        .memory
        .remember(
            &user,
            NewItem {
                domain: body.domain.unwrap_or_default(),
                category: body.category.as_deref().unwrap_or("context"),
                key: body.key.trim(),
                value: body.value,
                sensitivity: body.sensitivity.unwrap_or_default(),
                source_agent: "user",
                provenance: Provenance::new("user_statement").session(body.session_id.as_deref()),
            },
        )
        .await;
    Ok(Json(serde_json::to_value(item).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct PatchBody {
    pub session_id: Option<String>,
    /// confirm | correct
    pub op: String,
    pub value: Option<serde_json::Value>,
}

pub async fn patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchBody>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "memory", "manage_memory").await?;
    let user = resolve_user(&state, &headers, body.session_id.as_deref()).await?;
    let item = match body.op.as_str() {
        "confirm" => state.memory.confirm(&user, id, body.session_id.as_deref()).await,
        "correct" => {
            let Some(value) = body.value else {
                return Err((StatusCode::BAD_REQUEST, "value required for correct").into_response());
            };
            state.memory.correct(&user, id, value, body.session_id.as_deref()).await
        }
        _ => return Err((StatusCode::BAD_REQUEST, "op must be confirm or correct").into_response()),
    };
    item.map(|i| Json(serde_json::to_value(i).unwrap_or_default()))
        .ok_or_else(|| (StatusCode::NOT_FOUND, "memory item not found").into_response())
}

#[derive(Deserialize, Default)]
pub struct SessionQuery {
    pub session_id: Option<String>,
}

pub async fn forget(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(q): Query<SessionQuery>,
) -> Result<StatusCode, Response> {
    state.awp.check(&headers, "memory", "manage_memory").await?;
    let user = resolve_user(&state, &headers, q.session_id.as_deref()).await?;
    state
        .memory
        .forget(&user, id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "memory item not found").into_response())
}

pub async fn export(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SessionQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "memory", "manage_memory").await?;
    let user = resolve_user(&state, &headers, q.session_id.as_deref()).await?;
    let items = state.memory.export(&user).await;
    Ok(Json(serde_json::json!({
        "user_id": user,
        "exported_at": chrono::Utc::now(),
        "format": "agentrix-memory-export/v1",
        "items": items,
    })))
}

pub async fn purge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SessionQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "memory", "manage_memory").await?;
    let user = resolve_user(&state, &headers, q.session_id.as_deref()).await?;
    let n = state.memory.purge(&user).await;
    Ok(Json(serde_json::json!({ "user_id": user, "deleted": n })))
}
