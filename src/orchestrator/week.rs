use std::sync::Arc;

use adk_runner::Runner;
use tokio_stream::wrappers::ReceiverStream;

use crate::orchestrator::fallback;
use crate::orchestrator::workflow::{stream_workflow, AgentSlot, WorkflowStreamConfig};
use crate::state::SessionStore;

fn week_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"💸","title":"Money","agent":"finance.agent",
            "stream":["Reconciling accounts…"]
        }),
        serde_json::json!({
            "glyph":"💪","title":"Health","agent":"health.agent",
            "stream":["Reading activity & sleep…"]
        }),
        serde_json::json!({
            "glyph":"🧠","title":"Focus","agent":"work.agent","waitsFor":2,
            "stream":["Looking across your week…"]
        }),
    ]
}

fn status_line(_tool: &str) -> &'static str {
    "Working…"
}

fn money_resolve(agent_text: &str) -> serde_json::Value {
    if agent_text.contains("STUB") {
        return serde_json::json!({
            "big": "−$1,240 (stub)",
            "sub": "STUB — connect mcp-banking · sample total",
            "actions": ["Breakdown", "Set limit"]
        });
    }
    match fallback::first_line(agent_text, 72) {
        Some(sub) => serde_json::json!({ "big": "Weekly spend", "sub": sub, "actions": ["Breakdown", "Set limit"] }),
        None => fallback::unavailable("Banking returned nothing this time"),
    }
}

/// Hours after `avg sleep` in the health agent's text (`avg sleep 6.4h · …`).
fn avg_sleep(text: &str) -> Option<String> {
    let after = text.split("avg sleep").nth(1)?.split_whitespace().next()?;
    let hours = after.trim_end_matches('h').parse::<f64>().ok()?;
    Some(format!("{hours:.1}h avg sleep"))
}

fn health_resolve(agent_text: &str) -> serde_json::Value {
    let actions = ["Tips", "Plan rest"];
    if agent_text.contains("STUB") {
        return serde_json::json!({ "big": "6.1h avg sleep (stub)", "sub": "STUB — set HEALTH_CSV_PATH", "actions": actions });
    }
    match avg_sleep(agent_text) {
        Some(big) => serde_json::json!({ "big": big, "sub": "Imported health data", "actions": actions }),
        None => fallback::lines_or_unavailable(agent_text, "Health data returned nothing this time", &actions),
    }
}

fn focus_resolve(agent_text: &str) -> serde_json::Value {
    fallback::lines_or_unavailable(agent_text, "GitHub activity returned nothing this time", &["Apply", "Ignore"])
}

fn resolve(author: &str, _tool_text: &str, agent_text: &str) -> serde_json::Value {
    match author {
        "money_agent" => money_resolve(agent_text),
        "health_agent" => health_resolve(agent_text),
        "focus_agent" => focus_resolve(agent_text),
        _ => serde_json::json!({ "big": "Ready", "sub": "Done", "actions": ["Open"] }),
    }
}

pub fn stream_week(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    has_banking: bool,
    has_github: bool,
    has_health_csv: bool,
    sessions: Option<SessionStore>,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, std::convert::Infallible>> {
    stream_workflow(
        runner,
        user_id,
        session_id,
        intent,
        WorkflowStreamConfig {
            scenario_key: "week",
            cards: week_cards(),
            slots: vec![
                AgentSlot {
                    author: "money_agent",
                    index: 0,
                },
                AgentSlot {
                    author: "health_agent",
                    index: 1,
                },
                AgentSlot {
                    author: "focus_agent",
                    index: 2,
                },
            ],
            status_line,
            resolve,
            integrations_note: format!(
                "banking: {}\ngithub: {}\nhealth_csv: {}",
                if has_banking { "live" } else { "stub" },
                if has_github { "live" } else { "stub" },
                if has_health_csv { "live" } else { "stub" },
            ),
        },
        sessions,
        suzy_runner,
    )
}