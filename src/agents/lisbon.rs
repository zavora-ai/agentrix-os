//! Lisbon scenario — Flights/Stay stubs, Itinerary via maps + weather.

use std::sync::Arc;

use adk_agent::{LlmAgentBuilder, ParallelAgent, SequentialAgent};

use super::gemini;
use super::stub;
use crate::tools::merge::MergedToolset;

pub struct LisbonMcpPool {
    pub maps: Option<Arc<dyn adk_core::Toolset>>,
    pub weather: Arc<dyn adk_core::Toolset>,
    pub real_estate: Option<Arc<dyn adk_core::Toolset>>,
}

async fn flights_agent() -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    stub::labeled_stub(
        "flights_agent",
        "STUB — mcp-travel pending (BK-009)",
        "$284 · TAP Air · Fri 6:40pm → Sun 9:15pm · 1 stop. Not a live quote — connect travel MCP or computer-use.",
    )
}

async fn stay_agent(
    api_key: &str,
    model_name: &str,
    real_estate: Option<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(ts) = real_estate {
        let model = crate::llm::build(api_key, model_name)?;
        let tools = gemini::filtered_for_agent("stay_agent", ts);
        let agent = LlmAgentBuilder::new("stay_agent")
            .description("Stay scout — real estate MCP")
            .model(model)
            .instruction(
                r#"You are the Stay card for a Lisbon trip.
- geocode_search for Alfama or user neighborhood; search_properties_nearby for options.
- Summarize best stay with nightly rate and rating.
- Output big/sub for the card."#,
            )
            .toolset(tools)
            .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
            .generate_content_config(adk_core::GenerateContentConfig {
                temperature: Some(0.3),
                max_output_tokens: Some(4096),
                ..Default::default()
            })
            .build()?;
        return Ok(Arc::new(agent));
    }
    stub::labeled_stub(
        "stay_agent",
        "STUB — connect mcp-real-estate",
        "Alfama loft · $96/night · 9.4 rating. Scout only — not a booking. Connect real-estate MCP for live listings.",
    )
}

async fn planner_agent(
    api_key: &str,
    model_name: &str,
    maps: Option<Arc<dyn adk_core::Toolset>>,
    weather: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let mut parts = vec![weather];
    if let Some(m) = maps.clone() {
        parts.insert(0, m);
    }
    let model = crate::llm::build(api_key, model_name)?;
    let merged = MergedToolset::new(parts);
    let tools = gemini::filtered_for_agent("planner_agent", merged);
    let agent = LlmAgentBuilder::new("planner_agent")
        .description("Itinerary planner — maps + weather")
        .model(model)
        .instruction(
            r#"You are the Itinerary card for Lisbon.
- geocode Lisbon; search_poi for Miradouro, Sintra, Time Out Market.
- get_forecast for trip weekend.
- Read sibling flight/stay stubs from shared state if present.
- Output 3 day lines: Fri arrive, Sat Sintra, Sun fly home."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.35),
            max_output_tokens: Some(4096),
            ..Default::default()
        })
        .build()?;
    Ok(Arc::new(agent))
}

pub async fn build_workflow(
    api_key: &str,
    model_name: &str,
    pool: &LisbonMcpPool,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let flights = flights_agent().await?;
    let stay = stay_agent(api_key, model_name, pool.real_estate.clone()).await?;
    let planner = planner_agent(
        api_key,
        model_name,
        pool.maps.clone(),
        pool.weather.clone(),
    )
    .await?;

    let parallel = ParallelAgent::new("lisbon_parallel", vec![flights, stay])
        .with_description("Flights stub + stay scout in parallel")
        .with_shared_state();

    let pipeline = SequentialAgent::new(
        "lisbon_workflow",
        vec![
            Arc::new(parallel) as Arc<dyn adk_core::Agent>,
            planner,
        ],
    )
    .with_description("Lisbon: parallel flights+stay, then itinerary");

    Ok(Arc::new(pipeline))
}