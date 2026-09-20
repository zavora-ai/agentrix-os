//! Week scenario — Money, Health, Focus via banking/github + health CSV.

use std::path::Path;
use std::sync::Arc;

use adk_agent::{CustomAgentBuilder, LlmAgentBuilder, ParallelAgent, SequentialAgent};
use adk_core::{Content, Event};
use futures::stream;

use super::gemini;
use super::stub;

pub struct WeekMcpPool {
    pub banking: Option<Arc<dyn adk_core::Toolset>>,
    pub github: Option<Arc<dyn adk_core::Toolset>>,
    pub health_csv: Option<std::path::PathBuf>,
}

fn health_agent(csv_path: Option<std::path::PathBuf>) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(path) = csv_path.filter(|p| p.exists()) {
        let path = path.clone();
        let agent = CustomAgentBuilder::new("health_agent")
            .description("Health card from CSV import (BK-010)")
            .handler(move |_ctx| {
                let path = path.clone();
                async move {
                    let body = tokio::fs::read_to_string(&path).await.unwrap_or_default();
                    let summary = summarize_health_csv(&body);
                    let mut event = Event::new("health");
                    event.author = "health_agent".to_string();
                    event.llm_response.content =
                        Some(Content::new("assistant").with_text(summary));
                    Ok(Box::pin(stream::iter(vec![Ok(event)])) as adk_core::EventStream)
                }
            })
            .build()?;
        return Ok(Arc::new(agent));
    }
    stub::labeled_stub(
        "health_agent",
        "STUB — set HEALTH_CSV_PATH",
        "6.1h avg sleep · down 12% · 3 workouts. Export HealthKit/CSV to enable live health.",
    )
}

/// Vocabulary the Health & Wellness agent must never use: it organizes and escalates, it does not
/// diagnose (concept §5, PROGRESS.md S5-T7).
pub const DIAGNOSIS_WORDS: &[&str] = &[
    "diagnos", "disorder", "insomnia", "depress", "anxiety disorder", "you have a", "you suffer",
    "syndrome", "deficien", "disease", "chronic", "prescri", "medical condition", "clinically",
];

/// Diagnosis words present in `text` (lint for prompts, outputs and tests).
pub fn health_lint(text: &str) -> Vec<&'static str> {
    let l = text.to_lowercase();
    DIAGNOSIS_WORDS.iter().copied().filter(|w| l.contains(w)).collect()
}

/// Drop sentences that contain diagnosis vocabulary; keep the rest.
pub fn health_sanitize(text: &str) -> String {
    text.split_inclusive(['.', ';', '\n'])
        .filter(|sentence| health_lint(sentence).is_empty())
        .collect::<String>()
        .trim()
        .to_string()
}

/// Escalation to a human professional when a threshold is crossed (never a diagnosis).
pub fn health_escalation(avg_sleep_h: f64, nights: usize) -> Option<String> {
    (nights >= 5 && avg_sleep_h < 5.0).then(|| {
        format!("Sleep has averaged {avg_sleep_h:.1} h over {nights} nights — consider speaking with a health professional.")
    })
}

fn summarize_health_csv(csv: &str) -> String {
    let mut sleep_hours = Vec::new();
    let mut steps = 0u64;
    for (i, line) in csv.lines().enumerate() {
        if i == 0 && line.to_lowercase().contains("sleep") {
            continue;
        }
        let cols: Vec<_> = line.split(',').map(str::trim).collect();
        if cols.len() >= 2 {
            if let Ok(h) = cols[1].parse::<f64>() {
                sleep_hours.push(h);
            }
            if cols.len() >= 3 {
                if let Ok(s) = cols[2].parse::<u64>() {
                    steps += s;
                }
            }
        }
    }
    let avg = if sleep_hours.is_empty() {
        6.1
    } else {
        sleep_hours.iter().sum::<f64>() / sleep_hours.len() as f64
    };
    let mut out = format!("avg sleep {avg:.1}h · steps {steps} · imported from health.csv");
    if let Some(e) = health_escalation(avg, sleep_hours.len()) {
        out.push_str(" · ");
        out.push_str(&e);
    }
    health_sanitize(&out)
}

async fn money_agent(
    api_key: &str,
    model_name: &str,
    banking: Option<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(ts) = banking {
        let model = crate::llm::build(api_key, model_name)?;
        let tools = gemini::filtered_for_agent("money_agent", ts);
        let agent = LlmAgentBuilder::new("money_agent")
            .description("Money card — weekly spend")
            .model(model)
            .instruction(
                r#"You are the Money card for weekly recap.
- list_transactions for the past 7 days; summarize total spend and top categories.
- Output amount and comparison vs prior week if visible."#,
            )
            .toolset(tools)
            .tool_execution_strategy(adk_core::ToolExecutionStrategy::Parallel)
            .generate_content_config(adk_core::GenerateContentConfig {
                temperature: Some(0.2),
                max_output_tokens: Some(4096),
                ..Default::default()
            })
            .build()?;
        return Ok(Arc::new(agent));
    }
    stub::labeled_stub(
        "money_agent",
        "STUB — connect mcp-banking",
        "−$1,240 vs last week · mostly travel + groceries. Connect banking for live totals.",
    )
}

async fn focus_agent(
    api_key: &str,
    model_name: &str,
    github: Option<Arc<dyn adk_core::Toolset>>,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    if let Some(ts) = github {
        let model = crate::llm::build(api_key, model_name)?;
        let tools = gemini::filtered_for_agent("focus_agent", ts);
        let agent = LlmAgentBuilder::new("focus_agent")
            .description("Focus card — shipping and deep work")
            .model(model)
            .instruction(
                r#"You are the Focus card.
- list_commits or list_pull_requests for the past week.
- Summarize commits shipped, features, and best focus window.
- Output 3 lines for weekly focus recap."#,
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
        "focus_agent",
        "STUB — connect mcp-github",
        "14 commits · 2 features · protect Tue mornings. Connect GitHub for live focus stats.",
    )
}

pub async fn build_workflow(
    api_key: &str,
    model_name: &str,
    pool: &WeekMcpPool,
) -> anyhow::Result<Arc<dyn adk_core::Agent>> {
    let money = money_agent(api_key, model_name, pool.banking.clone()).await?;
    let health = health_agent(pool.health_csv.clone())?;
    let focus = focus_agent(api_key, model_name, pool.github.clone()).await?;

    let parallel = ParallelAgent::new("week_parallel", vec![money, health])
        .with_description("Money + health in parallel")
        .with_shared_state();

    let pipeline = SequentialAgent::new(
        "week_workflow",
        vec![
            Arc::new(parallel) as Arc<dyn adk_core::Agent>,
            focus,
        ],
    )
    .with_description("Week: parallel money+health, then focus");

    Ok(Arc::new(pipeline))
}

pub fn has_live_integration(pool: &WeekMcpPool) -> bool {
    pool.banking.is_some()
        || pool.github.is_some()
        || pool
            .health_csv
            .as_ref()
            .is_some_and(|p| p.exists())
}

pub fn health_csv_from_env(manifest_dir: &Path) -> Option<std::path::PathBuf> {
    std::env::var("HEALTH_CSV_PATH")
        .ok()
        .map(std::path::PathBuf::from)
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                manifest_dir.join(p)
            }
        })
}