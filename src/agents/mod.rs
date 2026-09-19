pub mod ambient;
pub mod brief;
pub mod calendar;
pub mod career;
pub mod combine;
pub mod deck;
pub mod docs;
pub mod excel;
pub mod family;
pub mod gemini;
pub mod inbox;
pub mod live;
pub mod lisbon;
pub mod morning;
pub mod people;
pub mod personal_productivity;
pub mod personal_social;
pub mod professional_social;
pub mod router;
pub mod slides;
pub mod stub;
pub mod suzy;
pub mod week;
/// Make sure the adk session `(runner.app_name(), user_id, session_id)` exists before
/// `runner.run`, creating it on first use.
///
/// Runners are keyed per app (`agentrix-os-router`, `agentrix-os-suzy`, `agentrix-os-morning`, …)
/// but `POST /api/sessions` only creates the `agentrix-os` session. Every other runner used to
/// fail with `session.not_found` and fall back — to keywords (router), templates (Mother
/// synthesis), the static summary (Suzy) or an `error` event (workflows) — while the logs still
/// said the LLM had answered.
pub async fn ensure_runner_session(runner: &adk_runner::Runner, user_id: &str, session_id: &str) {
    let service = runner.session_service();
    let app = runner.app_name().to_string();
    let exists = service
        .get(adk_session::GetRequest {
            app_name: app.clone(),
            user_id: user_id.to_string(),
            session_id: session_id.to_string(),
            num_recent_events: None,
            after: None,
        })
        .await
        .is_ok();
    if exists {
        return;
    }
    if let Err(e) = service
        .create(adk_session::CreateRequest {
            app_name: app.clone(),
            user_id: user_id.to_string(),
            session_id: Some(session_id.to_string()),
            state: Default::default(),
        })
        .await
    {
        // A concurrent caller may have created it first; the run itself reports anything else.
        tracing::debug!("session create for {app}/{session_id}: {e}");
    }
}
