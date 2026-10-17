use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use adk_runner::Runner;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::ambient::AmbientStore;
use crate::events::sse::{to_event, FieldEvent};
use crate::orchestrator::coordinator;
use crate::state::SessionStore;

fn proactive_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"🔎","title":"Research","agent":"research.agent",
            "stream":["Reading sources on ABC Corp…","Cross-checking claims…"]
        }),
        serde_json::json!({
            "glyph":"🌐","title":"Scout","agent":"scout.agent",
            "stream":["Watching prices for you…"]
        }),
        serde_json::json!({
            "glyph":"🎨","title":"Maker","agent":"maker.agent","waitsFor":2,"attention":true,
            "stream":["Making something you'd like…"]
        }),
    ]
}

fn fallback_resolve(id: &str, record: Option<&crate::ambient::AmbientAgentRecord>) -> serde_json::Value {
    if let Some(rec) = record {
        if let Some(resolve) = &rec.resolve {
            return resolve.clone();
        }
        return serde_json::json!({
            "big": rec.status.clone(),
            "sub": rec.task.clone(),
            "actions": ["Open"]
        });
    }
    crate::orchestrator::fallback::unavailable(format!("No {id} result yet"))
}

pub fn stream_proactive(
    _runner: Option<Arc<Runner>>,
    user_id: String,
    session_id: String,
    intent: String,
    store: AmbientStore,
    sessions: Option<SessionStore>,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    let (tx, rx) = mpsc::channel(64);
    let cards = proactive_cards();

    tokio::spawn(async move {
        if store.dnd().await {
            if let Some(ref s) = sessions {
                s.set_scenario(&session_id, "proactive", Some(&intent)).await;
            }
            let _ = tx
                .send(Ok(to_event(&FieldEvent::SuzySummary {
                    key: "proactive".into(),
                    html: "Do not disturb is on — background agents are paused. Turn off DND to see what they found.".into(),
                })))
                .await;
            let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
            return;
        }

        if let Some(ref s) = sessions {
            s.set_scenario(&session_id, "proactive", Some(&intent)).await;
        }

        let agents = store.list().await;
        let agent_map: std::collections::HashMap<_, _> =
            agents.iter().map(|a| (a.id.as_str(), a)).collect();

        let _ = tx
            .send(Ok(to_event(&FieldEvent::Scenario {
                key: "proactive".into(),
                text: intent.clone(),
                total_cards: cards.len(),
            })))
            .await;

        for (index, card) in cards.iter().enumerate() {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::CardSpawn {
                    index,
                    card: card.clone(),
                    domain: crate::domain::Domain::for_card("proactive", card),
                })))
                .await;
        }

        let ids = ["research", "scout", "maker"];
        for (index, id) in ids.iter().enumerate() {
            if let Some(rec) = agent_map.get(id) {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::CardStatus {
                        index,
                        status: rec.status.clone(),
                        line: Some(rec.task.clone()),
                    })))
                    .await;
            }
            tokio::time::sleep(Duration::from_millis(400 + index as u64 * 200)).await;

            let resolve = fallback_resolve(id, agent_map.get(id).copied());
            if let Some(ref s) = sessions {
                crate::orchestrator::persist::card_resolve(
                    s,
                    &session_id,
                    index,
                    cards.get(index).cloned().unwrap_or_default(),
                    resolve.clone(),
                    false,
                )
                .await;
            }
            let _ = tx
                .send(Ok(to_event(&FieldEvent::CardResolve { index, resolve })))
                .await;
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
        if let Some(ref s) = sessions {
            coordinator::emit_suzy_and_suggest(
                &tx,
                suzy_runner.as_ref(),
                s,
                &session_id,
                &user_id,
                "proactive",
            )
            .await;
        }
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    ReceiverStream::new(rx)
}