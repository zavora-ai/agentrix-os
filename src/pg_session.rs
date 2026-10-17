use adk_core::{
    identity::{AdkIdentity, AppName, SessionId, UserId},
    Event,
};
use adk_session::{AppendEventRequest, CreateRequest, DeleteRequest, GetRequest, ListRequest, SessionService};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;

pub struct PgSessionService {
    pool: PgPool,
}

impl PgSessionService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append to the session addressed by its full `(app_name, user_id, id)` key. One session id
    /// exists under several app names (`agents::ensure_runner_session` reuses the UI session id
    /// for every runner), so the id alone does not say whose history an event belongs to.
    async fn append(&self, app_name: &str, user_id: &str, session_id: &str, event: &Event) -> adk_core::Result<()> {
        let event_json = serde_json::to_value(event).map_err(pg_err)?;
        let mut tx = self.pool.begin().await.map_err(pg_err)?;
        let touched = sqlx::query("UPDATE agent_sessions SET updated_at = NOW() WHERE app_name = $1 AND user_id = $2 AND id = $3")
            .bind(app_name).bind(user_id).bind(session_id)
            .execute(&mut *tx).await
            .map_err(pg_err)?;
        if touched.rows_affected() == 0 {
            return Err(adk_core::AdkError::not_found(
                adk_core::ErrorComponent::Session,
                "pg_session.not_found",
                format!("session {app_name}/{session_id} not found"),
            ));
        }
        sqlx::query("INSERT INTO agent_events (session_app_name, session_user_id, session_id, event_data) VALUES ($1, $2, $3, $4)")
            .bind(app_name).bind(user_id).bind(session_id).bind(&event_json)
            .execute(&mut *tx).await
            .map_err(pg_err)?;
        tx.commit().await.map_err(pg_err)
    }
}

fn pg_err(e: impl std::fmt::Display) -> adk_core::AdkError {
    adk_core::AdkError::internal(adk_core::ErrorComponent::Session, "pg_session.error", e.to_string())
}

#[async_trait]
impl SessionService for PgSessionService {
    async fn create(&self, req: CreateRequest) -> adk_core::Result<Box<dyn adk_session::Session>> {
        let session_id = req
            .session_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let state_json = serde_json::to_value(&req.state).unwrap_or_default();

        sqlx::query("INSERT INTO agent_sessions (id, app_name, user_id, state) VALUES ($1, $2, $3, $4) ON CONFLICT (app_name, user_id, id) DO UPDATE SET state = $4, updated_at = NOW()")
            .bind(&session_id).bind(&req.app_name).bind(&req.user_id).bind(&state_json)
            .execute(&self.pool).await
            .map_err(|e| adk_core::AdkError::internal(adk_core::ErrorComponent::Session, "pg_session.error", e.to_string()))?;

        Ok(Box::new(PgSession {
            identity: AdkIdentity {
                app_name: AppName::try_from(req.app_name.as_str()).unwrap(),
                user_id: UserId::try_from(req.user_id.as_str()).unwrap(),
                session_id: SessionId::try_from(session_id.as_str()).unwrap(),
            },
            state: req.state,
            events: Vec::new(),
            updated_at: Utc::now(),
        }))
    }

    async fn get(&self, req: GetRequest) -> adk_core::Result<Box<dyn adk_session::Session>> {
        let row = sqlx::query_as::<_, SessionRow>("SELECT id, app_name, user_id, state, updated_at FROM agent_sessions WHERE app_name = $1 AND user_id = $2 AND id = $3")
            .bind(&req.app_name).bind(&req.user_id).bind(&req.session_id)
            .fetch_optional(&self.pool).await
            .map_err(|e| adk_core::AdkError::internal(adk_core::ErrorComponent::Session, "pg_session.error", e.to_string()))?
            .ok_or_else(|| adk_core::AdkError::not_found(adk_core::ErrorComponent::Session, "pg_session.not_found", format!("session {} not found", req.session_id)))?;

        let event_rows: Vec<(Value,)> = if req.num_recent_events.is_some() {
            let n = req.num_recent_events.unwrap() as i64;
            sqlx::query_as("SELECT event_data FROM agent_events WHERE session_app_name = $1 AND session_user_id = $2 AND session_id = $3 ORDER BY id DESC LIMIT $4")
                .bind(&req.app_name).bind(&req.user_id).bind(&req.session_id).bind(n)
                .fetch_all(&self.pool).await
                .map_err(|e| adk_core::AdkError::internal(adk_core::ErrorComponent::Session, "pg_session.error", e.to_string()))?
        } else {
            sqlx::query_as("SELECT event_data FROM agent_events WHERE session_app_name = $1 AND session_user_id = $2 AND session_id = $3 ORDER BY id ASC")
                .bind(&req.app_name).bind(&req.user_id).bind(&req.session_id)
                .fetch_all(&self.pool).await
                .map_err(|e| adk_core::AdkError::internal(adk_core::ErrorComponent::Session, "pg_session.error", e.to_string()))?
        };

        let mut events: Vec<Event> = event_rows
            .into_iter()
            .filter_map(|(v,)| serde_json::from_value(v).ok())
            .collect();
        if req.num_recent_events.is_some() {
            events.reverse();
        }

        let state: HashMap<String, Value> = serde_json::from_value(row.state).unwrap_or_default();

        Ok(Box::new(PgSession {
            identity: AdkIdentity {
                app_name: AppName::try_from(row.app_name.as_str()).unwrap(),
                user_id: UserId::try_from(row.user_id.as_str()).unwrap(),
                session_id: SessionId::try_from(row.id.as_str()).unwrap(),
            },
            state,
            events,
            updated_at: row.updated_at,
        }))
    }

    async fn list(&self, req: ListRequest) -> adk_core::Result<Vec<Box<dyn adk_session::Session>>> {
        let rows = sqlx::query_as::<_, SessionRow>("SELECT id, app_name, user_id, state, updated_at FROM agent_sessions WHERE app_name = $1 AND user_id = $2 ORDER BY updated_at DESC")
            .bind(&req.app_name).bind(&req.user_id)
            .fetch_all(&self.pool).await
            .map_err(|e| adk_core::AdkError::internal(adk_core::ErrorComponent::Session, "pg_session.error", e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let state: HashMap<String, Value> =
                    serde_json::from_value(row.state).unwrap_or_default();
                Box::new(PgSession {
                    identity: AdkIdentity {
                        app_name: AppName::try_from(row.app_name.as_str()).unwrap(),
                        user_id: UserId::try_from(row.user_id.as_str()).unwrap(),
                        session_id: SessionId::try_from(row.id.as_str()).unwrap(),
                    },
                    state,
                    events: Vec::new(),
                    updated_at: row.updated_at,
                }) as Box<dyn adk_session::Session>
            })
            .collect())
    }

    async fn delete(&self, req: DeleteRequest) -> adk_core::Result<()> {
        sqlx::query("DELETE FROM agent_sessions WHERE app_name = $1 AND user_id = $2 AND id = $3")
            .bind(&req.app_name)
            .bind(&req.user_id)
            .bind(&req.session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                adk_core::AdkError::internal(
                    adk_core::ErrorComponent::Session,
                    "pg_session.error",
                    e.to_string(),
                )
            })?;
        Ok(())
    }

    async fn append_event_for_identity(&self, req: AppendEventRequest) -> adk_core::Result<()> {
        let id = &req.identity;
        self.append(id.app_name.as_ref(), id.user_id.as_ref(), id.session_id.as_ref(), &req.event).await
    }

    /// Legacy id-only addressing. Appends only when exactly one session has this id; an id
    /// shared across app names is refused rather than written into an arbitrary app's history.
    async fn append_event(&self, session_id: &str, event: Event) -> adk_core::Result<()> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT app_name, user_id FROM agent_sessions WHERE id = $1 LIMIT 2")
            .bind(session_id)
            .fetch_all(&self.pool)
            .await
            .map_err(pg_err)?;
        match rows.as_slice() {
            [(app_name, user_id)] => self.append(app_name, user_id, session_id, &event).await,
            [] => Err(adk_core::AdkError::not_found(
                adk_core::ErrorComponent::Session,
                "pg_session.not_found",
                format!("session {session_id} not found"),
            )),
            _ => Err(adk_core::AdkError::internal(
                adk_core::ErrorComponent::Session,
                "pg_session.ambiguous",
                format!("session id {session_id} exists under several apps; append with its full identity"),
            )),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    app_name: String,
    user_id: String,
    state: Value,
    updated_at: DateTime<Utc>,
}

struct PgSession {
    identity: AdkIdentity,
    state: HashMap<String, Value>,
    events: Vec<Event>,
    updated_at: DateTime<Utc>,
}

impl adk_session::Session for PgSession {
    fn id(&self) -> &str {
        self.identity.session_id.as_ref()
    }
    fn app_name(&self) -> &str {
        self.identity.app_name.as_ref()
    }
    fn user_id(&self) -> &str {
        self.identity.user_id.as_ref()
    }
    fn state(&self) -> &dyn adk_session::State {
        self
    }
    fn events(&self) -> &dyn adk_session::Events {
        self
    }
    fn last_update_time(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

impl adk_session::State for PgSession {
    fn get(&self, key: &str) -> Option<Value> {
        self.state.get(key).cloned()
    }
    fn set(&mut self, key: String, value: Value) {
        self.state.insert(key, value);
    }
    fn all(&self) -> HashMap<String, Value> {
        self.state.clone()
    }
}

impl adk_session::Events for PgSession {
    fn all(&self) -> Vec<Event> {
        self.events.clone()
    }
    fn len(&self) -> usize {
        self.events.len()
    }
    fn at(&self, index: usize) -> Option<&Event> {
        self.events.get(index)
    }
}