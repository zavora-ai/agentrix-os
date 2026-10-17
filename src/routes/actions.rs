//! Pending actions API (S2-T7): list, approve, reject, edit, batch approve.
//!
//! Identity: the JWT user (`AwpGate::user_id`), never the owner of a caller-supplied
//! `session_id`; a `session_id` that is given must belong to that user.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::intelligence::ledger::ActivityEvent;
use crate::permissions::{execute_approved, AuditEntry, PendingAction, PendingStatus, Transition};
use crate::state::{AppState, SessionRecord};

/// The acting user on a `known` route: the subject of the JWT that earned Known trust. A
/// `session_id`, when given, must be one of that user's sessions.
pub async fn resolve_user(state: &AppState, headers: &HeaderMap, session_id: Option<&str>) -> Result<String, Response> {
    let Some(uid) = state.awp.user_id(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "sign in required").into_response());
    };
    let user = uid.to_string();
    if let Some(sid) = session_id {
        owned_record(state, &user, sid).await?;
    }
    Ok(user)
}

/// The session behind a session-scoped `known` route, if the JWT user owns it.
pub async fn owned_session(state: &AppState, headers: &HeaderMap, session_id: &str) -> Result<SessionRecord, Response> {
    let Some(uid) = state.awp.user_id(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "sign in required").into_response());
    };
    owned_record(state, &uid.to_string(), session_id).await
}

/// The session behind a session-scoped `anonymous` route (intent, chat, events). A session
/// created while signed in admits only that user; an anonymous one, whoever holds the id.
pub async fn caller_session(state: &AppState, headers: &HeaderMap, session_id: &str) -> Result<SessionRecord, Response> {
    let Some(record) = state.sessions.get(session_id).await else {
        return Err((StatusCode::NOT_FOUND, "session not found").into_response());
    };
    let caller = state.awp.user_id(headers).map(|u| u.to_string());
    if !record.admits(caller.as_deref()) {
        return Err((StatusCode::FORBIDDEN, "not your session").into_response());
    }
    Ok(record)
}

async fn owned_record(state: &AppState, user: &str, session_id: &str) -> Result<SessionRecord, Response> {
    let Some(record) = state.sessions.get(session_id).await else {
        return Err((StatusCode::NOT_FOUND, "session not found").into_response());
    };
    if record.user_id != user {
        return Err((StatusCode::FORBIDDEN, "not your session").into_response());
    }
    Ok(record)
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub status: Option<String>,
    pub session_id: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    let key = format!("actions:{}", q.session_id.clone().unwrap_or_default());
    state.awp.check(&headers, &key, "list_actions").await?;
    let user = resolve_user(&state, &headers, q.session_id.as_deref()).await?;
    let status = match q.status.as_deref() {
        None => Some(PendingStatus::Pending),
        Some("all") => None,
        Some(s) => Some(PendingStatus::parse(s).ok_or_else(|| (StatusCode::BAD_REQUEST, "bad status").into_response())?),
    };
    let actions = state.pending.list(&user, status, None).await;
    Ok(Json(serde_json::json!({ "user_id": user, "actions": actions })))
}

#[derive(Deserialize, Default)]
pub struct ActionBody {
    pub session_id: Option<String>,
    /// For `edit`: replacement arguments.
    pub args: Option<serde_json::Value>,
    /// For batch approve.
    pub ids: Option<Vec<Uuid>>,
}

async fn owned(state: &AppState, headers: &HeaderMap, id: Uuid, session_id: Option<&str>) -> Result<(String, PendingAction), Response> {
    let user = resolve_user(state, headers, session_id).await?;
    let Some(action) = state.pending.get(id).await else {
        return Err((StatusCode::NOT_FOUND, "action not found").into_response());
    };
    if action.user_id != user {
        return Err((StatusCode::FORBIDDEN, "not your action").into_response());
    }
    Ok((user, action))
}

/// Approve one pending action: claim it, execute with the un-gated tool, audit, ledger.
///
/// Only the request that claims the action (Pending → Executing) runs it. `Err` carries the
/// action as it stands when another approval or a rejection got there first, or it expired.
pub async fn approve_one(state: &AppState, user: &str, action: PendingAction) -> Result<PendingAction, PendingAction> {
    let action = match state.pending.claim(action.id).await {
        Transition::Applied(claimed) => claimed,
        Transition::Conflict(current) => return Err(current),
        Transition::NotFound => return Err(action),
    };
    let outcome = execute_approved(&action).await;
    let (status, result, decision) = match outcome {
        Ok(v) => (PendingStatus::Approved, Some(v), "approved"),
        Err(e) => (PendingStatus::Failed, Some(serde_json::json!({"error": e})), "failed"),
    };
    let resolved = state.pending.complete(action.id, status, result).await.unwrap_or(action.clone());
    let mode = state.permissions.mode_for(user, &action.agent_id, Some(&action.tool)).await;
    state
        .audit
        .record(
            AuditEntry::new(user, action.session_id.as_deref(), &action.agent_id, action.domain, &action.tool, action.effect, decision, mode, action.summary.clone())
                .approval(action.id)
                .trace(action.trace_id.as_deref()),
        )
        .await;
    state.ledger.record(
        ActivityEvent::new(user, action.domain, &action.agent_id, "approval")
            .effect(action.effect)
            .meta(serde_json::json!({"decision": decision, "tool_class": action.effect.as_str()})),
    );
    Ok(resolved)
}

/// 409 with the action as it stands: someone else claimed or resolved it, or it expired.
fn conflict(action: PendingAction) -> Response {
    (StatusCode::CONFLICT, Json(action)).into_response()
}

pub async fn approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    body: Option<Json<ActionBody>>,
) -> Result<Json<PendingAction>, Response> {
    let body = body.map(|b| b.0).unwrap_or_default();
    state.awp.check(&headers, &format!("approve:{id}"), "approve_action").await?;
    let (user, action) = owned(&state, &headers, id, body.session_id.as_deref()).await?;
    approve_one(&state, &user, action).await.map(Json).map_err(conflict)
}

pub async fn approve_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ActionBody>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "approve:batch", "approve_action").await?;
    let user = resolve_user(&state, &headers, body.session_id.as_deref()).await?;
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for id in body.ids.unwrap_or_default() {
        if let Some(action) = state.pending.get(id).await
            && action.user_id == user
        {
            match approve_one(&state, &user, action).await {
                Ok(done) => out.push(done),
                Err(current) => skipped.push(current),
            }
        }
    }
    Ok(Json(serde_json::json!({ "approved": out, "skipped": skipped })))
}

pub async fn reject(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    body: Option<Json<ActionBody>>,
) -> Result<Json<PendingAction>, Response> {
    let body = body.map(|b| b.0).unwrap_or_default();
    state.awp.check(&headers, &format!("reject:{id}"), "approve_action").await?;
    let (user, action) = owned(&state, &headers, id, body.session_id.as_deref()).await?;
    let resolved = match state.pending.resolve(id, PendingStatus::Rejected, None).await {
        Transition::Applied(resolved) => resolved,
        Transition::Conflict(current) => return Err(conflict(current)),
        Transition::NotFound => return Err((StatusCode::NOT_FOUND, "action not found").into_response()),
    };
    let mode = state.permissions.mode_for(&user, &action.agent_id, Some(&action.tool)).await;
    state
        .audit
        .record(AuditEntry::new(&user, action.session_id.as_deref(), &action.agent_id, action.domain, &action.tool, action.effect, "rejected", mode, action.summary.clone()).approval(action.id))
        .await;
    state.ledger.record(ActivityEvent::new(&user, action.domain, &action.agent_id, "rejection").effect(action.effect).meta(serde_json::json!({"decision": "rejected"})));
    Ok(Json(resolved))
}

pub async fn edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ActionBody>,
) -> Result<Json<PendingAction>, Response> {
    state.awp.check(&headers, &format!("edit:{id}"), "approve_action").await?;
    let (_user, _action) = owned(&state, &headers, id, body.session_id.as_deref()).await?;
    let Some(args) = body.args else {
        return Err((StatusCode::BAD_REQUEST, "args required").into_response());
    };
    state
        .pending
        .edit(id, args)
        .await
        .map(Json)
        .ok_or_else(|| (StatusCode::CONFLICT, "action is no longer pending").into_response())
}

/// `GET /api/audit?session_id=&limit=` — newest first (viewer UI lands in S11).
#[derive(Deserialize, Default)]
pub struct AuditQuery {
    pub session_id: Option<String>,
    pub limit: Option<usize>,
}

pub async fn audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "audit", "list_actions").await?;
    let user = resolve_user(&state, &headers, q.session_id.as_deref()).await?;
    let entries = state.audit.list(&user, q.limit.unwrap_or(100)).await;
    Ok(Json(serde_json::json!({ "user_id": user, "entries": entries })))
}

/// Helper for the SSE layer: pending actions created for a session, as `permission_request` events.
pub fn permission_request_event(a: &PendingAction) -> crate::events::sse::FieldEvent {
    crate::events::sse::FieldEvent::PermissionRequest {
        action_id: a.id.to_string(),
        agent_id: a.agent_id.clone(),
        domain: a.domain,
        effect: a.effect.as_str().to_string(),
        summary: a.summary.clone(),
        expires_at: a.expires_at.to_rfc3339(),
    }
}

