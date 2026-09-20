use std::convert::Infallible;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::sse::{to_event, ConductStep, FieldEvent};
use crate::orchestrator::coordinator;
use crate::scenarios::{self, tour};
use crate::state::SessionStore;

pub fn pick_scenario(text: &str) -> &'static str {
    let t = text.to_lowercase();
    if t.contains("deck")
        || t.contains("pitch")
        || t.contains("presentation")
        || t.contains("slides")
        || t.contains("document")
        || t.contains("report")
    {
        return "deck";
    }
    if t.contains("people")
        || t.contains("team")
        || t.contains("family")
        || t.contains("catch me up")
    {
        return "people";
    }
    if t.contains("live")
        || t.contains("news")
        || t.contains("happening")
        || t.contains("market")
    {
        return "live";
    }
    if t.contains("proactive")
        || t.contains("background")
        || t.contains("working on")
        || t.contains("found")
    {
        return "proactive";
    }
    if t.contains("morning") || t.contains("ready") || t.contains("start my day") {
        return "morning";
    }
    if t.contains("week") || t.contains("summar") {
        return "week";
    }
    "lisbon"
}

pub fn suzy_summary(key: &str) -> &'static str {
    match key {
        "lisbon" => "You can be in Lisbon Friday night for <b>$284</b>, staying at a riverside Alfama loft for <b>$96 a night</b>. I drafted a 3-day plan — just say the word and I’ll hold both.",
        "morning" => "Good morning. You have <b>3 meetings</b>, a free window <b>12–2pm</b>, and <b>2 emails</b> that actually need you. Everything else I’ve handled.",
        "week" => "This week you spent <b>$1,240</b> — mostly travel and groceries — slept <b>6.1h</b> a night, and shipped <b>14 commits</b>. My one suggestion: protect Tuesday mornings.",
        "deck" => "Your numbers, story and slides are ready. Say <b>combine</b> and I’ll merge them into one finished deck.",
        "people" => "Three people are waiting on you — <b>Alex</b>, <b>Priya</b> and <b>#dev-team</b>. I’ve drafted replies and prepped your 3pm one-on-one.",
        "live" => "Here’s what’s happening — rates paused, chips rallying, your portfolio <b>up 0.8%</b>, and a keynote is live now.",
        "proactive" => "While you were away I did a few things — researched <b>ABC Corp</b>, caught a <b>12% price drop</b>, and made a couple of things you might like.",
        _ => "All set — ask me for anything else.",
    }
}

fn scenario_cards(key: &str) -> &'static str {
    match key {
        "deck" => r#"[
          {"glyph":"📊","title":"Auto-Excel","agent":"auto-excel","surface":"excel","delay":0,
           "stream":["Pulling Q3 numbers…","Building the revenue chart…"],
           "resolve":{"big":"+38% QoQ","sub":"Revenue model · 4 sheets · 1 chart","actions":["Save","Open"]}},
          {"glyph":"📝","title":"Auto-Docs","agent":"auto-docs","surface":"docs","delay":250,
           "stream":["Drafting the narrative…","Tightening the story…"],
           "resolve":{"big":"1,240 words","sub":"Exec summary · problem · ask","actions":["Save","Open"]}},
          {"glyph":"🖼️","title":"Auto-Slides","agent":"auto-slides","surface":"slides","delay":500,"waitsFor":2,
           "stream":["Waiting for numbers & story…"],
           "resolve":{"big":"10 slides","sub":"Pitch deck · ready to combine","actions":["Save deck","Present"]}}
        ]"#,
        "morning" => r#"[
          {"glyph":"📅","title":"Today","agent":"calendar.agent","delay":0,
           "stream":["Reading your calendar…"],
           "resolve":{"big":"3 meetings","sub":"First: Standup 9:30 · gap 12–2pm free","actions":["Open","Reschedule"]}},
          {"glyph":"✉️","title":"Needs you","agent":"inbox.agent","delay":250,"attention":true,
           "stream":["Triaging 38 new emails…","Surfacing only what matters…"],
           "resolve":{"big":"2 to reply","sub":"Client contract · Mara re: launch date","actions":["Draft replies","Snooze"]}},
          {"glyph":"📰","title":"Brief","agent":"news.agent","delay":500,"waitsFor":2,
           "stream":["Composing your brief…"],
           "resolve":{"lines":["<b>Fri</b> — arrive, sunset at Miradouro","<b>Sat</b> — Sintra day trip","<b>Sun</b> — fly home"],"actions":["Read aloud","Dismiss"]}}
        ]"#,
        "live" => r#"[
          {"glyph":"📰","title":"Headlines","agent":"news.agent","delay":0,
           "stream":["Scanning live sources…"],
           "resolve":{"big":"3 big stories","sub":"Rates paused · chips rally","actions":["Read aloud","Open"]}},
          {"glyph":"📈","title":"Markets","agent":"markets.agent","delay":250,
           "stream":["Checking your watchlist…"],
           "resolve":{"big":"+0.6% pre-open","sub":"Portfolio +0.8%","actions":["Details","Set alert"]}},
          {"glyph":"🔴","title":"Now","agent":"live.agent","delay":500,"waitsFor":2,
           "stream":["Tuning into live events…"],
           "resolve":{"lines":["<b>•</b> Keynote live","<b>•</b> Local match 1–0"],"actions":["Watch","Dismiss"]}}
        ]"#,
        "people" => r#"[
          {"glyph":"💬","title":"Team","agent":"team.agent","delay":0,
           "stream":["Catching up channels…"],
           "resolve":{"big":"3 need replies","sub":"Alex, Priya & #dev-team","actions":["Draft replies","Open"]}},
          {"glyph":"👤","title":"Priya","agent":"people.agent","delay":250,
           "stream":["Reviewing 1:1 notes…"],
           "resolve":{"big":"1:1 at 3pm","sub":"2 open items","actions":["Prep notes","Reschedule"]}},
          {"glyph":"🤝","title":"Connections","agent":"crm.agent","delay":500,"waitsFor":2,
           "stream":["Finding reconnects…"],
           "resolve":{"lines":["<b>•</b> Reconnect: Dana","<b>•</b> Intro to Sam"],"actions":["Send notes","Snooze"]}}
        ]"#,
        "week" => r#"[
          {"glyph":"💸","title":"Money","agent":"finance.agent","delay":0,
           "stream":["Reconciling accounts…"],
           "resolve":{"big":"−$1,240","sub":"vs last week","actions":["Breakdown","Set limit"]}},
          {"glyph":"💪","title":"Health","agent":"health.agent","delay":250,
           "stream":["Reading sleep data…"],
           "resolve":{"big":"6.1h avg sleep","sub":"Down 12%","actions":["Tips","Plan rest"]}},
          {"glyph":"🧠","title":"Focus","agent":"work.agent","delay":500,"waitsFor":2,
           "stream":["Looking across your week…"],
           "resolve":{"lines":["<b>•</b> 14 commits shipped"],"actions":["Apply","Ignore"]}}
        ]"#,
        "lisbon" => r#"[
          {"glyph":"✈️","title":"Flights","agent":"travel.agent","delay":0,
           "stream":["Scanning 40+ carriers for Lisbon…","Comparing price vs. travel time…"],
           "resolve":{"big":"$284 · TAP Air","sub":"Fri 6:40pm → Sun 9:15pm · 1 stop","actions":["Hold seat","Compare"]}},
          {"glyph":"🏠","title":"Stay","agent":"stay.agent","delay":250,
           "stream":["Matching neighborhoods to your taste…","Filtering for walkable + great views…"],
           "resolve":{"big":"Alfama loft","sub":"$96/night · 9.4 · river view","actions":["Reserve","See 6 more"]}},
          {"glyph":"🗺️","title":"Itinerary","agent":"planner.agent","delay":500,"waitsFor":2,
           "stream":["Waiting for flights & stay…"],
           "resolve":{"lines":["<b>Fri</b> — arrive, sunset","<b>Sat</b> — Sintra","<b>Sun</b> — fly home"],"actions":["Save plan","Tweak"]}}
        ]"#,
        "proactive" => r#"[
          {"glyph":"🔎","title":"Research","agent":"research.agent","delay":0,
           "stream":["Reading sources on ABC Corp…","Cross-checking claims…"],
           "resolve":{"big":"Brief ready","sub":"ABC Corp · funding, team, risks","actions":["Open","Save"]}},
          {"glyph":"🌐","title":"Scout","agent":"scout.agent","delay":250,
           "stream":["Watching prices for you…"],
           "resolve":{"big":"Price dropped","sub":"That listing fell 12% overnight","actions":["Buy now","Keep watching"]}},
          {"glyph":"🎨","title":"Maker","agent":"maker.agent","delay":500,"waitsFor":2,"attention":true,
           "stream":["Making something you'd like…"],
           "resolve":{"lines":["<b>•</b> Drafted a weekend playlist","<b>•</b> Sketched 3 logo ideas","<b>•</b> Wrote a trip shortlist"],"actions":["Show me","Discard"]}}
        ]"#,
        _ => r#"[
          {"glyph":"📊","title":"Auto-Excel","agent":"auto-excel","surface":"excel","delay":0,
           "stream":["Working…"],
           "resolve":{"big":"Ready","sub":"Demo card","actions":["Open"]}}
        ]"#,
    }
}

pub fn stream_intent(text: &str) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    stream_intent_with_scenario(
        pick_scenario(text),
        text,
        None,
        None,
        None,
        None,
    )
}

pub fn stream_intent_with_scenario(
    key: &str,
    text: &str,
    suzy_runner: Option<std::sync::Arc<adk_runner::Runner>>,
    sessions: Option<SessionStore>,
    session_id: Option<String>,
    user_id: Option<String>,
) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    let (tx, rx) = mpsc::channel(64);
    let text = text.to_string();
    let key = key.to_string();

    tokio::spawn(async move {
        if let (Some(store), Some(sid)) = (&sessions, &session_id) {
            store.set_scenario(sid, &key, Some(&text)).await;
        }
        let cards: Vec<serde_json::Value> =
            serde_json::from_str(scenario_cards(&key)).unwrap_or_default();

        let _ = tx
            .send(Ok(to_event(&FieldEvent::Scenario {
                key: key.clone(),
                text: text.clone(),
                total_cards: cards.len(),
            })))
            .await;

        let mut resolved = 0usize;

        for (index, card) in cards.iter().enumerate() {
            let delay_ms = card.get("delay").and_then(|v| v.as_u64()).unwrap_or(0);
            tokio::time::sleep(Duration::from_millis(500 + delay_ms)).await;

            if let (Some(store), Some(sid)) = (&sessions, &session_id) {
                crate::orchestrator::persist::card_spawn(store, sid, index, card.clone()).await;
            }
            let _ = tx
                .send(Ok(to_event(&FieldEvent::CardSpawn {
                    index,
                    card: card.clone(),
                    domain: crate::domain::Domain::for_card(&key, card),
                })))
                .await;

            if let Some(stream) = card.get("stream").and_then(|v| v.as_array()) {
                for line in stream {
                    let line = line.as_str().unwrap_or("");
                    let _ = tx
                        .send(Ok(to_event(&FieldEvent::CardStatus {
                            index,
                            status: "working".into(),
                            line: Some(line.into()),
                        })))
                        .await;
                    tokio::time::sleep(Duration::from_millis(720)).await;
                }
            }

            let waits_for = card
                .get("waitsFor")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            if waits_for > 0 && resolved >= waits_for {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::CardStatus {
                        index,
                        status: "composing".into(),
                        line: None,
                    })))
                    .await;
                tokio::time::sleep(Duration::from_millis(600)).await;
            }

            if let Some(resolve) = card.get("resolve") {
                if let (Some(store), Some(sid)) = (&sessions, &session_id) {
                    crate::orchestrator::persist::card_resolve(store, sid, index, card.clone(), resolve.clone(), false).await;
                }
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::CardResolve {
                        index,
                        resolve: resolve.clone(),
                    })))
                    .await;
            }
            resolved += 1;
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
        if let (Some(store), Some(sid), Some(uid)) =
            (&sessions, &session_id, &user_id)
        {
            coordinator::emit_suzy_and_suggest(
                &tx,
                suzy_runner.as_ref(),
                store,
                sid,
                uid,
                &key,
            )
            .await;
        } else {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::SuzySummary {
                    key: key.clone(),
                    html: suzy_summary(&key).to_string(),
                })))
                .await;
            if let Some(action) = tour::action_prompt(&key) {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Suggest {
                        text: action.into(),
                        kind: "action".into(),
                    })))
                    .await;
            }
        }
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    ReceiverStream::new(rx)
}

fn deck_conduct_steps() -> Vec<ConductStep> {
    vec![
        ConductStep {
            op: "fuse".into(),
            source: Some("Auto-Excel".into()),
            target: Some("Auto-Slides".into()),
            delay_ms: None,
        },
        ConductStep {
            op: "wait".into(),
            source: None,
            target: None,
            delay_ms: Some(500),
        },
        ConductStep {
            op: "fuse".into(),
            source: Some("Auto-Docs".into()),
            target: Some("Auto-Slides".into()),
            delay_ms: None,
        },
        ConductStep {
            op: "wait".into(),
            source: None,
            target: None,
            delay_ms: Some(400),
        },
    ]
}

pub fn stream_action(
    action: &str,
    scenario: Option<&str>,
    _text: &str,
) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    let (tx, rx) = mpsc::channel(32);
    let action = action.to_string();
    let scenario = scenario.map(str::to_string);

    tokio::spawn(async move {
        let key = scenario.as_deref().unwrap_or("morning");

        if action == "combine" && key == "deck" {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::Conduct {
                    steps: deck_conduct_steps(),
                })))
                .await;
            tokio::time::sleep(Duration::from_millis(1200)).await;
            let _ = tx
                .send(Ok(to_event(&FieldEvent::DeckFinish {
                    big: "Deck ready".into(),
                    sub: "10 slides · numbers + story combined".into(),
                    artifact_url: None,
                    slide_count: Some(10),
                })))
                .await;
        } else if let Some((source, target)) = scenarios::mock_fuse_pair(key) {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::Conduct {
                    steps: vec![ConductStep {
                        op: "fuse".into(),
                        source: Some(source.into()),
                        target: Some(target.into()),
                        delay_ms: None,
                    }],
                })))
                .await;
            tokio::time::sleep(Duration::from_millis(900)).await;
        }

        coordinator::emit_tour_advance(&tx, key).await;
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    ReceiverStream::new(rx)
}

/// Returns the first `limit` mock intent event type names (fast, no full simulation wait).
pub async fn preview_intent_event_types(text: &str, limit: usize) -> Vec<String> {
    let key = pick_scenario(text);
    let cards: Vec<serde_json::Value> =
        serde_json::from_str(scenario_cards(key)).unwrap_or_default();

    let mut types = vec!["scenario".to_string()];
    for _ in cards.iter().take(limit.saturating_sub(1)) {
        types.push("card_spawn".to_string());
    }
    types.truncate(limit);
    types
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn deck_stream_emits_first_event() {
        let mut stream = stream_intent("Build me a pitch deck");
        assert!(
            stream.next().await.is_some(),
            "mock stream should emit at least one SSE event"
        );
    }
}