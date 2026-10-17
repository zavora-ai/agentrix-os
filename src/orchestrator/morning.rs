use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use adk_core::{Content, SessionId, UserId};
use adk_runner::Runner;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::events::sse::{to_event, FieldEvent};
use crate::orchestrator::{coordinator, fallback, persist};
use crate::state::SessionStore;

fn morning_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"📅","title":"Today","agent":"calendar.agent",
            "stream":["Reading your calendar…"]
        }),
        serde_json::json!({
            "glyph":"✉️","title":"Needs you","agent":"inbox.agent","attention":true,
            "stream":["Triaging new emails…","Surfacing only what matters…"]
        }),
        serde_json::json!({
            "glyph":"📰","title":"Brief","agent":"news.agent","waitsFor":2,
            "stream":["Composing your brief…"]
        }),
    ]
}

fn card_index(author: &str) -> Option<usize> {
    match author {
        "calendar_agent" => Some(0),
        "inbox_agent" => Some(1),
        "brief_agent" => Some(2),
        _ => None,
    }
}

fn status_line(tool: &str) -> &'static str {
    match tool {
        "get_today" | "list_events" | "list_calendars" => "Reading your calendar…",
        "find_free_time" | "search_events" => "Finding free time…",
        "list_inbox" => "Triaging new emails…",
        "search_emails" | "get_email" => "Surfacing only what matters…",
        "gnews_top_headlines" | "search_news" | "get_country_news" => "Pulling headlines…",
        "get_forecast" | "geocode_location" => "Checking weather…",
        _ => "Working…",
    }
}

fn text_from_response(response: &serde_json::Value) -> String {
    response
        .get("output")
        .and_then(|o| o.as_str())
        .or_else(|| response.get("text").and_then(|t| t.as_str()))
        .unwrap_or("")
        .to_string()
}

/// Meeting count from one `get_today` tool output (a JSON array of events).
fn parse_meeting_count(text: &str) -> Option<usize> {
    serde_json::from_str::<serde_json::Value>(text.trim())
        .ok()?
        .as_array()
        .map(Vec::len)
}

fn calendar_resolve(meetings: Option<usize>, agent_text: &str, connected: bool) -> serde_json::Value {
    if !connected {
        return fallback::not_connected("Connect a calendar (mcp-calendar auth google) to see today's meetings");
    }
    let Some(count) = meetings else {
        return fallback::lines_or_unavailable(agent_text, "Calendar returned nothing this time", &["Open", "Reschedule"]);
    };
    serde_json::json!({
        "big": match count {
            0 => "No meetings".to_string(),
            1 => "1 meeting".to_string(),
            n => format!("{n} meetings"),
        },
        "sub": fallback::first_line(agent_text, 72).unwrap_or_else(|| "From your calendar today".into()),
        "actions": ["Open", "Reschedule"]
    })
}

/// The inbox agent judges which threads need a reply; the card shows its own words, not a count.
fn inbox_resolve(agent_text: &str, connected: bool) -> serde_json::Value {
    if !connected {
        return fallback::not_connected("Connect email (mcp-email auth gmail, or SMTP/IMAP) to triage your inbox");
    }
    fallback::lines_or_unavailable(agent_text, "Inbox returned nothing this time", &["Draft replies", "Snooze"])
}

fn brief_resolve(agent_text: &str) -> serde_json::Value {
    fallback::lines_or_unavailable(agent_text, "Brief returned nothing this time", &["Read aloud", "Dismiss"])
}

struct MorningFacts {
    meetings: Option<usize>,
    has_calendar: bool,
    has_inbox: bool,
}

fn resolve_for_agent(author: &str, agent_text: &str, facts: &MorningFacts) -> serde_json::Value {
    match author {
        "calendar_agent" => calendar_resolve(facts.meetings, agent_text, facts.has_calendar),
        "inbox_agent" => inbox_resolve(agent_text, facts.has_inbox),
        "brief_agent" => brief_resolve(agent_text),
        _ => serde_json::json!({ "big": "Ready", "sub": "Done", "actions": ["Open"] }),
    }
}

pub fn stream_morning(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    sessions: Option<SessionStore>,
    has_calendar: bool,
    has_inbox: bool,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let cards = morning_cards();
        if let Some(ref store) = sessions {
            store
                .set_scenario(&session_id, "morning", Some(&intent))
                .await;
        }
        let _ = tx
            .send(Ok(to_event(&FieldEvent::Scenario {
                key: "morning".into(),
                text: intent.clone(),
                total_cards: cards.len(),
            })))
            .await;

        for (index, card) in cards.iter().enumerate() {
            if let Some(ref store) = sessions {
                persist::card_spawn(store, &session_id, index, card.clone()).await;
            }
            let _ = tx
                .send(Ok(to_event(&FieldEvent::CardSpawn {
                    index,
                    card: card.clone(),
                    domain: crate::domain::Domain::for_card("morning", card),
                })))
                .await;
        }

        let integrations = format!(
            "[Integrations]\ncalendar: {}\ninbox: {}\nnews: yes\nweather: yes",
            if has_calendar { "live" } else { "unavailable — use general context" },
            if has_inbox { "live" } else { "unavailable — use general context" },
        );
        // Profile facts from memory replace hardcoded defaults (S3-T6).
        let memory = crate::memory::service_handle();
        let profile = format!(
            "[Profile]\nhome_location: {}\ntimezone: {}",
            memory.profile(&user_id, "home_location").await.unwrap_or_else(|| "unknown".into()),
            memory.profile(&user_id, "timezone").await.unwrap_or_else(|| "unknown".into()),
        );

        let prompt = format!(
            "{intent}\n\n{integrations}\n{profile}\nPrepare the morning briefing cards.",
        );

        let content = Content::new("user").with_text(&prompt);
        let uid = match UserId::try_from(user_id.as_str()) {
            Ok(u) => u,
            Err(e) => {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                return;
            }
        };
        let sid = match SessionId::try_from(session_id.as_str()) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                return;
            }
        };

        crate::agents::ensure_runner_session(&runner, &user_id, &session_id).await;
        let stream = match runner.run(uid, sid, content).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                return;
            }
        };

        futures::pin_mut!(stream);
        let mut resolved = vec![false; 3];
        let mut agent_buf: [String; 3] = Default::default();
        let mut facts = MorningFacts { meetings: None, has_calendar, has_inbox };

        while let Some(result) = stream.next().await {
            let event = match result {
                Ok(ev) => ev,
                Err(e) => {
                    let _ = tx
                        .send(Ok(to_event(&FieldEvent::Error {
                            message: e.to_string(),
                        })))
                        .await;
                    break;
                }
            };

            let author = event.author.clone();
            let Some(index) = card_index(&author) else {
                continue;
            };

            if let Some(content) = event.content() {
                for part in &content.parts {
                    match part {
                        adk_core::Part::FunctionCall { name, .. } => {
                            let _ = tx
                                .send(Ok(to_event(&FieldEvent::CardStatus {
                                    index,
                                    status: "working".into(),
                                    line: Some(status_line(name).into()),
                                })))
                                .await;
                        }
                        adk_core::Part::FunctionResponse { function_response, .. }
                            if function_response.name == "get_today" =>
                        {
                            let out = text_from_response(&function_response.response);
                            facts.meetings = parse_meeting_count(&out).or(facts.meetings);
                        }
                        adk_core::Part::Text { text } => {
                            agent_buf[index].push_str(text);
                            agent_buf[index].push('\n');
                        }
                        _ => {}
                    }
                }
            }

            if event.is_final_response() && !resolved[index] {
                let resolve = resolve_for_agent(&author, &agent_buf[index], &facts);
                if let Some(ref store) = sessions {
                    let card = cards.get(index).cloned().unwrap_or_default();
                    persist::card_resolve(
                        store,
                        &session_id,
                        index,
                        card,
                        resolve.clone(),
                        false,
                    )
                    .await;
                }
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::CardResolve { index, resolve })))
                    .await;
                resolved[index] = true;
            }
        }

        for (index, author) in [
            (0, "calendar_agent"),
            (1, "inbox_agent"),
            (2, "brief_agent"),
        ] {
            if resolved[index] {
                continue;
            }
            let resolve = resolve_for_agent(author, &agent_buf[index], &facts);
            if let Some(ref store) = sessions {
                let card = cards.get(index).cloned().unwrap_or_default();
                persist::card_resolve(
                    store,
                    &session_id,
                    index,
                    card,
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
        if let Some(ref store) = sessions {
            coordinator::emit_suzy_and_suggest(
                &tx,
                suzy_runner.as_ref(),
                store,
                &session_id,
                &user_id,
                "morning",
            )
            .await;
        }
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    ReceiverStream::new(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morning_cards_never_invent_counts() {
        let off = MorningFacts { meetings: None, has_calendar: false, has_inbox: false };
        assert_eq!(resolve_for_agent("calendar_agent", "", &off)["big"], "Not connected");
        assert_eq!(resolve_for_agent("inbox_agent", "", &off)["big"], "Not connected");

        let on = MorningFacts { meetings: None, has_calendar: true, has_inbox: true };
        assert_eq!(resolve_for_agent("calendar_agent", "", &on)["big"], "Unavailable");
        assert_eq!(resolve_for_agent("inbox_agent", "", &on)["big"], "Unavailable");
        assert_eq!(resolve_for_agent("brief_agent", "", &on)["big"], "Unavailable");

        let two = MorningFacts { meetings: parse_meeting_count("[\n  {\"summary\": \"Standup\"},\n  {\"summary\": \"1:1\"}\n]"), ..on };
        let r = resolve_for_agent("calendar_agent", "First: Standup 9:30", &two);
        assert_eq!(r["big"], "2 meetings");
        assert_eq!(r["sub"], "First: Standup 9:30");
        assert_eq!(parse_meeting_count("Error: token expired"), None);
    }
}
