//! Pending actions — proposed effects waiting for the user (S2-T6/T7).

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::domain::Domain;
use crate::permissions::Effect;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingStatus {
    Pending,
    /// Claimed by one approval and running its tool; no other request may run or reject it.
    Executing,
    Approved,
    Rejected,
    Expired,
    Failed,
}

impl PendingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PendingStatus::Pending => "pending",
            PendingStatus::Executing => "executing",
            PendingStatus::Approved => "approved",
            PendingStatus::Rejected => "rejected",
            PendingStatus::Expired => "expired",
            PendingStatus::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "executing" => Some(Self::Executing),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "expired" => Some(Self::Expired),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: Uuid,
    pub user_id: String,
    pub session_id: Option<String>,
    pub agent_id: String,
    pub domain: Domain,
    pub tool: String,
    pub effect: Effect,
    /// The exact tool arguments to run on approval. Content by nature; never copied into the
    /// ledger or audit summary. Encrypted at rest from S3 for sensitive agents.
    pub args: serde_json::Value,
    /// Content-free description: agent, tool, effect, argument *keys*.
    pub summary: String,
    pub status: PendingStatus,
    pub trace_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub result: Option<serde_json::Value>,
}

/// Default lifetime of a pending action (§10.4).
pub const PENDING_TTL_HOURS: i64 = 48;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PendingEvent {
    Created(PendingAction),
    Resolved(PendingAction),
}

/// Outcome of a status change attempted on a pending action.
#[derive(Clone, Debug)]
pub enum Transition {
    /// This caller made the change.
    Applied(PendingAction),
    /// Another request claimed or resolved the action first, or it expired: the action as it stands.
    Conflict(PendingAction),
    NotFound,
}

impl Transition {
    /// The action after the attempt, whoever changed it.
    pub fn action(self) -> Option<PendingAction> {
        match self {
            Transition::Applied(a) | Transition::Conflict(a) => Some(a),
            Transition::NotFound => None,
        }
    }
}

/// Argument keys only — a content-free summary for audit and SSE.
pub fn summarize(agent_id: &str, tool: &str, effect: Effect, args: &serde_json::Value) -> String {
    let keys = args
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    if keys.is_empty() {
        format!("{agent_id} · {tool} ({effect})")
    } else {
        format!("{agent_id} · {tool} ({effect}) with {keys}")
    }
}

#[derive(Clone)]
pub struct PendingActions {
    inner: Arc<RwLock<Vec<PendingAction>>>,
    pg: Option<PgPool>,
    tx: broadcast::Sender<PendingEvent>,
}

impl PendingActions {
    pub fn new(pg: Option<PgPool>) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(RwLock::new(Vec::new())),
            pg,
            tx,
        }
    }

    pub fn in_memory() -> Self {
        Self::new(None)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PendingEvent> {
        self.tx.subscribe()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        user_id: &str,
        session_id: Option<&str>,
        agent_id: &str,
        domain: Domain,
        tool: &str,
        effect: Effect,
        args: serde_json::Value,
        trace_id: Option<&str>,
    ) -> PendingAction {
        let now = Utc::now();
        let action = PendingAction {
            id: Uuid::new_v4(),
            user_id: user_id.into(),
            session_id: session_id.map(str::to_string),
            agent_id: agent_id.into(),
            domain,
            tool: tool.into(),
            effect,
            summary: summarize(agent_id, tool, effect, &args),
            args,
            status: PendingStatus::Pending,
            trace_id: trace_id.map(str::to_string),
            created_at: now,
            expires_at: now + Duration::hours(PENDING_TTL_HOURS),
            resolved_at: None,
            result: None,
        };
        self.inner.write().await.push(action.clone());
        if let Some(pool) = &self.pg
            && let Err(e) = insert_pg(pool, &action).await
        {
            tracing::warn!("pending_actions insert failed: {e:#}");
        }
        let _ = self.tx.send(PendingEvent::Created(action.clone()));
        action
    }

    pub async fn get(&self, id: Uuid) -> Option<PendingAction> {
        self.inner.read().await.iter().find(|a| a.id == id).cloned()
    }

    /// List actions for a user, optionally filtered by status and session. Expires stale ones.
    pub async fn list(&self, user_id: &str, status: Option<PendingStatus>, session_id: Option<&str>) -> Vec<PendingAction> {
        self.expire_stale().await;
        self.inner
            .read()
            .await
            .iter()
            .filter(|a| a.user_id == user_id)
            .filter(|a| status.map(|s| a.status == s).unwrap_or(true))
            .filter(|a| session_id.map(|s| a.session_id.as_deref() == Some(s)).unwrap_or(true))
            .cloned()
            .collect()
    }

    /// Claim a pending action for execution: Pending → Executing under the write lock, so of
    /// two concurrent approvals (or an approval racing a rejection) exactly one wins. An action
    /// past `expires_at` becomes Expired instead. With Postgres the claim is also a conditional
    /// `UPDATE … WHERE status = 'pending'`, so a row another instance already took is not run.
    pub async fn claim(&self, id: Uuid) -> Transition {
        let now = Utc::now();
        let claimed = {
            let mut guard = self.inner.write().await;
            let Some(action) = guard.iter_mut().find(|a| a.id == id) else {
                return Transition::NotFound;
            };
            if action.status != PendingStatus::Pending {
                return Transition::Conflict(action.clone());
            }
            if action.expires_at <= now {
                action.status = PendingStatus::Expired;
                action.resolved_at = Some(now);
                Err(action.clone())
            } else {
                action.status = PendingStatus::Executing;
                Ok(action.clone())
            }
        };
        match claimed {
            Err(expired) => {
                self.persist_resolution(&expired, "status = 'pending'").await;
                let _ = self.tx.send(PendingEvent::Resolved(expired.clone()));
                Transition::Conflict(expired)
            }
            Ok(claimed) => match self.claim_pg(&claimed).await {
                Some(taken) => Transition::Conflict(self.set_status(id, taken).await.unwrap_or(claimed)),
                None => Transition::Applied(claimed),
            },
        }
    }

    /// Mirror the claim in Postgres. `Some(status)` when the row is no longer pending there
    /// (another instance claimed or resolved it). A missing row (its insert failed) or a database
    /// error leaves the in-memory claim, which already excludes every other request here, in force.
    async fn claim_pg(&self, a: &PendingAction) -> Option<PendingStatus> {
        let pool = self.pg.as_ref()?;
        let claimed = sqlx::query_scalar::<_, Uuid>(
            "UPDATE pending_actions SET status = 'executing' WHERE id = $1 AND status = 'pending' RETURNING id",
        )
        .bind(a.id)
        .fetch_optional(pool)
        .await;
        match claimed {
            Ok(Some(_)) => None,
            Ok(None) => {
                let current = sqlx::query_scalar::<_, String>("SELECT status FROM pending_actions WHERE id = $1")
                    .bind(a.id)
                    .fetch_optional(pool)
                    .await;
                match current {
                    Ok(Some(s)) => Some(PendingStatus::parse(&s).unwrap_or(PendingStatus::Failed)),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!("pending_actions claim lookup failed: {e:#}");
                        None
                    }
                }
            }
            Err(e) => {
                tracing::warn!("pending_actions claim failed: {e:#}");
                None
            }
        }
    }

    async fn set_status(&self, id: Uuid, status: PendingStatus) -> Option<PendingAction> {
        let mut guard = self.inner.write().await;
        let action = guard.iter_mut().find(|a| a.id == id)?;
        action.status = status;
        Some(action.clone())
    }

    /// Finish a claimed action: Executing → `status` (approved or failed) with the tool result.
    pub async fn complete(&self, id: Uuid, status: PendingStatus, result: Option<serde_json::Value>) -> Option<PendingAction> {
        let updated = {
            let mut guard = self.inner.write().await;
            let action = guard.iter_mut().find(|a| a.id == id && a.status == PendingStatus::Executing)?;
            action.status = status;
            action.resolved_at = Some(Utc::now());
            action.result = result;
            action.clone()
        };
        // 'pending' too: the claim's own write may have failed, and the row must still close.
        self.persist_resolution(&updated, "status IN ('pending', 'executing')").await;
        let _ = self.tx.send(PendingEvent::Resolved(updated.clone()));
        Some(updated)
    }

    /// Resolve a still-pending action without running it (reject, or a commit that is its own
    /// approval). A claimed, resolved or expired action is left as it is and reported as a conflict.
    pub async fn resolve(&self, id: Uuid, status: PendingStatus, result: Option<serde_json::Value>) -> Transition {
        let now = Utc::now();
        let outcome = {
            let mut guard = self.inner.write().await;
            let Some(action) = guard.iter_mut().find(|a| a.id == id) else {
                return Transition::NotFound;
            };
            if action.status != PendingStatus::Pending {
                return Transition::Conflict(action.clone());
            }
            action.resolved_at = Some(now);
            if action.expires_at <= now {
                action.status = PendingStatus::Expired;
                Transition::Conflict(action.clone())
            } else {
                action.status = status;
                action.result = result;
                Transition::Applied(action.clone())
            }
        };
        if let Transition::Applied(a) | Transition::Conflict(a) = &outcome {
            self.persist_resolution(a, "status = 'pending'").await;
            let _ = self.tx.send(PendingEvent::Resolved(a.clone()));
        }
        outcome
    }

    async fn persist_resolution(&self, a: &PendingAction, only_if: &'static str) {
        let Some(pool) = &self.pg else { return };
        let sql = format!("UPDATE pending_actions SET status = $2, resolved_at = $3, result = $4 WHERE id = $1 AND {only_if}");
        if let Err(e) = sqlx::query(&sql)
            .bind(a.id)
            .bind(a.status.as_str())
            .bind(a.resolved_at)
            .bind(&a.result)
            .execute(pool)
            .await
        {
            tracing::warn!("pending_actions update failed: {e:#}");
        }
    }

    /// Replace the arguments of a pending action (the "Edit" path) — stays pending.
    pub async fn edit(&self, id: Uuid, args: serde_json::Value) -> Option<PendingAction> {
        let mut guard = self.inner.write().await;
        let action = guard.iter_mut().find(|a| a.id == id && a.status == PendingStatus::Pending)?;
        action.summary = summarize(&action.agent_id, &action.tool, action.effect, &args);
        action.args = args;
        Some(action.clone())
    }

    pub async fn expire_stale(&self) {
        let now = Utc::now();
        let mut expired = Vec::new();
        {
            let mut guard = self.inner.write().await;
            for a in guard.iter_mut() {
                if a.status == PendingStatus::Pending && a.expires_at <= now {
                    a.status = PendingStatus::Expired;
                    a.resolved_at = Some(now);
                    expired.push(a.clone());
                }
            }
        }
        for a in expired {
            self.persist_resolution(&a, "status = 'pending'").await;
            let _ = self.tx.send(PendingEvent::Resolved(a));
        }
    }
}

async fn insert_pg(pool: &PgPool, a: &PendingAction) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO pending_actions (id, user_id, session_id, agent_id, domain, tool, effect, args, summary, status, trace_id, created_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(a.id)
    .bind(&a.user_id)
    .bind(&a.session_id)
    .bind(&a.agent_id)
    .bind(a.domain.as_str())
    .bind(&a.tool)
    .bind(a.effect.as_str())
    .bind(&a.args)
    .bind(&a.summary)
    .bind(a.status.as_str())
    .bind(&a.trace_id)
    .bind(a.created_at)
    .bind(a.expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn queued(p: &PendingActions) -> PendingAction {
        p.create("u", None, "inbox_agent", Domain::Work, "send_draft", Effect::SendExternal, serde_json::json!({}), None).await
    }

    #[tokio::test]
    async fn only_one_claim_wins_and_a_claimed_action_cannot_be_rejected() {
        let p = PendingActions::in_memory();
        let a = queued(&p).await;
        let (x, y) = tokio::join!(p.claim(a.id), p.claim(a.id));
        let wins = [&x, &y].iter().filter(|t| matches!(t, Transition::Applied(_))).count();
        assert_eq!(wins, 1, "exactly one concurrent claim wins");

        let reject = p.resolve(a.id, PendingStatus::Rejected, None).await;
        assert!(matches!(reject, Transition::Conflict(ref c) if c.status == PendingStatus::Executing));

        let done = p.complete(a.id, PendingStatus::Approved, Some(serde_json::json!({"ok": true}))).await.unwrap();
        assert_eq!(done.status, PendingStatus::Approved);
        assert!(p.complete(a.id, PendingStatus::Failed, None).await.is_none(), "complete only finishes a claim");
        assert!(matches!(p.claim(a.id).await, Transition::Conflict(ref c) if c.status == PendingStatus::Approved));
    }

    #[tokio::test]
    async fn a_rejected_action_cannot_be_claimed() {
        let p = PendingActions::in_memory();
        let a = queued(&p).await;
        assert!(matches!(p.resolve(a.id, PendingStatus::Rejected, None).await, Transition::Applied(_)));
        assert!(matches!(p.claim(a.id).await, Transition::Conflict(ref c) if c.status == PendingStatus::Rejected));
        assert!(matches!(p.claim(Uuid::new_v4()).await, Transition::NotFound));
    }

    #[tokio::test]
    async fn an_expired_action_is_expired_not_claimed() {
        let p = PendingActions::in_memory();
        let a = queued(&p).await;
        p.inner.write().await.iter_mut().find(|x| x.id == a.id).unwrap().expires_at = Utc::now() - Duration::seconds(1);
        // `get` alone does not expire it; the claim must check `expires_at` itself.
        assert_eq!(p.get(a.id).await.unwrap().status, PendingStatus::Pending);
        let claim = p.claim(a.id).await;
        assert!(matches!(claim, Transition::Conflict(ref c) if c.status == PendingStatus::Expired));
        assert_eq!(p.get(a.id).await.unwrap().status, PendingStatus::Expired);

        let b = queued(&p).await;
        p.inner.write().await.iter_mut().find(|x| x.id == b.id).unwrap().expires_at = Utc::now() - Duration::seconds(1);
        let reject = p.resolve(b.id, PendingStatus::Rejected, None).await;
        assert!(matches!(reject, Transition::Conflict(ref c) if c.status == PendingStatus::Expired));
    }

    #[test]
    fn executing_round_trips_as_a_status() {
        assert_eq!(PendingStatus::parse("executing"), Some(PendingStatus::Executing));
        assert_eq!(PendingStatus::Executing.as_str(), "executing");
        assert_eq!(PendingStatus::parse("all"), None);
    }
}
