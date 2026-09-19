use spatial_os::agents::deck::{self, McpPool};
use spatial_os::agents::live::{self, LiveMcpPool};
use spatial_os::agents::lisbon::{self, LisbonMcpPool};
use spatial_os::agents::morning::{self, MorningMcpPool};
use spatial_os::agents::people::{self, PeopleMcpPool};
use spatial_os::agents::week::{self, WeekMcpPool};
use spatial_os::ambient::{service, AmbientStore};
use spatial_os::auth::{self, AuthState};
use spatial_os::config::AppConfig;
use spatial_os::db;
use spatial_os::pg_session::PgSessionService;
use spatial_os::awp_gate::AwpGate;
use spatial_os::routes;
use spatial_os::scenarios::ScenarioLiveFlags;
use spatial_os::state::{AppState, SessionStore, SharedSessionService};
use spatial_os::tools;
use spatial_os::voice::VoiceState;

use std::path::Path;
use std::sync::Arc;

use adk_awp::{
    handlers, middleware::version_negotiation, AwpState, BusinessContextLoader,
    InMemoryEventSubscriptionService,
};
use awp_types::TrustLevel;
use adk_runner::Runner;
use adk_session::{CreateRequest, InMemorySessionService};
use axum::middleware::from_fn;
use axum::routing::{delete, get, post};
use axum::{Extension, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = AppConfig::from_env()?;
    spatial_os::tools::allowlist::init(&config.mcp_allowlists_toml)?;
    if let Err(e) = spatial_os::tools::registry::try_sync(&config, spatial_os::tools::allowlist::catalog()).await {
        tracing::warn!("mcp-registry sync skipped ({e:#})");
    }
    tokio::fs::create_dir_all(&config.artifact_dir).await?;

    let loader = BusinessContextLoader::from_file(&config.business_toml)?;
    {
        let mut ctx = loader.load().as_ref().clone();
        ctx.domain = config.public_domain();
        loader.context_ref().store(std::sync::Arc::new(ctx));
    }
    let biz = loader.load();
    tracing::info!(
        "Loaded business context: {} (domain={})",
        biz.site_name,
        biz.domain
    );
    let brand_greeting_body = biz
        .brand_voice
        .as_ref()
        .and_then(|b| b.greeting.clone())
        .unwrap_or_else(|| {
            "I'm synced and ready — tell me what you'd like to do, or tap Start my day.".into()
        });
    let brand_tone = biz.brand_voice.as_ref().and_then(|b| b.tone.clone());

    let (session_service, session_store, auth_state, pg_pool) = boot_persistence(&config).await?;

    // Permission gate services (S2): ledger, modes, pending actions, audit. Installed before any
    // agent is built so every gated tool shares the same stores as the HTTP routes.
    let ledger_key = std::env::var("LEDGER_HASH_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| "agentrix-ledger-dev-key".into())
        .into_bytes();
    let services = match pg_pool.clone() {
        Some(pool) => spatial_os::permissions::PermissionServices::with_postgres(pool, ledger_key),
        None => spatial_os::permissions::PermissionServices::in_memory(),
    };
    if spatial_os::permissions::gate::init(services).is_err() {
        tracing::warn!("permission services were already initialized — keeping the existing instance");
    }
    // Personal memory (S3): known · assumed · recommended, sensitive values encrypted at rest.
    let memory = spatial_os::memory::MemoryService::new(
        pg_pool.clone(),
        spatial_os::memory::crypto::Crypto::from_env_or_dev(),
    );
    if spatial_os::memory::init(memory).is_err() {
        tracing::warn!("memory service was already initialized — keeping the existing instance");
    }
    // Consents (S11-T4, pulled forward): one store behind the AWP consent endpoints and the OS.
    let consents = spatial_os::memory::consent::ConsentStore::new(pg_pool.clone());
    if spatial_os::memory::consent::init(consents).is_err() {
        tracing::warn!("consent store was already initialized — keeping the existing instance");
    }
    // Tasks (S4-T3): shared by Work Productivity and Home Personal Productivity.
    let tasks = spatial_os::tools::tasks::TaskStore::new(pg_pool.clone());
    if spatial_os::tools::tasks::init(tasks).is_err() {
        tracing::warn!("task store was already initialized — keeping the existing instance");
    }
    if pg_pool.is_none() {
        tracing::warn!(
            "Phase 2 persistence disabled — demo/dev mode (ledger, permissions, memory, consents and tasks are in-memory; see ADR-006)"
        );
    }

    let mut deck_runner = None;
    let mut combine_runner = None;
    let mut deck_mcp = None;
    let mut deck_enabled = false;

    if config.agents_enabled() {
        match boot_deck_stack(&config, session_service.clone()).await {
            Ok((deck, combine, pool)) => {
                tracing::info!("Deck + combine workflows enabled (MCP + Gemini)");
                deck_runner = Some(deck);
                combine_runner = Some(combine);
                deck_mcp = Some(pool);
                deck_enabled = true;
            }
            Err(e) => {
                tracing::warn!("Deck workflow unavailable ({e:#}) — deck stays mock");
            }
        }
    } else {
        tracing::warn!("GOOGLE_API_KEY not set — deck uses mock SSE");
    }

    let mut morning_runner = None;
    let mut morning_mcp = None;
    let mut morning_enabled = false;

    if config.agents_enabled() {
        match boot_morning_stack(&config, session_service.clone()).await {
            Ok((runner, pool)) => {
                tracing::info!("Morning workflow enabled (news + weather MCP + Gemini)");
                morning_runner = Some(runner);
                morning_mcp = Some(pool);
                morning_enabled = true;
            }
            Err(e) => {
                tracing::warn!("Morning workflow unavailable ({e:#}) — morning stays mock");
            }
        }
    }

    let mut live_runner = None;
    let mut live_mcp = None;
    let mut live_enabled = false;

    let mut people_runner = None;
    let mut people_mcp = None;
    let mut people_enabled = false;

    let mut week_runner = None;
    let mut week_mcp = None;
    let mut week_enabled = false;

    let mut lisbon_runner = None;
    let mut lisbon_mcp = None;
    let mut lisbon_enabled = false;

    if config.agents_enabled() {
        if let Ok((runner, pool)) = boot_live_stack(&config, session_service.clone()).await {
            tracing::info!("Live workflow enabled (mcp-news)");
            live_runner = Some(runner);
            live_mcp = Some(pool);
            live_enabled = true;
        }
        if let Ok((runner, pool)) = boot_people_stack(&config, session_service.clone()).await {
            tracing::info!("People workflow enabled");
            people_runner = Some(runner);
            people_mcp = Some(pool);
            people_enabled = true;
        }
        if let Ok((runner, pool)) = boot_week_stack(&config, session_service.clone()).await {
            tracing::info!("Week workflow enabled");
            week_runner = Some(runner);
            week_mcp = Some(pool);
            week_enabled = true;
        }
        if let Ok((runner, pool)) = boot_lisbon_stack(&config, session_service.clone()).await {
            tracing::info!("Lisbon workflow enabled (weather + itinerary)");
            lisbon_runner = Some(runner);
            lisbon_mcp = Some(pool);
            lisbon_enabled = true;
        }
    }

    let mut router_runner = None;
    let mut suzy_runner = None;
    let coordinator_enabled = config.agents_enabled();

    if coordinator_enabled {
        match boot_coordinator_stack(&config, session_service.clone()).await {
            Ok((router, suzy)) => {
                tracing::info!("Suzy coordinator + LLM router enabled");
                router_runner = Some(router);
                suzy_runner = Some(suzy);
            }
            Err(e) => {
                tracing::warn!("Coordinator unavailable ({e:#}) — keyword routing + static summaries");
            }
        }
    }

    let mut greeting_runner = None;
    if config.agents_enabled() {
        match boot_greeting_stack(&config, session_service.clone()).await {
            Ok(runner) => {
                tracing::info!("Greeting agent enabled (integration-fact composition)");
                greeting_runner = Some(runner);
            }
            Err(e) => {
                tracing::warn!("Greeting agent unavailable ({e:#}) — deterministic/brand fallback");
            }
        }
    }

    let event_service = Arc::new(InMemoryEventSubscriptionService::new());

    let ambient_store = AmbientStore::new();
    let mut ambient_enabled = false;
    let mut ambient_service = None;

    if config.agents_enabled() {
        match boot_ambient_stack(
            &config,
            ambient_store.clone(),
            session_service.clone(),
            event_service.clone(),
        )
        .await
        {
            Ok((svc, enabled)) => {
                ambient_enabled = enabled;
                ambient_service = Some(svc);
            }
            Err(e) => {
                tracing::warn!("Ambient layer unavailable ({e:#}) — proactive uses store defaults");
            }
        }
    }

    let scenario_flags = ScenarioLiveFlags {
        deck: deck_enabled,
        morning: morning_enabled,
        live: live_enabled,
        people: people_enabled,
        week: week_enabled,
        lisbon: lisbon_enabled,
        proactive: true,
    };

    let awp = Arc::new(AwpGate::new(
        config.jwt_secret.clone(),
        loader.context_ref(),
    ));

    let voice = VoiceState::boot(&config);
    if voice.enabled {
        tracing::info!(
            "Gemini Live voice enabled (model={}, voice={})",
            config.gemini_live_model,
            voice.voice_name
        );
    }

    let runtime = spatial_os::state::RuntimeStatus {
        milestone: "M11",
        phase: "P2-S3",
        agents_enabled: config.agents_enabled(),
        postgres_enabled: config.postgres_enabled(),
        auth_enabled: config.auth_enabled(),
        voice_enabled: voice.enabled,
        coordinator_enabled,
        uses_mock_orchestration: !deck_enabled,
        scenarios: scenario_flags,
        mcp_worksheet: config.mcp_worksheet_path.exists(),
        mcp_docx: config.mcp_docx_path.exists(),
        mcp_slides: config.mcp_slides_path.exists(),
        mcp_news: config.mcp_news_path.exists(),
        allow_demo_mode: config.allow_demo_mode,
        public_domain: config.public_domain(),
        signup_endpoint: config.signup_endpoint.clone(),
        linkedin_partner_id: config.linkedin_partner_id.clone(),
        linkedin_conversion_id: config.linkedin_conversion_id,
    };

    if runtime.uses_mock_orchestration {
        tracing::warn!(
            "Orchestration mock fallback active — set GOOGLE_API_KEY and build MCP servers for live agents"
        );
    }

    let mut mother_runner = None;
    if coordinator_enabled {
        match boot_mother_stack(&config, session_service.clone(), session_store.clone()).await {
            Ok(runner) => {
                tracing::info!("Mother Agent (LLM half) enabled");
                mother_runner = Some(runner);
            }
            Err(e) => tracing::warn!("Mother Agent LLM unavailable ({e:#}) — deterministic intake/synthesis"),
        }
    }

    let mut app_state = AppState::new(
        runtime,
        session_store,
        config.artifact_dir.clone(),
        scenario_flags,
        coordinator_enabled,
        deck_runner,
        combine_runner,
        morning_runner,
        live_runner,
        people_runner,
        week_runner,
        lisbon_runner,
        router_runner,
        suzy_runner,
        session_service,
        auth_state.clone(),
        awp.clone(),
        event_service.clone(),
        deck_mcp,
        morning_mcp,
        live_mcp,
        people_mcp,
        week_mcp,
        lisbon_mcp,
        ambient_store,
        ambient_enabled,
        greeting_runner,
        brand_greeting_body,
        brand_tone,
        voice,
    );
    app_state.mother_runner = mother_runner;
    let session_store = app_state.sessions.clone();

    // Known trust is verified by `JwtTrustAssigner`; the A2A dispatcher stays on
    // `POST /awp/a2a` (routes::intent::a2a_intent), so adk-awp's own handler is unset.
    let awp_state = AwpState::builder(loader.context_ref())
        .rate_limiter(awp.rate_limiter.clone())
        .consent_service(Arc::new(spatial_os::memory::consent::handle().clone()))
        .event_service(event_service.clone())
        .trust_assigner(awp.trust_assigner.clone())
        .supported_trust_levels([TrustLevel::Anonymous, TrustLevel::Known])
        .build();

    let api = Router::new()
        .route("/health", get(routes::health::health))
        .route("/api/public-config", get(routes::public::public_config))
        .route("/api/greeting", get(routes::greeting::get_greeting))
        .route("/api/people", get(routes::people::get_people))
        .route("/api/live", get(routes::live::get_live))
        .route("/api/rails/background", get(routes::background::get_background))
        .route("/api/ambient", get(routes::ambient::stream_ambient))
        .route("/api/ambient/status", get(routes::ambient::list_ambient))
        .route("/api/ambient/dnd", post(routes::ambient::set_dnd))
        .route("/api/oauth/{provider}", get(routes::oauth::oauth_guide))
        .route("/api/sessions", post(routes::session::create_session))
        .route("/api/voice/status", get(routes::voice::status))
        .route("/ws/voice", get(routes::voice::ws_voice));

    let mut api = api;

    if let Some(auth) = auth_state {
        api = api
            .merge(
                Router::new()
                    .route("/api/auth/google", get(auth::google_redirect))
                    .route("/api/auth/google/callback", get(auth::google_callback))
                    .route("/api/auth/providers", get(auth::providers))
                    .route("/api/auth/dev", post(auth::dev_login))
                    .route("/api/auth/me", get(auth::me))
                    .route("/api/auth/logout", post(auth::logout))
                    .with_state(auth),
            );
    }

    let api = api
        .route(
            "/api/sessions/{session_id}/intent",
            post(routes::intent::submit_intent),
        )
        .route(
            "/api/sessions/{session_id}/action",
            post(routes::action::submit_action),
        )
        .route(
            "/api/sessions/{session_id}/chat",
            post(routes::chat::chat).get(routes::chat::history),
        )
        .route(
            "/api/sessions/{session_id}/events",
            post(routes::events::record),
        )
        .route("/api/actions", get(routes::actions::list))
        .route("/api/actions/approve", post(routes::actions::approve_batch))
        .route("/api/actions/{id}/approve", post(routes::actions::approve))
        .route("/api/actions/{id}/reject", post(routes::actions::reject))
        .route("/api/actions/{id}/edit", post(routes::actions::edit))
        .route("/api/audit", get(routes::actions::audit))
        .route(
            "/api/permissions",
            get(routes::permissions::get).put(routes::permissions::put),
        )
        .route("/api/pause", post(routes::permissions::pause))
        .route("/api/resume", post(routes::permissions::resume))
        .route(
            "/api/memory",
            get(routes::memory::list).post(routes::memory::remember).delete(routes::memory::purge),
        )
        .route(
            "/api/consents",
            get(routes::consents::get).put(routes::consents::put),
        )
        .route("/api/tasks", get(routes::tasks::list))
        .route("/api/memory/export", get(routes::memory::export))
        .route(
            "/api/memory/{id}",
            axum::routing::patch(routes::memory::patch).delete(routes::memory::forget),
        )
        .route(
            "/api/sessions/{session_id}/fuse",
            post(routes::fuse::fuse_cards),
        )
        .route(
            "/api/sessions/{session_id}/cards",
            get(routes::cards::list_cards),
        )
        .route(
            "/api/sessions/{session_id}/agents",
            get(routes::agents::list_agents),
        )
        .route(
            "/api/sessions/{session_id}/commit",
            post(routes::commit::commit_action),
        )
        .route(
            "/api/agents/{agent_id}/snooze",
            post(routes::agents::snooze_agent),
        )
        .route(
            "/api/agents/{agent_id}/wake",
            post(routes::agents::wake_agent),
        )
        .route(
            "/artifacts/{user_id}/{session_id}/{*path}",
            get(routes::artifacts::get_scoped),
        )
        .route(
            "/artifacts/{session_id}/{filename}",
            get(routes::artifacts::get_legacy),
        )
        .route("/awp/events/subscribe", post(routes::awp::subscribe))
        .route("/awp/a2a", post(routes::intent::a2a_intent))
        .with_state(app_state)
        .merge(awp_router(awp_state))
        .layer(from_fn(version_negotiation))
        .layer(CorsLayer::very_permissive());

    let mut app = api.layer(Extension(session_store));

    if Path::new(&config.audio_dir).exists() {
        app = app.nest_service("/audio", ServeDir::new(&config.audio_dir));
    }

    if Path::new(&config.static_dir).exists() {
        app = app.nest_service("/static", ServeDir::new(&config.static_dir));
    }

    if Path::new(&config.web_dir).exists() {
        app = app.fallback_service(
            ServeDir::new(&config.web_dir).append_index_html_on_directories(true),
        );
    } else {
        tracing::warn!("web dir missing at {}", config.web_dir.display());
    }

    let _ambient_service = ambient_service;

    let addr = config.addr();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Agentrix OS listening on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn boot_deck_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<Runner>, Arc<McpPool>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let worksheet = tools::mcp::spawn_mcp_server(&config.mcp_worksheet_path).await?;
    let docx = tools::mcp::spawn_mcp_server(&config.mcp_docx_path).await?;
    let slides = tools::mcp::spawn_mcp_server(&config.mcp_slides_path).await?;

    let w = tools::mcp::health_check(&worksheet).await?;
    let d = tools::mcp::health_check(&docx).await?;
    let s = tools::mcp::health_check(&slides).await?;
    tracing::info!("MCP tools ready: worksheet={w}, docx={d}, slides={s}");

    let pool = Arc::new(McpPool {
        worksheet: Arc::new(worksheet),
        docx: Arc::new(docx),
        slides: Arc::new(slides),
    });

    let workflow = deck::build_workflow(api_key, &config.gemini_model, pool.as_ref()).await?;
    let combine_agent =
        spatial_os::agents::combine::build(api_key, &config.gemini_model, pool.slides.clone())
            .await?;

    let deck_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os")
            .agent(workflow)
            .session_service(session_service.clone())
            .build()?,
    );

    let combine_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-combine")
            .agent(combine_agent)
            .session_service(session_service)
            .build()?,
    );

    Ok((deck_runner, combine_runner, pool))
}

async fn boot_morning_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<MorningMcpPool>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let news = tools::mcp::spawn_mcp_server(&config.mcp_news_path).await?;
    let weather = tools::mcp::spawn_mcp_server(&config.mcp_weather_path).await?;
    let n = tools::mcp::health_check(&news).await?;
    let w = tools::mcp::health_check(&weather).await?;
    tracing::info!("Morning MCP ready: news={n}, weather={w}");

    let calendar = tools::mcp::try_spawn_mcp_server(&config.mcp_calendar_path)
        .await
        .map(|t| {
            tracing::info!("Calendar MCP connected");
            Arc::new(t) as Arc<dyn adk_core::Toolset>
        });
    let email = tools::mcp::try_spawn_mcp_server(&config.mcp_email_path)
        .await
        .map(|t| {
            tracing::info!("Email MCP connected");
            Arc::new(t) as Arc<dyn adk_core::Toolset>
        });

    if calendar.is_none() {
        tracing::warn!("Calendar MCP unavailable — Today card uses brief context only");
    }
    if email.is_none() {
        tracing::warn!("Email MCP unavailable — set SMTP/IMAP or run mcp-email auth gmail");
    }

    let pool = Arc::new(MorningMcpPool {
        calendar,
        email,
        news: Arc::new(news),
        weather: Arc::new(weather),
    });

    let workflow = morning::build_workflow(api_key, &config.gemini_model, pool.as_ref()).await?;

    let runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-morning")
            .agent(workflow)
            .session_service(session_service)
            .build()?,
    );

    Ok((runner, pool))
}

async fn boot_mother_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
    sessions: SessionStore,
) -> anyhow::Result<Arc<Runner>> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");
    let agent = spatial_os::mother::agent::build(api_key, &config.gemini_model, sessions).await?;
    Ok(Arc::new(
        Runner::builder()
            .app_name("agentrix-os-mother")
            .agent(agent)
            .session_service(session_service)
            .build()?,
    ))
}

async fn boot_coordinator_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<Runner>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let router_agent =
        spatial_os::agents::router::build(api_key, &config.gemini_model).await?;
    let suzy_agent = spatial_os::agents::suzy::build(api_key, &config.gemini_model).await?;

    let router_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-router")
            .agent(router_agent)
            .session_service(session_service.clone())
            .build()?,
    );

    let suzy_runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-suzy")
            .agent(suzy_agent)
            .session_service(session_service)
            .build()?,
    );

    Ok((router_runner, suzy_runner))
}

async fn boot_live_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<LiveMcpPool>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let news = tools::mcp::spawn_mcp_server(&config.mcp_news_path).await?;
    let n = tools::mcp::health_check(&news).await?;
    tracing::info!("Live MCP: news={n}");

    let market_data = tools::mcp::try_spawn_mcp_server(&config.mcp_market_data_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);

    let pool = Arc::new(LiveMcpPool {
        news: Arc::new(news),
        market_data,
    });

    let workflow = live::build_workflow(api_key, &config.gemini_model, pool.as_ref()).await?;
    let runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-live")
            .agent(workflow)
            .session_service(session_service)
            .build()?,
    );
    Ok((runner, pool))
}

async fn boot_people_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<PeopleMcpPool>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let slack = tools::mcp::try_spawn_mcp_server(&config.mcp_slack_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);
    let crm = tools::mcp::try_spawn_mcp_server(&config.mcp_crm_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);
    let calendar = tools::mcp::try_spawn_mcp_server(&config.mcp_calendar_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);

    let pool = Arc::new(PeopleMcpPool {
        slack,
        crm,
        calendar,
    });

    let workflow = people::build_workflow(api_key, &config.gemini_model, pool.as_ref()).await?;
    let runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-people")
            .agent(workflow)
            .session_service(session_service)
            .build()?,
    );
    Ok((runner, pool))
}

async fn boot_week_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<WeekMcpPool>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let banking = tools::mcp::try_spawn_mcp_server(&config.mcp_banking_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);
    let github = tools::mcp::try_spawn_mcp_server(&config.mcp_github_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);

    let pool = Arc::new(WeekMcpPool {
        banking,
        github,
        health_csv: week::health_csv_from_env(&manifest_dir),
    });

    let workflow = week::build_workflow(api_key, &config.gemini_model, pool.as_ref()).await?;
    let runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-week")
            .agent(workflow)
            .session_service(session_service)
            .build()?,
    );
    Ok((runner, pool))
}

async fn boot_greeting_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<Arc<Runner>> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let agent =
        spatial_os::greeting::agent::build(api_key, &config.gemini_model).await?;

    session_service
        .create(CreateRequest {
            app_name: "agentrix-os-greeting".into(),
            user_id: "greeting-user".into(),
            session_id: Some("greeting-session".into()),
            state: Default::default(),
        })
        .await?;

    let runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-greeting")
            .agent(agent)
            .session_service(session_service)
            .build()?,
    );

    Ok(runner)
}

async fn boot_ambient_stack(
    config: &AppConfig,
    store: AmbientStore,
    session_service: SharedSessionService,
    event_service: Arc<InMemoryEventSubscriptionService>,
) -> anyhow::Result<(service::AmbientService, bool)> {
    let news = tools::mcp::spawn_mcp_server(&config.mcp_news_path).await?;
    let n = tools::mcp::health_check(&news).await?;
    tracing::info!("Ambient MCP: news={n}");

    let real_estate = tools::mcp::try_spawn_mcp_server(&config.mcp_real_estate_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);

    let pool = service::AmbientMcpPool {
        news: Arc::new(news),
        real_estate,
    };

    let svc = service::boot(
        config,
        &pool,
        store,
        session_service,
        event_service,
    )
    .await?;

    Ok((svc, true))
}

async fn boot_lisbon_stack(
    config: &AppConfig,
    session_service: SharedSessionService,
) -> anyhow::Result<(Arc<Runner>, Arc<LisbonMcpPool>)> {
    let api_key = config
        .google_api_key
        .as_deref()
        .expect("agents_enabled implies API key");

    let weather = tools::mcp::spawn_mcp_server(&config.mcp_weather_path).await?;
    let w = tools::mcp::health_check(&weather).await?;
    tracing::info!("Lisbon MCP: weather={w}");

    let maps = tools::mcp::try_spawn_mcp_server(&config.mcp_maps_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);
    let real_estate = tools::mcp::try_spawn_mcp_server(&config.mcp_real_estate_path)
        .await
        .map(|t| Arc::new(t) as Arc<dyn adk_core::Toolset>);

    let pool = Arc::new(LisbonMcpPool {
        maps,
        weather: Arc::new(weather),
        real_estate,
    });

    let workflow = lisbon::build_workflow(api_key, &config.gemini_model, pool.as_ref()).await?;
    let runner = Arc::new(
        Runner::builder()
            .app_name("agentrix-os-lisbon")
            .agent(workflow)
            .session_service(session_service)
            .build()?,
    );
    Ok((runner, pool))
}

async fn boot_persistence(
    config: &AppConfig,
) -> anyhow::Result<(
    SharedSessionService,
    SessionStore,
    Option<Arc<AuthState>>,
    Option<sqlx::PgPool>,
)> {
    let Some(database_url) = config.database_url.as_deref() else {
        tracing::info!("DATABASE_URL unset — in-memory sessions (M4 behaviour)");
        return Ok((
            Arc::new(InMemorySessionService::new()),
            SessionStore::new(),
            None,
            None,
        ));
    };

    let pool = db::connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Postgres connected — ui_sessions + agent_sessions persisted");

    let session_service: SharedSessionService =
        Arc::new(PgSessionService::new(pool.clone()));
    let session_store = SessionStore::with_postgres(pool.clone());

    let auth_state = config.jwt_secret.as_ref().map(|jwt_secret| {
        Arc::new(AuthState {
            db: pool.clone(),
            jwt_secret: jwt_secret.clone(),
            google_client_id: config.google_oauth_client_id.clone(),
            google_client_secret: config.google_oauth_client_secret.clone(),
            base_url: config.base_url.clone(),
        })
    });

    if auth_state.is_some() {
        tracing::info!("Google OAuth + JWT auth enabled");
    }

    Ok((session_service, session_store, auth_state, Some(pool)))
}

fn awp_router(state: AwpState) -> Router {
    Router::new()
        .route("/.well-known/awp.json", get(handlers::discovery))
        .route("/awp/manifest", get(handlers::manifest))
        .route("/awp/health", get(handlers::health))
        .route("/awp/events/subscriptions", get(handlers::list_subscriptions))
        .route(
            "/awp/events/subscriptions/{id}",
            delete(handlers::delete_subscription),
        )
        .layer(from_fn(version_negotiation))
        .with_state(state)
}