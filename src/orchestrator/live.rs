use std::sync::Arc;

use adk_runner::Runner;
use tokio_stream::wrappers::ReceiverStream;

use crate::orchestrator::fallback;
use crate::orchestrator::workflow::{AgentSlot, WorkflowStreamConfig};
use crate::orchestrator::workflow::stream_workflow;
use crate::state::SessionStore;

fn live_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"📰","title":"Headlines","agent":"news.agent",
            "stream":["Scanning live sources…","Filtering to what you follow…"]
        }),
        serde_json::json!({
            "glyph":"📈","title":"Markets","agent":"markets.agent",
            "stream":["Checking your watchlist…"]
        }),
        serde_json::json!({
            "glyph":"🔴","title":"Now","agent":"live.agent","waitsFor":2,
            "stream":["Tuning into live events…"]
        }),
    ]
}

fn status_line(tool: &str) -> &'static str {
    match tool {
        "gnews_top_headlines" | "search_news" | "get_country_news" => "Scanning live sources…",
        "get_trending_topics" => "Filtering to what you follow…",
        "yfinance_chart" => "Checking your watchlist…",
        _ => "Working…",
    }
}

fn headlines_resolve(agent_text: &str) -> serde_json::Value {
    fallback::lines_or_unavailable(agent_text, "Headlines returned nothing this time", &["Read aloud", "Open"])
}

/// First signed percentage the agent wrote (`+0.62%`, `-1.1%`, `−0.4%`), as written.
fn first_percent(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|t| t.trim_matches(|c: char| matches!(c, '(' | ')' | ',' | ';' | ':' | '*')))
        .find(|t| {
            t.ends_with('%')
                && t.starts_with(['+', '-', '−'])
                && t.trim_start_matches(['+', '-', '−']).trim_end_matches('%').parse::<f64>().is_ok()
        })
        .map(str::to_string)
}

/// The big number is a percentage the markets agent actually reported; never guessed from a sign.
fn markets_resolve(agent_text: &str) -> serde_json::Value {
    let actions = ["Details", "Set alert"];
    match (first_percent(agent_text), fallback::first_line(agent_text, 72)) {
        (Some(pct), Some(sub)) => serde_json::json!({ "big": pct, "sub": sub, "actions": actions }),
        _ => fallback::lines_or_unavailable(agent_text, "Markets returned nothing this time", &actions),
    }
}

fn now_resolve(agent_text: &str) -> serde_json::Value {
    fallback::lines_or_unavailable(agent_text, "Live events returned nothing this time", &["Watch", "Dismiss"])
}

fn resolve(author: &str, _tool_text: &str, agent_text: &str) -> serde_json::Value {
    match author {
        "headlines_agent" => headlines_resolve(agent_text),
        "markets_agent" => markets_resolve(agent_text),
        "now_agent" => now_resolve(agent_text),
        _ => serde_json::json!({ "big": "Ready", "sub": "Done", "actions": ["Open"] }),
    }
}

pub fn stream_live(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    has_market_data: bool,
    sessions: Option<SessionStore>,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, std::convert::Infallible>> {
    stream_workflow(
        runner,
        user_id,
        session_id,
        intent,
        WorkflowStreamConfig {
            scenario_key: "live",
            cards: live_cards(),
            slots: vec![
                AgentSlot {
                    author: "headlines_agent",
                    index: 0,
                },
                AgentSlot {
                    author: "markets_agent",
                    index: 1,
                },
                AgentSlot {
                    author: "now_agent",
                    index: 2,
                },
            ],
            status_line,
            resolve,
            integrations_note: format!(
                "news: live\nmarket_data: {}",
                if has_market_data {
                    "live"
                } else {
                    "via mcp-news yfinance"
                }
            ),
        },
        sessions,
        suzy_runner,
    )
}