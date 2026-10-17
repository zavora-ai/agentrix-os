use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub type Db = PgPool;

pub async fn connect(database_url: &str) -> anyhow::Result<Db> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct User {
    pub id: uuid::Uuid,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub provider: String,
    pub provider_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn find_user_by_email(db: &Db, email: &str) -> anyhow::Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, email, name, avatar_url, provider, provider_id, created_at FROM users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(db)
    .await?)
}

pub async fn find_user_by_id(db: &Db, id: uuid::Uuid) -> anyhow::Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, email, name, avatar_url, provider, provider_id, created_at FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?)
}

pub async fn find_user_by_provider(
    db: &Db,
    provider: &str,
    provider_id: &str,
) -> anyhow::Result<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT id, email, name, avatar_url, provider, provider_id, created_at FROM users WHERE provider = $1 AND provider_id = $2",
    )
    .bind(provider)
    .bind(provider_id)
    .fetch_optional(db)
    .await?)
}

pub async fn create_user(
    db: &Db,
    email: &str,
    name: Option<&str>,
    provider: &str,
    provider_id: Option<&str>,
    avatar_url: Option<&str>,
) -> anyhow::Result<User> {
    Ok(sqlx::query_as::<_, User>(
        "INSERT INTO users (email, name, provider, provider_id, avatar_url) VALUES ($1, $2, $3, $4, $5) RETURNING id, email, name, avatar_url, provider, provider_id, created_at",
    )
    .bind(email)
    .bind(name)
    .bind(provider)
    .bind(provider_id)
    .bind(avatar_url)
    .fetch_one(db)
    .await?)
}
/// First-touch bookkeeping for stores that load a user's rows from Postgres into memory on the
/// first request for that user (memory, tasks, consents, permissions, pending actions).
///
/// A user counts as loaded only after a read succeeded. After a failed read the next touch tries
/// again, at most once per [`UserLoads::RETRY_AFTER`], so a database outage neither leaves the
/// user marked loaded with nothing in memory nor makes every request wait on the pool.
#[derive(Debug, Default)]
pub struct UserLoads {
    loaded: std::collections::HashSet<String>,
    failed_at: std::collections::HashMap<String, std::time::Instant>,
}

impl UserLoads {
    pub const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

    /// Whether this touch should read the user's rows.
    pub fn should_load(&self, user_id: &str) -> bool {
        !self.loaded.contains(user_id)
            && self
                .failed_at
                .get(user_id)
                .is_none_or(|at| at.elapsed() >= Self::RETRY_AFTER)
    }

    pub fn is_loaded(&self, user_id: &str) -> bool {
        self.loaded.contains(user_id)
    }

    pub fn succeeded(&mut self, user_id: &str) {
        self.failed_at.remove(user_id);
        self.loaded.insert(user_id.to_string());
    }

    pub fn failed(&mut self, user_id: &str) {
        self.failed_at.insert(user_id.to_string(), std::time::Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_loads_mark_loaded_only_on_success_and_retry_after_a_failure() {
        let mut loads = UserLoads::default();
        assert!(loads.should_load("u"));
        loads.failed("u");
        assert!(!loads.is_loaded("u"), "a failed read never marks the user loaded");
        assert!(!loads.should_load("u"), "no retry storm right after a failure");
        loads.failed_at.insert("u".into(), std::time::Instant::now() - UserLoads::RETRY_AFTER);
        assert!(loads.should_load("u"), "retried once the back-off passed");
        loads.succeeded("u");
        assert!(loads.is_loaded("u") && !loads.should_load("u"));
    }
}
