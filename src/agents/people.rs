//! People scenario — Team, Priya, Connections via slack/crm/calendar.

use std::sync::Arc;

use adk_agent::{LlmAgentBuilder, ParallelAgent, SequentialAgent};

use super::gemini;
use super::stub;
pub struct PeopleMcpPool {
    pub slack: Option<Arc<dyn adk_core::Toolset>>,
    pub crm: Option<Arc<dyn adk_core::Toolset>>,
    pub calendar: Option<Arc<dyn adk_core::Toolset>>,
}

async fn team_agent(
    api_key: &str,
    model_name: &str,
    slack: Option<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(ts) = slack {
        let model = crate::llm::build(api_key, model_name)?;
        let tools = gemini::filtered_for_agent("team_agent", ts);
        let agent = LlmAgentBuilder::new("team_agent")
            .description("Team card — Slack channels and DMs")
            .model(model)
            .instruction(
                r#"You are the Team card.
- list_channels or list_dms, then get_channel_history or search_messages for urgent threads.
- Summarize who needs replies (names + channel).
- Output counts and names for big/sub."#,
            )
            .toolset(tools)
            .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
            .generate_content_config(adk_core::GenerateContentConfig {
                temperature: Some(0.25),
                max_output_tokens: Some(4096),
                ..Default::default()
            })
            .build()?;
        return Ok(Arc::new(agent));
    }
    stub::labeled_stub(
        "team_agent",
        "STUB — connect mcp-slack",
        "3 threads may need replies (Alex, Priya, #dev-team). Connect Slack for live data.",
    )
}

async fn priya_agent(
    api_key: &str,
    model_name: &str,
    calendar: Option<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(ts) = calendar {
        let model = crate::llm::build(api_key, model_name)?;
        let tools = gemini::filtered_for_agent("priya_agent", ts);
        let agent = LlmAgentBuilder::new("priya_agent")
            .description("Priya 1:1 prep card")
            .model(model)
            .instruction(
                r#"You are the Priya card — prep for the next 1:1.
- list_events or search_events for meetings with Priya (or next 1:1).
- Note time and open items from context.
- Output meeting time + prep notes."#,
            )
            .toolset(tools)
            .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
            .generate_content_config(adk_core::GenerateContentConfig {
                temperature: Some(0.25),
                max_output_tokens: Some(4096),
                ..Default::default()
            })
            .build()?;
        return Ok(Arc::new(agent));
    }
    stub::labeled_stub(
        "priya_agent",
        "STUB — connect mcp-calendar",
        "1:1 at 3pm · 2 open items from last week. Connect calendar for live prep.",
    )
}

async fn connections_agent(
    api_key: &str,
    model_name: &str,
    crm: Option<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(ts) = crm {
        let model = crate::llm::build(api_key, model_name)?;
        let tools = gemini::filtered_for_agent("connections_agent", ts);
        let agent = LlmAgentBuilder::new("connections_agent")
            .description("Connections card — CRM follow-ups")
            .model(model)
            .instruction(
                r#"You are the Connections card.
- search_contacts and list_activities for stale relationships and promised intros.
- Output 3 bullet lines: reconnect, intro, birthday/event."#,
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
        "connections_agent",
        "STUB — connect mcp-crm",
        "Reconnect: Dana (3 mo) · Intro promised to Sam · Birthday: Mara Friday. Connect CRM for live contacts.",
    )
}

pub async fn build_workflow(
    api_key: &str,
    model_name: &str,
    pool: &PeopleMcpPool,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let team = team_agent(api_key, model_name, pool.slack.clone()).await?;
    let priya = priya_agent(api_key, model_name, pool.calendar.clone()).await?;
    let connections = connections_agent(api_key, model_name, pool.crm.clone()).await?;

    let parallel = ParallelAgent::new("people_parallel", vec![team, priya])
        .with_description("Team + Priya in parallel")
        .with_shared_state();

    let pipeline = SequentialAgent::new(
        "people_workflow",
        vec![
            Arc::new(parallel) as Arc<dyn adk_core::Agent>,
            connections,
        ],
    )
    .with_description("People: parallel team+priya, then connections");

    Ok(Arc::new(pipeline))
}

pub fn has_live_integration(pool: &PeopleMcpPool) -> bool {
    pool.slack.is_some() || pool.crm.is_some() || pool.calendar.is_some()
}