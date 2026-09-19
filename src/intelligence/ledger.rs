//! The content-free activity ledger (S2-T1, ADR-004).
//!
//! Every intent, tool call, card resolve, approval, observation and user action is recorded
//! as an [`ActivityEvent`]: who (agent), what kind of thing, which domain, which effect class,
//! how long, and a **keyed hash** of the subject — never the subject itself. `meta` is a small
//! JSON object limited to an allow-list of content-free keys.
//!
//! Storage: an in-memory ring (always) plus Postgres when a pool is configured. Writes are
//! fire-and-forget through a bounded channel so hot paths never wait on the database.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;

use crate::domain::Domain;
use crate::permissions::Effect;

/// Keys `meta` may carry. Anything else is dropped before write (and reported by
/// [`ActivityEvent::sanitize`]). Counts, categories, durations, flags — never content.
pub const META_ALLOWED_KEYS: &[&str] = &[
    "kind", "count", "minutes", "hours", "postponed_count", "tool_class", "channel", "weekend",
    "topic_id", "status", "decision", "entry", "source", "targets", "domains", "scenario", "cards",
    "mode", "effect", "attention", "notifications", "focus", "reason", "agent_count", "index",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActivityEvent {
    #[serde(default)]
    pub id: Option<i64>,
    pub user_id: String,
    #[serde(default = "Utc::now")]
    pub ts: DateTime<Utc>,
    pub domain: Domain,
    pub agent_id: String,
    pub kind: String,
    #[serde(default)]
    pub effect: Option<Effect>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
    #[serde(default)]
    pub subject_hash: Option<String>,
    #[serde(default)]
    pub meta: serde_json::Value,
    #[serde(default)]
    pub trace_id: Option<String>,
}

impl ActivityEvent {
    pub fn new(user_id: impl Into<String>, domain: Domain, agent_id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: None,
            user_id: user_id.into(),
            ts: Utc::now(),
            domain,
            agent_id: agent_id.into(),
            kind: kind.into(),
            effect: None,
            duration_ms: None,
            subject_hash: None,
            meta: serde_json::json!({}),
            trace_id: None,
        }
    }
    pub fn effect(mut self, e: Effect) -> Self {
        self.effect = Some(e);
        self
    }
    pub fn duration_ms(mut self, ms: i32) -> Self {
        self.duration_ms = Some(ms);
        self
    }
    /// Store a keyed hash of `subject` (a thread id, task id, …) — never the subject.
    pub fn subject(mut self, key: &[u8], subject: &str) -> Self {
        self.subject_hash = Some(hash_subject(key, subject));
        self
    }
    pub fn meta(mut self, meta: serde_json::Value) -> Self {
        self.meta = meta;
        self
    }
    pub fn trace(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    /// Drop any `meta` key that is not on the allow-list. Returns the dropped keys.
    pub fn sanitize(&mut self) -> Vec<String> {
        let mut dropped = Vec::new();
        if let Some(obj) = self.meta.as_object_mut() {
            let keys: Vec<String> = obj.keys().cloned().collect();
            for k in keys {
                let ok = META_ALLOWED_KEYS.contains(&k.as_str());
                let scalar = obj.get(&k).map(|v| !v.is_object() && !(v.is_string() && v.as_str().map(|s| s.len() > 64).unwrap_or(false))).unwrap_or(false);
                if !ok || !scalar {
                    obj.remove(&k);
                    dropped.push(k);
                }
            }
        } else {
            self.meta = serde_json::json!({});
        }
        dropped
    }
}

/// HMAC-SHA256 of `subject`, hex, truncated to 24 chars — enough to correlate, useless to invert
/// without the key.
pub fn hash_subject(key: &[u8], subject: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(subject.as_bytes());
    let out = mac.finalize().into_bytes();
    let hex: String = out.iter().map(|b| format!("{b:02x}")).collect();
    hex[..24].to_string()
}

#[derive(Clone, Debug, Default)]
pub struct LedgerQuery {
    pub user_id: String,
    pub domain: Option<Domain>,
    pub kind: Option<String>,
    pub agent_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: usize,
}

const RING_CAPACITY: usize = 5_000;

struct Inner {
    /// Synchronous in-memory ring: `record` never awaits, so hot paths and process-wide use
    /// (agents built at boot, tests with short-lived runtimes) always see their events.
    ring: std::sync::Mutex<VecDeque<ActivityEvent>>,
    pg: Option<PgPool>,
    hash_key: Vec<u8>,
    pending_pg: AtomicU64,
}

/// Cloneable handle to the ledger.
#[derive(Clone)]
pub struct LedgerService {
    inner: Arc<Inner>,
}

impl LedgerService {
    pub fn new(pg: Option<PgPool>, hash_key: impl Into<Vec<u8>>) -> Self {
        Self {
            inner: Arc::new(Inner {
                ring: std::sync::Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
                pg,
                hash_key: hash_key.into(),
                pending_pg: AtomicU64::new(0),
            }),
        }
    }

    /// In-memory only (tests, no DATABASE_URL).
    pub fn in_memory() -> Self {
        Self::new(None, b"agentrix-ledger-dev-key".to_vec())
    }

    pub fn postgres_enabled(&self) -> bool {
        self.inner.pg.is_some()
    }

    /// Key for subject hashing (so callers can build events with `.subject(key, …)`).
    pub fn hash_key(&self) -> &[u8] {
        &self.inner.hash_key
    }

    pub fn hash(&self, subject: &str) -> String {
        hash_subject(&self.inner.hash_key, subject)
    }

    /// Record an event (sanitized). The in-memory write is immediate; the Postgres insert is
    /// spawned on the current runtime and never blocks the caller.
    pub fn record(&self, mut ev: ActivityEvent) {
        let dropped = ev.sanitize();
        if !dropped.is_empty() {
            tracing::debug!(kind = %ev.kind, ?dropped, "ledger dropped non-allow-listed meta keys");
        }
        {
            let mut ring = self.inner.ring.lock().unwrap_or_else(|e| e.into_inner());
            if ring.len() >= RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(ev.clone());
        }
        if let Some(pool) = self.inner.pg.clone() {
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => {
                    let inner = self.inner.clone();
                    inner.pending_pg.fetch_add(1, Ordering::SeqCst);
                    handle.spawn(async move {
                        if let Err(e) = insert_pg(&pool, &ev).await {
                            tracing::warn!("ledger insert failed: {e:#}");
                        }
                        inner.pending_pg.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Err(_) => tracing::warn!("ledger: no runtime for the Postgres insert — event kept in memory only"),
            }
        }
    }

    /// Wait until spawned Postgres inserts have finished (in-memory writes are already visible).
    pub async fn flush(&self) {
        for _ in 0..400 {
            if self.inner.pending_pg.load(Ordering::SeqCst) == 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        tracing::warn!("ledger flush timed out waiting for Postgres inserts");
    }

    /// Query the in-memory ring (Postgres-backed reads land with the intelligence jobs in S7).
    pub async fn query(&self, q: &LedgerQuery) -> Vec<ActivityEvent> {
        let ring = self.inner.ring.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<ActivityEvent> = ring
            .iter()
            .rev()
            .filter(|e| e.user_id == q.user_id)
            .filter(|e| q.domain.map(|d| e.domain == d).unwrap_or(true))
            .filter(|e| q.kind.as_deref().map(|k| e.kind == k).unwrap_or(true))
            .filter(|e| q.agent_id.as_deref().map(|a| e.agent_id == a).unwrap_or(true))
            .filter(|e| q.since.map(|s| e.ts >= s).unwrap_or(true))
            .take(if q.limit == 0 { usize::MAX } else { q.limit })
            .cloned()
            .collect();
        out.reverse();
        out
    }

    pub async fn count(&self, user_id: &str) -> usize {
        self.inner
            .ring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|e| e.user_id == user_id)
            .count()
    }

    /// Bulk insert (dev loader). Postgres only.
    pub async fn import(&self, events: Vec<ActivityEvent>) -> anyhow::Result<usize> {
        let Some(pool) = &self.inner.pg else {
            anyhow::bail!("import requires DATABASE_URL");
        };
        let mut n = 0;
        for mut ev in events {
            ev.sanitize();
            insert_pg(pool, &ev).await?;
            n += 1;
        }
        Ok(n)
    }
}

async fn insert_pg(pool: &PgPool, ev: &ActivityEvent) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO activity_events (user_id, ts, domain, agent_id, kind, effect, duration_ms, subject_hash, meta, trace_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&ev.user_id)
    .bind(ev.ts)
    .bind(ev.domain.as_str())
    .bind(&ev.agent_id)
    .bind(&ev.kind)
    .bind(ev.effect.map(|e| e.as_str()))
    .bind(ev.duration_ms)
    .bind(&ev.subject_hash)
    .bind(&ev.meta)
    .bind(&ev.trace_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_sanitize_drops_content_and_hashes_subjects() {
        let mut ev = ActivityEvent::new("u", Domain::Work, "email", "tool_call")
            .effect(Effect::Read)
            .subject(b"k", "thread-42")
            .meta(serde_json::json!({"count": 3, "body": "Dear client…", "subject": "Contract", "tool_class": "read"}));
        let dropped = ev.sanitize();
        assert_eq!(dropped, vec!["body".to_string(), "subject".to_string()]);
        assert_eq!(ev.meta, serde_json::json!({"count": 3, "tool_class": "read"}));
        assert_eq!(ev.subject_hash.as_ref().unwrap().len(), 24);
        assert_ne!(hash_subject(b"k", "thread-42"), hash_subject(b"other", "thread-42"));
    }

    #[tokio::test]
    async fn ledger_records_and_queries_in_memory() {
        let ledger = LedgerService::in_memory();
        ledger.record(ActivityEvent::new("u1", Domain::Work, "email", "tool_call").effect(Effect::Read));
        ledger.record(ActivityEvent::new("u1", Domain::Home, "family", "tool_call").effect(Effect::Read));
        ledger.record(ActivityEvent::new("u2", Domain::Work, "email", "intent"));
        ledger.flush().await;
        assert_eq!(ledger.count("u1").await, 2);
        let work = ledger.query(&LedgerQuery { user_id: "u1".into(), domain: Some(Domain::Work), ..Default::default() }).await;
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].agent_id, "email");
    }
}
