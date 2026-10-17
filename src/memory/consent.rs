//! Persisted consents (S11-T4, pulled forward to team sprint A — `docs/PROGRESS.md` §3).
//!
//! One row per grant in `consents` (migration 007): user, category, world, purpose, granted_at,
//! revoked_at. The store backs adk-awp's [`ConsentService`] (subject = user id, purpose =
//! category) so the AWP consent endpoints and the OS share one record, and exposes a
//! domain-aware API for the trust center and for the storage checks (memory, ledger, imports)
//! that S11 completes.
//!
//! Like the other R1 services it is installed process-wide at boot (`init`) with an in-memory
//! default when no database is configured (ADR-006).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use awp_types::AwpError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::Domain;

/// Consent categories the OS asks for (concept §12.2). A consent is per category and world.
pub const CATEGORIES: &[&str] = &[
    "calendar", "email", "health", "finance", "social", "location", "reading", "routines", "camera",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Consent {
    pub id: Uuid,
    pub user_id: String,
    pub category: String,
    pub world: Domain,
    /// Plain-language purpose shown in the trust center ("read your calendar to plan the day").
    pub purpose: String,
    pub granted_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Consent {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Whether this grant answers a check for `world`. A `shared` grant covers both worlds; a
    /// world grant covers only that world; a `shared` check is satisfied by any active grant.
    pub fn covers(&self, world: Domain) -> bool {
        self.is_active()
            && match world {
                Domain::Shared => true,
                w => self.world == w || self.world == Domain::Shared,
            }
    }
}

/// Lower-case letters and underscores, at most 40 chars.
pub fn normalize_category(raw: &str) -> Option<String> {
    let c = raw.trim().to_ascii_lowercase();
    if c.is_empty() || c.len() > 40 || !c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_') {
        return None;
    }
    Some(c)
}

#[derive(Default)]
struct Inner {
    users: HashMap<String, Vec<Consent>>,
    loads: crate::db::UserLoads,
}

#[derive(Clone)]
pub struct ConsentStore {
    inner: Arc<RwLock<Inner>>,
    pg: Option<PgPool>,
}

impl ConsentStore {
    pub fn new(pg: Option<PgPool>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            pg,
        }
    }

    pub fn in_memory() -> Self {
        Self::new(None)
    }

    pub fn postgres_enabled(&self) -> bool {
        self.pg.is_some()
    }

    /// Load the user's consents on first touch; a database error leaves the user unloaded so a
    /// later touch reads again. Consents granted or revoked here meanwhile are newer and win.
    async fn ensure_loaded(&self, user_id: &str) {
        let Some(pool) = &self.pg else { return };
        if !self.inner.read().await.loads.should_load(user_id) {
            return;
        }
        match load_pg(pool, user_id).await {
            Ok(mut rows) => {
                let mut guard = self.inner.write().await;
                if guard.loads.is_loaded(user_id) {
                    return;
                }
                for consent in guard.users.remove(user_id).unwrap_or_default() {
                    rows.retain(|r| r.id != consent.id);
                    rows.push(consent);
                }
                guard.users.insert(user_id.to_string(), rows);
                guard.loads.succeeded(user_id);
            }
            Err(e) => {
                tracing::warn!("consents load failed; retrying on a later request: {e:#}");
                self.inner.write().await.loads.failed(user_id);
            }
        }
    }

    async fn insert_pg(&self, c: &Consent) {
        let Some(pool) = &self.pg else { return };
        if let Err(e) = sqlx::query(
            "INSERT INTO consents (id, user_id, category, world, purpose, granted_at, revoked_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(c.id)
        .bind(&c.user_id)
        .bind(&c.category)
        .bind(c.world.as_str())
        .bind(&c.purpose)
        .bind(c.granted_at)
        .bind(c.revoked_at)
        .execute(pool)
        .await
        {
            tracing::warn!("consents persist failed: {e:#}");
        }
    }

    async fn revoke_pg(&self, id: Uuid, at: DateTime<Utc>) {
        let Some(pool) = &self.pg else { return };
        if let Err(e) = sqlx::query("UPDATE consents SET revoked_at = $2 WHERE id = $1")
            .bind(id)
            .bind(at)
            .execute(pool)
            .await
        {
            tracing::warn!("consents revoke failed: {e:#}");
        }
    }

    /// Grant `category` for `world`. Idempotent: an active grant for the same category and world
    /// is returned unchanged. `None` when the category is not a valid identifier.
    pub async fn grant(&self, user_id: &str, category: &str, world: Domain, purpose: &str) -> Option<Consent> {
        let category = normalize_category(category)?;
        self.ensure_loaded(user_id).await;
        let (consent, inserted) = {
            let mut guard = self.inner.write().await;
            let list = guard.users.entry(user_id.to_string()).or_default();
            if let Some(existing) = list.iter().find(|c| c.category == category && c.world == world && c.is_active()) {
                (existing.clone(), false)
            } else {
                let c = Consent {
                    id: Uuid::new_v4(),
                    user_id: user_id.to_string(),
                    category,
                    world,
                    purpose: purpose.trim().chars().take(240).collect(),
                    granted_at: Utc::now(),
                    revoked_at: None,
                };
                list.push(c.clone());
                (c, true)
            }
        };
        if inserted {
            self.insert_pg(&consent).await;
        }
        Some(consent)
    }

    /// Revoke active grants for `category` in `world` (every world when `None`). Returns how many.
    pub async fn revoke(&self, user_id: &str, category: &str, world: Option<Domain>) -> usize {
        let Some(category) = normalize_category(category) else { return 0 };
        self.ensure_loaded(user_id).await;
        let now = Utc::now();
        let revoked: Vec<Uuid> = {
            let mut guard = self.inner.write().await;
            let Some(list) = guard.users.get_mut(user_id) else { return 0 };
            let mut ids = Vec::new();
            for c in list.iter_mut() {
                let in_scope = world.map(|w| c.world == w).unwrap_or(true);
                if c.category == category && c.is_active() && in_scope {
                    c.revoked_at = Some(now);
                    ids.push(c.id);
                }
            }
            ids
        };
        for id in &revoked {
            self.revoke_pg(*id, now).await;
        }
        revoked.len()
    }

    /// Whether an active grant covers `category` for `world` (see [`Consent::covers`]).
    pub async fn has(&self, user_id: &str, category: &str, world: Domain) -> bool {
        let Some(category) = normalize_category(category) else { return false };
        self.ensure_loaded(user_id).await;
        self.inner
            .read()
            .await
            .users
            .get(user_id)
            .map(|list| list.iter().any(|c| c.category == category && c.covers(world)))
            .unwrap_or(false)
    }

    /// Every grant, including revoked ones, newest first — the trust center's history.
    pub async fn list(&self, user_id: &str) -> Vec<Consent> {
        self.ensure_loaded(user_id).await;
        let mut out = self.inner.read().await.users.get(user_id).cloned().unwrap_or_default();
        out.sort_by_key(|c| std::cmp::Reverse(c.granted_at));
        out
    }

    /// Only active grants.
    pub async fn active(&self, user_id: &str) -> Vec<Consent> {
        self.list(user_id).await.into_iter().filter(Consent::is_active).collect()
    }

    /// Hard delete everything for the user (account deletion, S11).
    pub async fn purge(&self, user_id: &str) -> usize {
        self.ensure_loaded(user_id).await;
        let n = {
            let mut guard = self.inner.write().await;
            guard.users.remove(user_id).map(|v| v.len()).unwrap_or(0)
        };
        if let Some(pool) = &self.pg
            && let Err(e) = sqlx::query("DELETE FROM consents WHERE user_id = $1").bind(user_id).execute(pool).await
        {
            tracing::warn!("consents purge failed: {e:#}");
        }
        n
    }
}

/// adk-awp view of the same store: subject = user id, purpose = category, world = shared.
#[async_trait]
impl adk_awp::ConsentService for ConsentStore {
    async fn capture_consent(&self, subject: &str, purpose: &str) -> Result<(), AwpError> {
        self.grant(subject, purpose, Domain::Shared, "granted through the AWP consent endpoint")
            .await
            .map(|_| ())
            .ok_or_else(|| AwpError::InvalidRequest(format!("invalid consent purpose '{purpose}'")))
    }

    async fn check_consent(&self, subject: &str, purpose: &str) -> Result<bool, AwpError> {
        Ok(self.has(subject, purpose, Domain::Shared).await)
    }

    async fn revoke_consent(&self, subject: &str, purpose: &str) -> Result<(), AwpError> {
        self.revoke(subject, purpose, None).await;
        Ok(())
    }
}

static STORE: OnceLock<ConsentStore> = OnceLock::new();

/// Install the process-wide consent store (boot). Errors if already installed.
pub fn init(store: ConsentStore) -> Result<(), ConsentStore> {
    STORE.set(store)
}

/// The process-wide consent store (an in-memory default when `init` was never called).
pub fn handle() -> &'static ConsentStore {
    STORE.get_or_init(ConsentStore::in_memory)
}

// ---- Postgres ----

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: String,
    category: String,
    world: String,
    purpose: String,
    granted_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

async fn load_pg(pool: &PgPool, user_id: &str) -> anyhow::Result<Vec<Consent>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, user_id, category, world, purpose, granted_at, revoked_at \
         FROM consents WHERE user_id = $1 ORDER BY granted_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Consent {
            id: r.id,
            user_id: r.user_id,
            category: r.category,
            world: Domain::parse(&r.world).unwrap_or_default(),
            purpose: r.purpose,
            granted_at: r.granted_at,
            revoked_at: r.revoked_at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_normalization() {
        assert_eq!(normalize_category(" Email ").as_deref(), Some("email"));
        assert_eq!(normalize_category("reading_list").as_deref(), Some("reading_list"));
        assert!(normalize_category("").is_none());
        assert!(normalize_category("Not Valid!").is_none());
        assert!(normalize_category(&"x".repeat(41)).is_none());
    }

    #[test]
    fn shared_grant_covers_both_worlds_and_world_grant_only_its_own() {
        let mk = |world| Consent {
            id: Uuid::new_v4(),
            user_id: "u".into(),
            category: "calendar".into(),
            world,
            purpose: "p".into(),
            granted_at: Utc::now(),
            revoked_at: None,
        };
        assert!(mk(Domain::Shared).covers(Domain::Work));
        assert!(mk(Domain::Shared).covers(Domain::Home));
        assert!(mk(Domain::Work).covers(Domain::Work));
        assert!(!mk(Domain::Work).covers(Domain::Home));
        assert!(mk(Domain::Work).covers(Domain::Shared), "a shared check accepts any active grant");
        let mut revoked = mk(Domain::Shared);
        revoked.revoked_at = Some(Utc::now());
        assert!(!revoked.covers(Domain::Work));
    }

    #[tokio::test]
    async fn grant_is_idempotent_and_revoke_scopes_by_world() {
        let store = ConsentStore::in_memory();
        let a = store.grant("u", "email", Domain::Work, "drafts").await.unwrap();
        let b = store.grant("u", "email", Domain::Work, "again").await.unwrap();
        assert_eq!(a.id, b.id);
        store.grant("u", "email", Domain::Home, "personal mail").await.unwrap();
        assert_eq!(store.active("u").await.len(), 2);
        assert_eq!(store.revoke("u", "email", Some(Domain::Work)).await, 1);
        assert!(!store.has("u", "email", Domain::Work).await);
        assert!(store.has("u", "email", Domain::Home).await);
        assert_eq!(store.revoke("u", "email", None).await, 1);
        assert_eq!(store.active("u").await.len(), 0);
        assert_eq!(store.list("u").await.len(), 2);
        assert_eq!(store.purge("u").await, 2);
    }
}
