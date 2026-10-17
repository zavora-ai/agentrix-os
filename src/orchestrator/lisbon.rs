use std::sync::Arc;

use adk_runner::Runner;
use tokio_stream::wrappers::ReceiverStream;

use crate::orchestrator::fallback;
use crate::orchestrator::workflow::{stream_workflow, AgentSlot, WorkflowStreamConfig};
use crate::state::SessionStore;

fn lisbon_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"✈️","title":"Flights","agent":"travel.agent",
            "stream":["Scanning carriers for Lisbon…","Comparing price vs. travel time…"]
        }),
        serde_json::json!({
            "glyph":"🏠","title":"Stay","agent":"stay.agent",
            "stream":["Matching neighborhoods…","Filtering for walkable + views…"]
        }),
        serde_json::json!({
            "glyph":"🗺️","title":"Itinerary","agent":"planner.agent","waitsFor":2,
            "stream":["Waiting for flights & stay…","Building day-by-day plan…"]
        }),
    ]
}

fn status_line(tool: &str) -> &'static str {
    match tool {
        "geocode" | "search_poi" | "get_route" => "Building day-by-day plan…",
        "get_forecast" | "geocode_location" => "Checking weather…",
        "geocode_search" | "search_properties_nearby" => "Matching neighborhoods…",
        _ => "Working…",
    }
}

/// Flights is always the BK-009 labeled stub: no flight search exists yet.
fn flights_resolve() -> serde_json::Value {
    serde_json::json!({
        "big": "$284 · TAP Air (stub)",
        "sub": "Not a live quote — BK-009 travel gap",
        "actions": ["Hold seat", "Compare"]
    })
}

fn stay_resolve(agent_text: &str) -> serde_json::Value {
    if agent_text.contains("STUB") {
        return serde_json::json!({
            "big": "Alfama loft (stub)",
            "sub": "Scout only — connect mcp-real-estate",
            "actions": ["Reserve", "See 6 more"]
        });
    }
    fallback::lines_or_unavailable(agent_text, "Stay search returned nothing this time", &["Reserve", "See more"])
}

fn planner_resolve(agent_text: &str) -> serde_json::Value {
    fallback::lines_or_unavailable(agent_text, "Itinerary returned nothing this time", &["Save plan", "Tweak"])
}

fn resolve(author: &str, _tool_text: &str, agent_text: &str) -> serde_json::Value {
    match author {
        "flights_agent" => flights_resolve(),
        "stay_agent" => stay_resolve(agent_text),
        "planner_agent" => planner_resolve(agent_text),
        _ => serde_json::json!({ "big": "Ready", "sub": "Done", "actions": ["Open"] }),
    }
}

pub fn stream_lisbon(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    has_maps: bool,
    has_real_estate: bool,
    sessions: Option<SessionStore>,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, std::convert::Infallible>> {
    stream_workflow(
        runner,
        user_id,
        session_id,
        intent,
        WorkflowStreamConfig {
            scenario_key: "lisbon",
            cards: lisbon_cards(),
            slots: vec![
                AgentSlot {
                    author: "flights_agent",
                    index: 0,
                },
                AgentSlot {
                    author: "stay_agent",
                    index: 1,
                },
                AgentSlot {
                    author: "planner_agent",
                    index: 2,
                },
            ],
            status_line,
            resolve,
            integrations_note: format!(
                "flights: stub (BK-009)\nstay: {}\nmaps: {}\nweather: live",
                if has_real_estate { "live scout" } else { "stub" },
                if has_maps { "live" } else { "weather-only itinerary" },
            ),
        },
        sessions,
        suzy_runner,
    )
}