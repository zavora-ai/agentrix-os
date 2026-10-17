use std::sync::Arc;

use adk_runner::Runner;
use tokio_stream::wrappers::ReceiverStream;

use crate::orchestrator::fallback;
use crate::orchestrator::workflow::{stream_workflow, AgentSlot, WorkflowStreamConfig};
use crate::state::SessionStore;

fn people_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"💬","title":"Team","agent":"team.agent",
            "stream":["Catching up your channels…","Summarizing threads…"]
        }),
        serde_json::json!({
            "glyph":"👤","title":"Priya","agent":"people.agent",
            "stream":["Reviewing your 1:1 notes…"]
        }),
        serde_json::json!({
            "glyph":"🤝","title":"Connections","agent":"crm.agent","waitsFor":2,"attention":true,
            "stream":["Finding who to reconnect with…"]
        }),
    ]
}

fn status_line(tool: &str) -> &'static str {
    match tool {
        "list_channels" | "list_dms" => "Catching up your channels…",
        "get_channel_history" | "search_messages" => "Summarizing threads…",
        "list_events" | "search_events" | "get_today" => "Reviewing your 1:1 notes…",
        "search_contacts" | "list_contacts" | "list_activities" => "Finding who to reconnect with…",
        _ => "Working…",
    }
}

/// Stub agents (no Slack / calendar / CRM) answer with a `[STUB — …]` label; their cards stay
/// labeled stubs. Live agents' cards show the agent's own words, or "Unavailable" when it failed.
fn is_stub(agent_text: &str) -> bool {
    agent_text.contains("STUB")
}

fn team_resolve(agent_text: &str) -> serde_json::Value {
    if is_stub(agent_text) {
        return serde_json::json!({
            "big": "3 need replies (stub)",
            "sub": "STUB — connect mcp-slack · sample names, not your team",
            "actions": ["Draft replies", "Open"]
        });
    }
    fallback::lines_or_unavailable(agent_text, "Team channels returned nothing this time", &["Draft replies", "Open"])
}

fn priya_resolve(agent_text: &str) -> serde_json::Value {
    if is_stub(agent_text) {
        return serde_json::json!({
            "big": "1:1 at 3pm (stub)",
            "sub": "STUB — connect calendar · sample meeting",
            "actions": ["Prep notes", "Reschedule"]
        });
    }
    fallback::lines_or_unavailable(agent_text, "1:1 notes returned nothing this time", &["Prep notes", "Reschedule"])
}

fn connections_resolve(agent_text: &str) -> serde_json::Value {
    fallback::lines_or_unavailable(agent_text, "Connections returned nothing this time", &["Send notes", "Snooze"])
}

fn resolve(author: &str, _tool_text: &str, agent_text: &str) -> serde_json::Value {
    match author {
        "team_agent" => team_resolve(agent_text),
        "priya_agent" => priya_resolve(agent_text),
        "connections_agent" => connections_resolve(agent_text),
        _ => serde_json::json!({ "big": "Ready", "sub": "Done", "actions": ["Open"] }),
    }
}

pub fn stream_people(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    has_slack: bool,
    has_crm: bool,
    has_calendar: bool,
    sessions: Option<SessionStore>,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, std::convert::Infallible>> {
    stream_workflow(
        runner,
        user_id,
        session_id,
        intent,
        WorkflowStreamConfig {
            scenario_key: "people",
            cards: people_cards(),
            slots: vec![
                AgentSlot {
                    author: "team_agent",
                    index: 0,
                },
                AgentSlot {
                    author: "priya_agent",
                    index: 1,
                },
                AgentSlot {
                    author: "connections_agent",
                    index: 2,
                },
            ],
            status_line,
            resolve,
            integrations_note: format!(
                "slack: {}\ncrm: {}\ncalendar: {}",
                if has_slack { "live" } else { "stub" },
                if has_crm { "live" } else { "stub" },
                if has_calendar { "live" } else { "stub" },
            ),
        },
        sessions,
        suzy_runner,
    )
}