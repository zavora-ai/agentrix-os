//! Suzy summaries and tour suggestions.

use std::sync::Arc;

use adk_runner::Runner;
use tokio::sync::mpsc::Sender;

use crate::events::sse::{to_event, FieldEvent};
use crate::html;
use crate::orchestrator::fallback;
use crate::scenarios::tour;
use crate::state::{SessionRecord, SessionStore};

/// Suzy's summary of a live scenario. Without Suzy, or when she fails, the summary restates the
/// session's own card results. The scripted tour summaries belong to the no-key mock stream only
/// (`events::mock`); they are never a fallback for live cards (rule 1).
pub async fn suzy_html(
    suzy_runner: Option<&Arc<Runner>>,
    store: &SessionStore,
    session_id: &str,
    user_id: &str,
) -> String {
    let record = store.get(session_id).await;
    if let (Some(runner), Some(record)) = (suzy_runner, record.as_ref()) {
        match crate::agents::suzy::summarize(runner, user_id, session_id, record).await {
            Ok(h) => return h,
            Err(e) => tracing::warn!("Suzy summary fallback ({e:#})"),
        }
    }
    fallback_summary(record.as_ref())
}

/// Deterministic summary from the session's resolved cards: titles and their headline values only,
/// tags stripped and the text escaped, so the only markup is the bold title added here.
pub fn fallback_summary(record: Option<&SessionRecord>) -> String {
    let cards: Vec<_> = record
        .into_iter()
        .flat_map(|r| r.cards.iter())
        .filter(|c| !c.removed)
        .collect();
    if let Some(r) = record.filter(|_| cards.iter().any(|c| fallback::is_simulated(&c.card))) {
        return fallback::simulated_summary(r.scenario.as_deref().unwrap_or("field"));
    }
    let facts: Vec<String> = cards
        .iter()
        .filter_map(|c| {
            let title = c.card.get("title")?.as_str()?;
            let resolve = c.resolve.as_ref()?;
            let value = resolve.get("big").and_then(|v| v.as_str()).map(str::to_string).or_else(|| {
                resolve.get("lines")?.as_array()?.first()?.as_str().map(crate::mother::synth::strip_tags)
            })?;
            let value = crate::mother::synth::strip_tags(&value);
            Some(format!("<b>{}</b> {}", html::escape(title), html::escape(value.trim_end_matches('.'))))
        })
        .collect();
    if facts.is_empty() {
        "I couldn't put a summary together this time — the cards show what came back.".into()
    } else {
        format!("Here's what came back: {}.", facts.join(" · "))
    }
}

pub async fn emit_suzy_and_suggest(
    tx: &Sender<Result<axum::response::sse::Event, std::convert::Infallible>>,
    suzy_runner: Option<&Arc<Runner>>,
    store: &SessionStore,
    session_id: &str,
    user_id: &str,
    scenario: &str,
) {
    let html = suzy_html(suzy_runner, store, session_id, user_id).await;
    emit_summary_and_suggest(tx, scenario, html).await;
}

/// Send a ready summary plus the tour's action suggestion for `scenario`.
pub async fn emit_summary_and_suggest(
    tx: &Sender<Result<axum::response::sse::Event, std::convert::Infallible>>,
    scenario: &str,
    html: String,
) {
    let _ = tx
        .send(Ok(to_event(&FieldEvent::SuzySummary {
            key: scenario.into(),
            html,
        })))
        .await;

    if let Some(text) = tour::action_prompt(scenario) {
        let _ = tx
            .send(Ok(to_event(&FieldEvent::Suggest {
                text: text.into(),
                kind: "action".into(),
            })))
            .await;
    }
}

pub async fn emit_tour_advance(
    tx: &Sender<Result<axum::response::sse::Event, std::convert::Infallible>>,
    current: &str,
) {
    if let Some(next) = tour::next_scenario(current) {
        if let Some(text) = tour::scenario_prompt(next) {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::Suggest {
                    text: text.into(),
                    kind: "scenario".into(),
                })))
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::state::{CardRecord, SessionArtifacts};

    fn record(cards: Vec<(serde_json::Value, serde_json::Value)>) -> SessionRecord {
        SessionRecord {
            session_id: "s".into(),
            user_id: "u".into(),
            scenario: Some("morning".into()),
            origin_text: None,
            artifacts: SessionArtifacts::default(),
            cards: cards
                .into_iter()
                .enumerate()
                .map(|(index, (card, resolve))| CardRecord {
                    index,
                    card,
                    status: "done".into(),
                    resolve: Some(resolve),
                    pinned: false,
                    removed: false,
                    domain: Default::default(),
                })
                .collect(),
            agents_active: vec![],
            agents_resting: vec![],
            chat_history: vec![],
        }
    }

    #[test]
    fn fallback_summary_escapes_card_text() {
        let line = fallback::agent_lines("<img src=x onerror=alert(1)> wants a reply", 1).remove(0);
        let r = record(vec![
            (json!({"title": "Needs you"}), json!({"lines": [line], "actions": []})),
            (json!({"title": "Brief <i>"}), json!({"big": "AT&T <script>alert(1)</script>", "sub": "", "actions": []})),
        ]);
        assert_eq!(
            fallback_summary(Some(&r)),
            "Here's what came back: <b>Needs you</b> • &lt;img src=x onerror=alert(1)&gt; wants a reply · \
             <b>Brief &lt;i&gt;</b> AT&amp;T alert(1)."
        );
    }
}
