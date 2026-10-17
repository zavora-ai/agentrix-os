//! `MemoryService` — known · assumed · recommended, scoped by domain, with provenance (S3-T2).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::Domain;
use crate::memory::crypto::Crypto;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Known,
    Assumed,
    Recommended,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Known => "known",
            Kind::Assumed => "assumed",
            Kind::Recommended => "recommended",
        }
    }
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "known" => Some(Kind::Known),
            "assumed" => Some(Kind::Assumed),
            "recommended" => Some(Kind::Recommended),
            _ => None,
        }
    }
    /// How the Mother Agent cites this kind in prose (§11.1).
    pub fn citation(&self) -> &'static str {
        match self {
            Kind::Known => "(you told me)",
            Kind::Assumed => "(I think)",
            Kind::Recommended => "(I suggest)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sensitivity {
    #[default]
    Normal,
    Sensitive,
    Health,
    Financial,
}

impl Sensitivity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Sensitivity::Normal => "normal",
            Sensitivity::Sensitive => "sensitive",
            Sensitivity::Health => "health",
            Sensitivity::Financial => "financial",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(Self::Normal),
            "sensitive" => Some(Self::Sensitive),
            "health" => Some(Self::Health),
            "financial" => Some(Self::Financial),
            _ => None,
        }
    }
    pub fn encrypted_at_rest(&self) -> bool {
        !matches!(self, Sensitivity::Normal)
    }
}

/// Where a memory item (or a change to it) came from.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Provenance {
    /// user_statement | user_confirmation | user_correction | integration | pattern | agent_proposal
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: DateTime<Utc>,
}

impl Provenance {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            agent: None,
            session_id: None,
            observation_id: None,
            note: None,
            at: Utc::now(),
        }
    }
    pub fn agent(mut self, a: &str) -> Self {
        self.agent = Some(a.into());
        self
    }
    pub fn session(mut self, s: Option<&str>) -> Self {
        self.session_id = s.map(str::to_string);
        self
    }
    pub fn note(mut self, n: &str) -> Self {
        self.note = Some(n.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: Uuid,
    pub user_id: String,
    pub domain: Domain,
    pub category: String,
    pub key: String,
    pub value: serde_json::Value,
    pub kind: Kind,
    pub confidence: Option<f32>,
    pub sensitivity: Sensitivity,
    pub source_agent: String,
    pub provenance: Vec<Provenance>,
    pub consent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl MemoryItem {
    pub fn is_live(&self) -> bool {
        self.deleted_at.is_none() && self.expires_at.map(|e| e > Utc::now()).unwrap_or(true)
    }

    /// One line for synthesis: `key: value (you told me)`.
    pub fn note(&self) -> String {
        let v = match &self.value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        format!("{}: {} {}", self.key, v, self.kind.citation())
    }
}

/// What an agent may read: its world's prefix plus `shared.` (or everything for shared agents).
#[derive(Clone, Copy, Debug)]
pub struct Scope {
    pub domain: Domain,
}

impl Scope {
    pub const MOTHER: Scope = Scope { domain: Domain::Shared };
    pub fn for_agent(agent_id: &str) -> Scope {
        Scope { domain: crate::tools::allowlist::catalog().world_for(agent_id) }
    }
    pub fn may_read(&self, item: &MemoryItem) -> bool {
        self.domain == Domain::Shared || item.domain == self.domain || item.domain == Domain::Shared
    }
    pub fn may_write(&self, domain: Domain) -> bool {
        self.domain == Domain::Shared || domain == self.domain || domain == Domain::Shared
    }
}

#[derive(Default)]
struct Inner {
    items: HashMap<String, Vec<MemoryItem>>,
    loaded: std::collections::HashSet<String>,
}

#[derive(Clone)]
pub struct MemoryService {
    inner: Arc<RwLock<Inner>>,
    pg: Option<PgPool>,
    crypto: Crypto,
}

pub struct NewItem<'a> {
    pub domain: Domain,
    pub category: &'a str,
    pub key: &'a str,
    pub value: serde_json::Value,
    pub sensitivity: Sensitivity,
    pub source_agent: &'a str,
    pub provenance: Provenance,
}

impl MemoryService {
    pub fn new(pg: Option<PgPool>, crypto: Crypto) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            pg,
            crypto,
        }
    }

    pub fn in_memory() -> Self {
        Self::new(None, Crypto::from_master("test-master-key"))
    }

    pub fn crypto(&self) -> &Crypto {
        &self.crypto
    }

    async fn ensure_loaded(&self, user_id: &str) {
        let Some(pool) = &self.pg else { return };
        if self.inner.read().await.loaded.contains(user_id) {
            return;
        }
        let rows = load_pg(pool, &self.crypto, user_id).await.unwrap_or_default();
        let mut guard = self.inner.write().await;
        guard.items.insert(user_id.to_string(), rows);
        guard.loaded.insert(user_id.to_string());
    }

    async fn persist(&self, item: &MemoryItem) {
        let Some(pool) = &self.pg else { return };
        if let Err(e) = upsert_pg(pool, &self.crypto, item).await {
            tracing::warn!("memory_items persist failed: {e:#}");
        }
    }

    /// Items readable within `scope`, optionally restricted to `keys`.
    pub async fn read(&self, user_id: &str, scope: Scope, keys: Option<&[String]>) -> Vec<MemoryItem> {
        self.ensure_loaded(user_id).await;
        let guard = self.inner.read().await;
        guard
            .items
            .get(user_id)
            .map(|items| {
                items
                    .iter()
                    .filter(|i| i.is_live() && scope.may_read(i))
                    .filter(|i| keys.map(|ks| ks.iter().any(|k| &i.key == k || i.key.starts_with(k))).unwrap_or(true))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn get(&self, user_id: &str, id: Uuid) -> Option<MemoryItem> {
        self.ensure_loaded(user_id).await;
        self.inner.read().await.items.get(user_id)?.iter().find(|i| i.id == id && i.deleted_at.is_none()).cloned()
    }

    async fn upsert(&self, user_id: &str, kind: Kind, confidence: Option<f32>, new: NewItem<'_>, overwrite_known: bool) -> MemoryItem {
        self.ensure_loaded(user_id).await;
        let item = {
            let mut guard = self.inner.write().await;
            let items = guard.items.entry(user_id.to_string()).or_default();
            if let Some(existing) = items.iter_mut().find(|i| i.domain == new.domain && i.key == new.key && i.deleted_at.is_none()) {
                if existing.kind == Kind::Known && !overwrite_known {
                    // A proposal never overrides what the user said.
                    return existing.clone();
                }
                existing.value = new.value;
                existing.kind = kind;
                existing.confidence = confidence;
                existing.category = new.category.to_string();
                existing.sensitivity = new.sensitivity;
                existing.source_agent = new.source_agent.to_string();
                existing.provenance.push(new.provenance);
                existing.updated_at = Utc::now();
                existing.clone()
            } else {
                let now = Utc::now();
                let item = MemoryItem {
                    id: Uuid::new_v4(),
                    user_id: user_id.to_string(),
                    domain: new.domain,
                    category: new.category.to_string(),
                    key: new.key.to_string(),
                    value: new.value,
                    kind,
                    confidence,
                    sensitivity: new.sensitivity,
                    source_agent: new.source_agent.to_string(),
                    provenance: vec![new.provenance],
                    consent_id: None,
                    created_at: now,
                    updated_at: now,
                    expires_at: None,
                    deleted_at: None,
                };
                items.push(item.clone());
                item
            }
        };
        self.persist(&item).await;
        item
    }

    /// The user said it (or an authorized integration reported it): `known`.
    pub async fn remember(&self, user_id: &str, new: NewItem<'_>) -> MemoryItem {
        self.upsert(user_id, Kind::Known, None, new, true).await
    }

    /// An agent or the intelligence layer inferred it: `assumed`. Never overrides a `known` item.
    pub async fn propose(&self, user_id: &str, confidence: f32, new: NewItem<'_>) -> MemoryItem {
        self.upsert(user_id, Kind::Assumed, Some(confidence.clamp(0.0, 1.0)), new, false).await
    }

    /// The system is suggesting it: `recommended`.
    pub async fn recommend(&self, user_id: &str, new: NewItem<'_>) -> MemoryItem {
        self.upsert(user_id, Kind::Recommended, None, new, false).await
    }

    /// User confirms an assumed/recommended item → known.
    pub async fn confirm(&self, user_id: &str, id: Uuid, session_id: Option<&str>) -> Option<MemoryItem> {
        self.mutate(user_id, id, |i| {
            i.kind = Kind::Known;
            i.confidence = None;
            i.provenance.push(Provenance::new("user_confirmation").session(session_id));
        })
        .await
    }

    /// User corrects the value → known with the new value.
    pub async fn correct(&self, user_id: &str, id: Uuid, value: serde_json::Value, session_id: Option<&str>) -> Option<MemoryItem> {
        self.mutate(user_id, id, |i| {
            i.value = value;
            i.kind = Kind::Known;
            i.confidence = None;
            i.provenance.push(Provenance::new("user_correction").session(session_id));
        })
        .await
    }

    /// Soft delete.
    pub async fn forget(&self, user_id: &str, id: Uuid) -> Option<MemoryItem> {
        self.mutate(user_id, id, |i| i.deleted_at = Some(Utc::now())).await
    }

    /// Forget every live item whose key or value mentions `phrase` (chat: "forget …").
    pub async fn forget_matching(&self, user_id: &str, phrase: &str) -> Vec<MemoryItem> {
        let ids: Vec<Uuid> = self
            .read(user_id, Scope::MOTHER, None)
            .await
            .into_iter()
            .filter(|i| {
                let p = phrase.to_lowercase();
                i.key.to_lowercase().contains(&p) || i.value.to_string().to_lowercase().contains(&p)
            })
            .map(|i| i.id)
            .collect();
        let mut out = Vec::new();
        for id in ids {
            if let Some(i) = self.forget(user_id, id).await {
                out.push(i);
            }
        }
        out
    }

    async fn mutate(&self, user_id: &str, id: Uuid, f: impl FnOnce(&mut MemoryItem)) -> Option<MemoryItem> {
        self.ensure_loaded(user_id).await;
        let item = {
            let mut guard = self.inner.write().await;
            let item = guard.items.get_mut(user_id)?.iter_mut().find(|i| i.id == id && i.deleted_at.is_none())?;
            f(item);
            item.updated_at = Utc::now();
            item.clone()
        };
        self.persist(&item).await;
        Some(item)
    }

    /// Everything (live) with provenance — the user's export.
    pub async fn export(&self, user_id: &str) -> Vec<MemoryItem> {
        self.read(user_id, Scope::MOTHER, None).await
    }

    /// Hard delete everything for the user.
    pub async fn purge(&self, user_id: &str) -> usize {
        self.ensure_loaded(user_id).await;
        let n = {
            let mut guard = self.inner.write().await;
            guard.items.remove(user_id).map(|v| v.len()).unwrap_or(0)
        };
        if let Some(pool) = &self.pg
            && let Err(e) = sqlx::query("DELETE FROM memory_items WHERE user_id = $1").bind(user_id).execute(pool).await
        {
            tracing::warn!("memory purge failed: {e:#}");
        }
        n
    }

    /// Short citations for synthesis (§11.1), most recently updated first.
    pub async fn notes_for(&self, user_id: &str, scope: Scope, limit: usize) -> Vec<String> {
        let mut items = self.read(user_id, scope, None).await;
        items.sort_by_key(|i| std::cmp::Reverse(i.updated_at));
        items.iter().filter(|i| i.kind != Kind::Recommended).take(limit).map(MemoryItem::note).collect()
    }

    /// A profile value (`shared.profile.<name>`) as a string, if known or assumed.
    pub async fn profile(&self, user_id: &str, name: &str) -> Option<String> {
        let key = format!("profile.{name}");
        self.read(user_id, Scope::MOTHER, Some(&[key]))
            .await
            .into_iter()
            .find(|i| i.kind != Kind::Recommended)
            .and_then(|i| i.value.as_str().map(str::to_string))
    }
}

// ---- Postgres ----

/// The `value` and `provenance` columns as stored. A sensitive item's value is encrypted and
/// its provenance keeps no free-text notes, which would otherwise sit in plain JSONB.
pub(crate) fn stored_columns(crypto: &Crypto, item: &MemoryItem) -> anyhow::Result<(serde_json::Value, serde_json::Value)> {
    if !item.sensitivity.encrypted_at_rest() {
        return Ok((item.value.clone(), serde_json::to_value(&item.provenance)?));
    }
    let provenance: Vec<Provenance> = item.provenance.iter().cloned().map(|p| Provenance { note: None, ..p }).collect();
    Ok((crypto.encrypt(&item.user_id, &item.value)?, serde_json::to_value(provenance)?))
}

async fn upsert_pg(pool: &PgPool, crypto: &Crypto, item: &MemoryItem) -> anyhow::Result<()> {
    let (value, provenance) = stored_columns(crypto, item)?;
    sqlx::query(
        "INSERT INTO memory_items (id, user_id, domain, category, key, value, kind, confidence, sensitivity, source_agent, provenance, consent_id, created_at, updated_at, expires_at, deleted_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
         ON CONFLICT (user_id, domain, key) DO UPDATE SET value = $6, kind = $7, confidence = $8, sensitivity = $9, source_agent = $10, provenance = $11, updated_at = $14, expires_at = $15, deleted_at = $16",
    )
    .bind(item.id)
    .bind(&item.user_id)
    .bind(item.domain.as_str())
    .bind(&item.category)
    .bind(&item.key)
    .bind(value)
    .bind(item.kind.as_str())
    .bind(item.confidence)
    .bind(item.sensitivity.as_str())
    .bind(&item.source_agent)
    .bind(provenance)
    .bind(item.consent_id)
    .bind(item.created_at)
    .bind(item.updated_at)
    .bind(item.expires_at)
    .bind(item.deleted_at)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    user_id: String,
    domain: String,
    category: String,
    key: String,
    value: serde_json::Value,
    kind: String,
    confidence: Option<f32>,
    sensitivity: String,
    source_agent: String,
    provenance: serde_json::Value,
    consent_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    deleted_at: Option<DateTime<Utc>>,
}

async fn load_pg(pool: &PgPool, crypto: &Crypto, user_id: &str) -> anyhow::Result<Vec<MemoryItem>> {
    let rows: Vec<Row> = sqlx::query_as("SELECT * FROM memory_items WHERE user_id = $1 AND deleted_at IS NULL")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            Some(MemoryItem {
                id: r.id,
                value: crypto.decrypt(&r.user_id, &r.value).ok()?,
                user_id: r.user_id,
                domain: Domain::parse(&r.domain)?,
                category: r.category,
                key: r.key,
                kind: Kind::parse(&r.kind)?,
                confidence: r.confidence,
                sensitivity: Sensitivity::parse(&r.sensitivity).unwrap_or_default(),
                source_agent: r.source_agent,
                provenance: serde_json::from_value(r.provenance).unwrap_or_default(),
                consent_id: r.consent_id,
                created_at: r.created_at,
                updated_at: r.updated_at,
                expires_at: r.expires_at,
                deleted_at: r.deleted_at,
            })
        })
        .collect())
}
