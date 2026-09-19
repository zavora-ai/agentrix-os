//! Boot and run AmbientAgent cron loops for research, scout, maker.

use std::sync::Arc;

use adk_agent::ambient::{AmbientAgent, CronTrigger, TriggerHandler};
use adk_awp::{AwpEvent, EventSubscriptionService, InMemoryEventSubscriptionService};
use adk_core::{Content, SessionId, UserId};
use adk_runner::Runner;
use adk_session::CreateRequest;
use crate::state::SharedSessionService;
use chrono::Utc;
use futures::{stream, StreamExt};
use uuid::Uuid;

use crate::agents::ambient::{maker, research, scout};
use crate::ambient::store::AmbientStore;
use crate::config::AppConfig;

pub struct AmbientMcpPool {
    pub news: Arc<dyn adk_core::Toolset>,
    pub real_estate: Option<Arc<dyn adk_core::Toolset>>,
}

pub struct AmbientService {
    _store: AmbientStore,
    _handles: Vec<tokio::task::JoinHandle<()>>,
}

fn research_resolve(text: &str) -> serde_json::Value {
    serde_json::json!({
        "big": "Brief ready",
        "sub": text.lines().next().unwrap_or("ABC Corp research").chars().take(72).collect::<String>(),
        "actions": ["Open", "Save"]
    })
}

fn scout_resolve(text: &str) -> serde_json::Value {
    serde_json::json!({
        "big": "Price dropped",
        "sub": text.lines().next().unwrap_or("12% overnight on watched listing").chars().take(72).collect::<String>(),
        "actions": ["Buy now", "Keep watching"]
    })
}

fn maker_resolve(text: &str) -> serde_json::Value {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(3)
        .map(|l| format!("<b>•</b> {l}"))
        .collect();
    let lines = if lines.is_empty() {
        vec![
            "<b>•</b> Drafted a weekend playlist".into(),
            "<b>•</b> Sketched 3 logo ideas".into(),
            "<b>•</b> Wrote a trip shortlist".into(),
        ]
    } else {
        lines
    };
    serde_json::json!({
        "lines": lines,
        "actions": ["Show me", "Discard"]
    })
}

async fn extract_agent_text(runner: &Runner, user_id: &str, session_id: &str, prompt: &str) -> String {
    let Ok(uid) = UserId::try_from(user_id) else {
        return String::new();
    };
    let Ok(sid) = SessionId::try_from(session_id) else {
        return String::new();
    };

    let Ok(mut stream) = runner
        .run(uid, sid, Content::new("user").with_text(prompt))
        .await
    else {
        return String::new();
    };

    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        if let Ok(ev) = chunk {
            if let Some(content) = ev.content() {
                for part in &content.parts {
                    if let Some(t) = part.text() {
                        text.push_str(t);
                    }
                }
            }
        }
    }
    text.trim().to_string()
}

async fn on_agent_done(
    store: &AmbientStore,
    event_service: &Arc<InMemoryEventSubscriptionService>,
    agent_id: &str,
    task: &str,
    text: &str,
) {
    let resolve = match agent_id {
        "research" => research_resolve(text),
        "scout" => scout_resolve(text),
        "maker" => maker_resolve(text),
        _ => serde_json::json!({ "big": "Done", "sub": text, "actions": ["Open"] }),
    };

    let summary = text.chars().take(200).collect::<String>();
    store
        .set_done(agent_id, task, resolve.clone(), Some(summary.clone()))
        .await;

    let _ = event_service
        .deliver(AwpEvent {
            id: Uuid::new_v4(),
            event_type: "proactive.completed".into(),
            timestamp: Utc::now(),
            payload: serde_json::json!({
                "agent": agent_id,
                "task": task,
                "summary": summary,
                "resolve": resolve,
            }),
        })
        .await;
}

fn spawn_ambient(
    store: AmbientStore,
    runner: Arc<Runner>,
    agent: Arc<dyn adk_core::Agent>,
    agent_id: &'static str,
    cron_expr: &str,
    working_task: &'static str,
    prompt: &'static str,
    event_service: Arc<InMemoryEventSubscriptionService>,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    let trigger = Arc::new(CronTrigger::new(cron_expr)?);
    let store_bg = store.clone();
    let runner_bg = runner.clone();
    let es = event_service.clone();

    let handler: TriggerHandler = Arc::new(move |_event, _agent| {
        let store = store_bg.clone();
        let runner = runner_bg.clone();
        let es = es.clone();
        let aid = agent_id;
        let task = working_task;
        let prompt = prompt;
        Box::pin(async move {
            if store.dnd().await {
                tracing::debug!(agent = aid, "ambient cycle skipped — DND on");
                return Ok(Box::pin(stream::empty()) as adk_core::EventStream);
            }
            store.set_working(aid, task).await;
            let text = extract_agent_text(
                &runner,
                "ambient-user",
                &format!("ambient-{aid}"),
                prompt,
            )
            .await;
            on_agent_done(&store, &es, aid, task, &text).await;
            Ok(Box::pin(stream::empty()) as adk_core::EventStream)
        })
    });

    let mut ambient = AmbientAgent::new(agent, trigger).with_trigger_handler(handler);

    let handle = tokio::spawn(async move {
        if let Err(e) = ambient.start().await {
            tracing::warn!("ambient {agent_id} failed to start: {e:#}");
        }
    });

    Ok(handle)
}

pub async fn boot(
    config: &AppConfig,
    pool: &AmbientMcpPool,
    store: AmbientStore,
    session_service: SharedSessionService,
    event_service: Arc<InMemoryEventSubscriptionService>,
) -> anyhow::Result<AmbientService> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("ambient requires API key");

    let fast = std::env::var("AMBIENT_FAST").ok().as_deref() == Some("1");
    let research_cron = std::env::var("AMBIENT_RESEARCH_CRON")
        .unwrap_or_else(|_| {
            if fast {
                "0 */2 * * * *".into()
            } else {
                "0 */30 * * * *".into()
            }
        });
    let scout_cron = std::env::var("AMBIENT_SCOUT_CRON")
        .unwrap_or_else(|_| {
            if fast {
                "0 */3 * * * *".into()
            } else {
                "0 */15 * * * *".into()
            }
        });
    let maker_cron = std::env::var("AMBIENT_MAKER_CRON")
        .unwrap_or_else(|_| {
            if fast {
                "0 */5 * * * *".into()
            } else {
                "0 0 */1 * * *".into()
            }
        });

    for (sid, app) in [
        ("ambient-research", "agentrix-ambient-research"),
        ("ambient-scout", "agentrix-ambient-scout"),
        ("ambient-maker", "agentrix-ambient-maker"),
    ] {
        session_service
            .create(CreateRequest {
                app_name: app.into(),
                user_id: "ambient-user".into(),
                session_id: Some(sid.into()),
                state: Default::default(),
            })
            .await?;
    }

    let research_agent =
        research::build(api_key, &config.gemini_model, pool.news.clone()).await?;
    let scout_agent = scout::build(
        api_key,
        &config.gemini_model,
        pool.real_estate.clone(),
    )
    .await?;
    let maker_agent = maker::build(api_key, &config.gemini_model).await?;

    let research_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-ambient-research")
            .agent(research_agent.clone())
            .session_service(session_service.clone())
            .build()?,
    );
    let scout_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-ambient-scout")
            .agent(scout_agent.clone())
            .session_service(session_service.clone())
            .build()?,
    );
    let maker_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-ambient-maker")
            .agent(maker_agent.clone())
            .session_service(session_service)
            .build()?,
    );

    let mut handles = Vec::new();
    handles.push(spawn_ambient(
        store.clone(),
        research_runner,
        research_agent,
        "research",
        &research_cron,
        "researching ABC Corp",
        "Research ABC Corp — funding, team, risks. Use news tools.",
        event_service.clone(),
    )?);
    handles.push(spawn_ambient(
        store.clone(),
        scout_runner,
        scout_agent,
        "scout",
        &scout_cron,
        "watching prices",
        "Scout for notable price drops on watched listings.",
        event_service.clone(),
    )?);
    handles.push(spawn_ambient(
        store.clone(),
        maker_runner,
        maker_agent,
        "maker",
        &maker_cron,
        "making something you'd like",
        "Create 3 small delightful outputs for the user.",
        event_service,
    )?);

    tracing::info!(
        "Ambient agents started (research={research_cron}, scout={scout_cron}, maker={maker_cron})"
    );

    Ok(AmbientService {
        _store: store,
        _handles: handles,
    })
}