//! Live scenario — Headlines, Markets, Now via mcp-news (+ optional market-data).

use std::sync::Arc;

use adk_agent::{ParallelAgent, SequentialAgent, LlmAgentBuilder};

use super::gemini;
use crate::tools::merge::MergedToolset;

pub struct LiveMcpPool {
    pub news: Arc<dyn adk_core::Toolset>,
    pub market_data: Option<Arc<dyn adk_core::Toolset>>,
}

async fn headlines_agent(
    api_key: &str,
    model_name: &str,
    news: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("headlines_agent", news);
    let agent = LlmAgentBuilder::new("headlines_agent")
        .description("Live headlines card")
        .model(model)
        .instruction(
            r#"You are the Headlines card for the live scenario.
- gnews_top_headlines (country us) for top stories.
- Summarize 2–3 headlines in one line each.
- Output: story count + short subline with key themes."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.25),
            max_output_tokens: Some(4096),
            ..Default::default()
        })
        .build()?;
    Ok(Arc::new(agent))
}

async fn markets_agent(
    api_key: &str,
    model_name: &str,
    toolsets: Vec<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let merged = MergedToolset::new(toolsets);
    let tools = gemini::filtered_for_agent("markets_agent", merged);
    let agent = LlmAgentBuilder::new("markets_agent")
        .description("Live markets card")
        .model(model)
        .instruction(
            r#"You are the Markets card.
- yfinance_chart for SPY or user's watchlist tickers if mentioned.
- Summarize pre-market or daily move and one standout ticker.
- Output concise numbers for big/sub resolve."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.2),
            max_output_tokens: Some(4096),
            ..Default::default()
        })
        .build()?;
    Ok(Arc::new(agent))
}

async fn now_agent(
    api_key: &str,
    model_name: &str,
    news: Arc<dyn adk_core::Toolset>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let model = crate::llm::build(api_key, model_name)?;
    let tools = gemini::filtered_for_agent("now_agent", news);
    let agent = LlmAgentBuilder::new("now_agent")
        .description("Live Now card — trending + breaking")
        .model(model)
        .instruction(
            r#"You are the Now card — what's happening right now.
- get_trending_topics and gnews_top_headlines for live-feel items.
- Output 3 short lines (breaking, trending, local/world).
- Minimize narration."#,
        )
        .toolset(tools)
        .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
        .generate_content_config(adk_core::GenerateContentConfig {
            temperature: Some(0.3),
            max_output_tokens: Some(4096),
            ..Default::default()
        })
        .build()?;
    Ok(Arc::new(agent))
}

pub async fn build_workflow(
    api_key: &str,
    model_name: &str,
    pool: &LiveMcpPool,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let headlines = headlines_agent(api_key, model_name, pool.news.clone()).await?;

    let mut market_toolsets = vec![pool.news.clone()];
    if let Some(md) = pool.market_data.clone() {
        market_toolsets.push(md);
    }
    let markets = markets_agent(api_key, model_name, market_toolsets).await?;
    let now = now_agent(api_key, model_name, pool.news.clone()).await?;

    let parallel = ParallelAgent::new("live_parallel", vec![headlines, markets])
        .with_description("Headlines + markets in parallel")
        .with_shared_state();

    let pipeline = SequentialAgent::new(
        "live_workflow",
        vec![
            Arc::new(parallel) as Arc<dyn adk_core::Agent>,
            now,
        ],
    )
    .with_description("Live: parallel headlines+markets, then Now");

    Ok(Arc::new(pipeline))
}