use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::Domain;
use crate::intelligence::ledger::ActivityEvent;
use crate::permissions::{AuditEntry, Effect, PendingStatus};
use crate::routes::actions::owned_session;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CommitRequest {
    pub card_title: String,
    pub action_label: String,
}

#[derive(Serialize)]
pub struct CommitResponse {
    pub status: &'static str,
    pub action: String,
    pub message: String,
    /// The pending action this commit created and approved in one step (S2-T9).
    pub action_id: String,
}

/// A card's primary label is a user-authored approval: it maps to the effect the label implies.
fn label_effect(label: &str) -> Effect {
    let l = label.to_lowercase();
    if l.contains("send") || l.contains("reply") || l.contains("post") {
        Effect::SendExternal
    } else if l.contains("book") || l.contains("hold") || l.contains("reserve") || l.contains("reschedule") {
        Effect::ScheduleWithOthers
    } else if l.contains("buy") || l.contains("pay") {
        Effect::Financial
    } else if l.contains("archive") || l.contains("dismiss") || l.contains("snooze") {
        Effect::Delete
    } else if l.contains("draft") || l.contains("save") || l.contains("apply") || l.contains("prep") {
        Effect::WriteLocal
    } else {
        Effect::Read
    }
}

pub async fn commit_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<CommitRequest>,
) -> Result<Json<CommitResponse>, Response> {
    let client_key = format!("commit:{session_id}");
    state
        .awp
        .check(&headers, &client_key, "commit_action")
        .await
        .map_err(|r| r)?;

    let record = owned_session(&state, &headers, &session_id).await?;

    let scenario = record.scenario.as_deref().unwrap_or("").to_string();
    let card = record
        .cards
        .iter()
        .find(|c| c.card.get("title").and_then(|t| t.as_str()) == Some(body.card_title.as_str()));
    let domain = card.map(|c| c.domain).unwrap_or_else(|| Domain::for_scenario(&scenario));
    let agent = card
        .and_then(|c| c.card.get("agent").and_then(|a| a.as_str()))
        .map(crate::mother::delegate::agent_id_from_card_agent)
        .unwrap_or_else(|| "ui".into());
    let effect = label_effect(&body.action_label);

    // The commit is itself the approval: create and resolve a pending action, audit it, ledger it.
    let pending = state
        .pending
        .create(
            &record.user_id,
            Some(&session_id),
            &agent,
            domain,
            &format!("commit:{}", body.action_label.to_lowercase().replace(' ', "_")),
            effect,
            serde_json::json!({ "card": body.card_title, "label": body.action_label }),
            None,
        )
        .await;
    let resolved = state
        .pending
        .resolve(pending.id, PendingStatus::Approved, Some(serde_json::json!({"via": "commit"})))
        .await
        .unwrap_or(pending);
    let mode = state.permissions.mode_for(&record.user_id, &agent, None).await;
    state
        .audit
        .record(
            AuditEntry::new(&record.user_id, Some(&session_id), &agent, domain, &resolved.tool, effect, "approved", mode, resolved.summary.clone())
                .approval(resolved.id),
        )
        .await;
    state.ledger.record(
        ActivityEvent::new(&record.user_id, domain, &agent, "approval")
            .effect(effect)
            .meta(serde_json::json!({ "decision": "approved", "source": "commit", "scenario": scenario })),
    );

    let message = match (scenario.as_str(), body.action_label.to_lowercase().as_str()) {
        ("morning", label) if label.contains("draft") => {
            "Draft replies queued via inbox agent (connect email MCP for live drafts)."
                .into()
        }
        ("deck", label) if label.contains("save") || label.contains("open") => {
            "Artifact link preserved on card.".into()
        }
        _ => format!("Recorded commit: {}", body.action_label),
    };

    Ok(Json(CommitResponse {
        status: "ok",
        action: body.action_label,
        message,
        action_id: resolved.id.to_string(),
    }))
}