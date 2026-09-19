//! Validation suite for Agentrix OS M1 deck stack.
//!
//! Run: `cargo test --test validate`
//! Full E2E (slow, uses Gemini + MCP): `cargo test --test validate deck_workflow -- --ignored`

mod common;

use std::sync::Arc;

use adk_core::{Content, Llm, LlmRequest, ReadonlyContext};
use adk_model::gemini::GeminiModel;
use futures::StreamExt;
use spatial_os::config::AppConfig;
use spatial_os::events::mock;
use spatial_os::scenarios;
use spatial_os::tools::mcp;

#[test]
fn gemini_model_configuration() {
    common::load_env();
    assert_eq!(common::gemini_model(), "gemini-3.1-flash-lite");
    assert_eq!(
        AppConfig::from_env().expect("config").gemini_model,
        "gemini-3.1-flash-lite"
    );
}

#[tokio::test]
async fn mock_deck_intent_maps_to_scenario_events() {
    assert_eq!(mock::pick_scenario("Build me a pitch deck"), "deck");
    let types = mock::preview_intent_event_types("Build me a pitch deck", 4).await;
    assert_eq!(types[0], "scenario");
    assert!(types.contains(&"card_spawn".to_string()));
}

#[tokio::test]
async fn mcp_servers_spawn_and_expose_tools() {
    let paths = common::mcp_paths();
    common::assert_mcp_binaries_exist(&paths);

    let worksheet = mcp::spawn_mcp_server(&paths.worksheet).await.expect("worksheet");
    let docx = mcp::spawn_mcp_server(&paths.docx).await.expect("docx");
    let slides = mcp::spawn_mcp_server(&paths.slides).await.expect("slides");

    let w = mcp::health_check(&worksheet).await.expect("worksheet health");
    let d = mcp::health_check(&docx).await.expect("docx health");
    let s = mcp::health_check(&slides).await.expect("slides health");

    assert!(w > 10, "worksheet should expose many tools, got {w}");
    assert!(d > 10, "docx should expose many tools, got {d}");
    assert!(s > 10, "slides should expose many tools, got {s}");
}

#[tokio::test]
async fn gemini_31_flash_lite_responds() {
    let api_key = common::google_api_key();
    let model_name = common::gemini_model();
    assert_eq!(model_name, "gemini-3.1-flash-lite");

    let model = GeminiModel::new(&api_key, &model_name).expect("gemini model");
    let request = LlmRequest::new(
        &model_name,
        vec![Content::new("user").with_text("Reply with exactly the word PONG.")],
    );

    let mut stream = model
        .generate_content(request, false)
        .await
        .expect("generate_content should succeed");

    let response = stream
        .next()
        .await
        .expect("one response chunk")
        .expect("response ok");

    let text = response
        .content
        .expect("content")
        .parts
        .iter()
        .filter_map(|p| match p {
            adk_core::Part::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert!(
        text.to_uppercase().contains("PONG"),
        "expected PONG in model reply, got: {text}"
    );
}

#[test]
fn combine_action_routing_is_live_for_deck() {
    assert_eq!(scenarios::pick_action("combine"), Some("combine"));
    assert!(scenarios::action_is_live("combine", Some("deck"), true));
    assert!(!scenarios::action_is_live("combine", Some("morning"), true));
    use spatial_os::scenarios::ScenarioLiveFlags;
    assert!(scenarios::intent_is_live(
        "deck",
        ScenarioLiveFlags {
            deck: true,
            ..Default::default()
        }
    ));
    assert!(scenarios::intent_is_live(
        "morning",
        ScenarioLiveFlags {
            morning: true,
            ..Default::default()
        }
    ));
    assert!(scenarios::intent_is_live(
        "live",
        ScenarioLiveFlags {
            live: true,
            ..Default::default()
        }
    ));
}

#[tokio::test]
async fn mock_combine_action_stream_completes_with_events() {
    use std::time::Duration;

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = mock::stream_action("combine", Some("deck"), "combine");
        let mut count = 0usize;
        while let Some(item) = stream.next().await {
            let _ = item.expect("sse event");
            count += 1;
        }
        count
    })
    .await
    .expect("action stream should finish within 5s");

    assert!(
        result >= 3,
        "deck combine mock should emit conduct + deck_finish + done, got {result}"
    );
}

#[test]
fn tour_prompts_advance_in_order() {
    use spatial_os::scenarios::tour;

    assert_eq!(tour::next_scenario("morning"), Some("people"));
    assert_eq!(tour::action_prompt("deck"), Some("combine"));
    assert_eq!(tour::scenario_prompt("live"), Some("What is happening live"));
}

#[test]
fn keyword_router_still_maps_pitch_to_deck() {
    assert_eq!(mock::pick_scenario("pitch presentation"), "deck");
    assert_eq!(mock::pick_scenario("Start my day"), "morning");
}

#[tokio::test]
async fn router_agent_builds_with_gemini() {
    let api_key = common::google_api_key();
    let agent =
        spatial_os::agents::router::build(&api_key, &common::gemini_model())
            .await
            .expect("router should build");
    assert_eq!(agent.name(), "intent_router");
}

#[tokio::test]
async fn live_workflow_builds_with_news_mcp() {
    let api_key = common::google_api_key();
    let paths = common::mcp_paths();
    let news = mcp::spawn_mcp_server(&paths.news).await.expect("news");

    let pool = spatial_os::agents::live::LiveMcpPool {
        news: Arc::new(news),
        market_data: None,
    };

    spatial_os::agents::live::build_workflow(&api_key, &common::gemini_model(), &pool)
        .await
        .expect("live workflow should build");
}

#[tokio::test]
async fn suzy_agent_builds_with_gemini() {
    let api_key = common::google_api_key();
    let agent = spatial_os::agents::suzy::build(&api_key, &common::gemini_model())
        .await
        .expect("suzy should build");
    assert_eq!(agent.name(), "suzy_coordinator");
}

#[tokio::test]
async fn session_persistence_cards_and_agents() {
    let store = spatial_os::state::SessionStore::new();
    let record = store.create().await;
    let sid = record.session_id;

    store
        .set_scenario(&sid, "deck", Some("Build me a pitch deck"))
        .await;
    store
        .upsert_card(
            &sid,
            0,
            serde_json::json!({"glyph":"📊","title":"Auto-Excel","agent":"auto-excel"}),
            "resolved",
            Some(serde_json::json!({"big":"+38%","sub":"ready"})),
            false,
        )
        .await;
    store
        .agent_snooze(
            &sid,
            spatial_os::state::AgentRecord {
                id: "auto-excel".into(),
                title: "Auto-Excel".into(),
                glyph: "📊".into(),
                agent: "auto-excel".into(),
                rail: "resting".into(),
                domain: Default::default(),
            },
        )
        .await;

    let cards = store.list_cards(&sid).await.expect("cards");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].status, "resolved");

    let (active, resting) = store.list_agents(&sid).await.expect("agents");
    assert!(active.is_empty());
    assert_eq!(resting.len(), 1);

    let woke = store.agent_wake(&sid, "auto-excel").await.expect("wake");
    assert_eq!(woke.title, "Auto-Excel");
}

#[tokio::test]
async fn morning_phase_b_mcp_spawn() {
    let paths = common::mcp_paths();
    assert!(paths.news.exists(), "build mcp-news: cd mcp-servers/mcp-news && cargo build --release");
    assert!(
        paths.weather.exists(),
        "build mcp-weather: cd mcp-servers/mcp-weather && cargo build --release"
    );

    let news = mcp::spawn_mcp_server(&paths.news).await.expect("news");
    let weather = mcp::spawn_mcp_server(&paths.weather).await.expect("weather");
    assert!(mcp::health_check(&news).await.expect("news health") > 5);
    assert!(mcp::health_check(&weather).await.expect("weather health") > 3);
}

#[tokio::test]
async fn morning_workflow_builds_with_news_and_weather() {
    let api_key = common::google_api_key();
    let paths = common::mcp_paths();

    let news = Arc::new(mcp::spawn_mcp_server(&paths.news).await.unwrap());
    let weather = Arc::new(mcp::spawn_mcp_server(&paths.weather).await.unwrap());

    let pool = spatial_os::agents::morning::MorningMcpPool {
        calendar: None,
        email: None,
        news,
        weather,
    };

    spatial_os::agents::morning::build_workflow(&api_key, &common::gemini_model(), &pool)
        .await
        .expect("morning workflow should build");
}

#[tokio::test]
async fn combine_agent_builds_with_configured_model() {
    let api_key = common::google_api_key();
    let paths = common::mcp_paths();
    common::assert_mcp_binaries_exist(&paths);

    let slides = mcp::spawn_mcp_server(&paths.slides).await.expect("slides");
    spatial_os::agents::combine::build(&api_key, &common::gemini_model(), Arc::new(slides))
        .await
        .expect("combine agent should build");
}

#[tokio::test]
async fn greeting_brand_fallback_is_honest() {
    let payload = spatial_os::greeting::compose(
        None,
        None,
        "I'm synced and ready — tell me what you'd like to do.",
        Some("warm, confident"),
        None
    )
    .await;
    assert_eq!(payload.source, "brand");
    assert!(!payload.full_text.to_lowercase().contains("meetings"));
    assert!(!payload.full_text.to_lowercase().contains("emails"));
    assert!(payload.full_text.contains("I'm synced and ready"));
    assert_eq!(payload.audio_clip, "/audio/greeting.wav");
}

#[tokio::test]
async fn greeting_agent_builds_with_gemini() {
    let api_key = common::google_api_key();
    spatial_os::greeting::agent::build(&api_key, &common::gemini_model())
        .await
        .expect("greeting agent should build");
}

fn awp_gate_fixture() -> spatial_os::awp_gate::AwpGate {
    use adk_awp::BusinessContextLoader;

    let path = common::manifest_dir().join("business.toml");
    let loader = BusinessContextLoader::from_file(&path).expect("business.toml");
    spatial_os::awp_gate::AwpGate::new(Some("validate-jwt-secret".into()), loader.context_ref())
}

#[tokio::test]
async fn dev_auth_jwt_unlocks_known_awp_capabilities() {
    common::load_env();
    let Ok(jwt_secret) = std::env::var("JWT_SECRET") else {
        return;
    };
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }

    let pool = common::postgres_pool().await;
    let email = format!("dev-validate-{}@localhost", uuid::Uuid::new_v4());
    let user = spatial_os::db::create_user(&pool, &email, Some("Dev"), "dev", Some(&email), None)
        .await
        .expect("dev user");
    let token = spatial_os::auth::create_token(user.id, &jwt_secret).expect("jwt");

    let gate = awp_gate_fixture();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    gate.check(&headers, "action:sess-1", "submit_action")
        .await
        .expect("dev JWT should satisfy known capability");
}

#[test]
fn voice_state_boots_when_api_key_present() {
    common::load_env();
    let config = AppConfig::from_env().expect("config");
    let voice = spatial_os::voice::VoiceState::boot(&config);
    if config.voice_enabled() {
        assert!(voice.enabled);
        assert!(voice.model.is_some());
    } else {
        assert!(!voice.enabled);
    }
}

#[test]
fn voice_state_camera_follows_voice_and_flag() {
    use spatial_os::voice::camera::{instruction, Gesture, TOOL_NAME};

    common::load_env();
    let config = AppConfig::from_env().expect("config");
    let voice = spatial_os::voice::VoiceState::boot(&config);
    assert_eq!(voice.camera, voice.enabled && config.camera_enabled, "camera needs voice and AGENTRIX_CAMERA");

    // What the client relies on exists whether or not a key is configured.
    assert!(spatial_os::routes::events::UI_KINDS.contains(&"ui_gesture"), "content-free gesture rows");
    assert!(spatial_os::memory::consent::CATEGORIES.contains(&"camera"), "camera is a consent category");
    assert_eq!(TOOL_NAME, "ui_gesture");
    let text = instruction();
    for g in Gesture::ALL {
        assert!(text.contains(g.as_str()) && text.contains(g.effect()), "{}", g.as_str());
    }
}

#[tokio::test]
async fn voice_state_status_route_reports_camera() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let state = offline_app_state();
    let expected = state.voice.camera;
    let app = axum::Router::new()
        .route("/api/voice/status", axum::routing::get(spatial_os::routes::voice::status))
        .with_state(state);
    let response = app.oneshot(Request::get("/api/voice/status").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(json["camera"], serde_json::Value::Bool(expected));
    assert_eq!(json["ws_path"], "/ws/voice");
    if !json["enabled"].as_bool().unwrap() {
        assert_eq!(json["camera"], false, "no camera without voice");
    }
}

fn awp_test_state() -> adk_awp::AwpState {
    use adk_awp::{
        AwpState, BusinessContextLoader, DefaultTrustAssigner, InMemoryConsentService,
        InMemoryEventSubscriptionService, InMemoryRateLimiter,
    };
    use std::sync::Arc;

    let path = common::manifest_dir().join("business.toml");
    let loader = BusinessContextLoader::from_file(&path).expect("business.toml");
    let event_service = Arc::new(InMemoryEventSubscriptionService::new());
    AwpState::builder(loader.context_ref())
        .rate_limiter(Arc::new(InMemoryRateLimiter::new()))
        .consent_service(Arc::new(InMemoryConsentService::new()))
        .event_service(event_service)
        .trust_assigner(Arc::new(DefaultTrustAssigner))
        .build()
}

#[tokio::test]
async fn awp_discovery_document_valid() {
    use adk_awp::awp_routes;
    use awp_types::{AwpDiscoveryDocument, CURRENT_VERSION};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = awp_routes(awp_test_state());
    let response = app
        .oneshot(Request::get("/.well-known/awp.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 16_384)
        .await
        .unwrap();
    let doc: AwpDiscoveryDocument = serde_json::from_slice(&body).unwrap();
    assert_eq!(doc.version, CURRENT_VERSION);
    assert!(doc.capability_manifest_url.contains("/awp/manifest"));
    assert!(doc.a2a_endpoint_url.contains("/awp/a2a"));
}

#[tokio::test]
async fn awp_manifest_lists_submit_intent() {
    use adk_awp::awp_routes;
    use awp_types::CapabilityManifest;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = awp_routes(awp_test_state());
    let response = app
        .oneshot(Request::get("/awp/manifest").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 65_536)
        .await
        .unwrap();
    let manifest: CapabilityManifest = serde_json::from_slice(&body).unwrap();
    assert!(manifest.capabilities.iter().any(|c| c.name == "submit_intent"));
    assert!(manifest.capabilities.iter().any(|c| c.name == "stream_voice"));
}

#[tokio::test]
#[ignore = "set AGENTRIX_DEPLOY_URL=https://your-host to run production AWP conformance"]
async fn awp_conformance_against_deploy_url() {
    let base = std::env::var("AGENTRIX_DEPLOY_URL").expect("AGENTRIX_DEPLOY_URL");
    let client = reqwest::Client::new();
    let doc: serde_json::Value = client
        .get(format!("{base}/.well-known/awp.json"))
        .send()
        .await
        .expect("discovery request")
        .error_for_status()
        .expect("discovery status")
        .json()
        .await
        .expect("discovery json");
    assert!(doc.get("version").is_some(), "discovery missing version");

    let manifest: serde_json::Value = client
        .get(format!("{base}/awp/manifest"))
        .send()
        .await
        .expect("manifest request")
        .error_for_status()
        .expect("manifest status")
        .json()
        .await
        .expect("manifest json");
    let caps = manifest
        .get("capabilities")
        .and_then(|c| c.as_array())
        .expect("capabilities array");
    assert!(
        caps.iter()
            .any(|c| c.get("name").and_then(|n| n.as_str()) == Some("submit_intent")),
        "manifest missing submit_intent"
    );
}


/// Offline `AppState`: no API key, no MCP, in-memory sessions — every scenario streams its mock.
fn offline_app_state() -> spatial_os::state::AppState {
    use adk_awp::BusinessContextLoader;
    use std::sync::Arc;

    common::load_env();
    let config = AppConfig::from_env().expect("config");
    let path = common::manifest_dir().join("business.toml");
    let loader = BusinessContextLoader::from_file(&path).expect("business.toml");
    let awp = Arc::new(spatial_os::awp_gate::AwpGate::new(
        config.jwt_secret.clone(),
        loader.context_ref(),
    ));
    let runtime = spatial_os::state::RuntimeStatus {
        milestone: "M11",
        phase: "P2-S0",
        agents_enabled: config.agents_enabled(),
        postgres_enabled: config.postgres_enabled(),
        auth_enabled: config.auth_enabled(),
        voice_enabled: config.voice_enabled(),
        coordinator_enabled: config.agents_enabled(),
        uses_mock_orchestration: !config.agents_enabled(),
        scenarios: spatial_os::scenarios::ScenarioLiveFlags::default(),
        mcp_worksheet: false,
        mcp_docx: false,
        mcp_slides: false,
        mcp_news: false,
        allow_demo_mode: config.allow_demo_mode,
        public_domain: config.public_domain(),
        signup_endpoint: config.signup_endpoint.clone(),
        linkedin_partner_id: config.linkedin_partner_id.clone(),
        linkedin_conversion_id: config.linkedin_conversion_id,
    };
    spatial_os::state::AppState::new(
        runtime,
        spatial_os::state::SessionStore::new(),
        config.artifact_dir.clone(),
        spatial_os::scenarios::ScenarioLiveFlags::default(),
        false,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Arc::new({
            use adk_session::InMemorySessionService;
            InMemorySessionService::new()
        }),
        None,
        awp,
        Arc::new(adk_awp::InMemoryEventSubscriptionService::new()),
        None,
        None,
        None,
        None,
        None,
        None,
        spatial_os::ambient::AmbientStore::new(),
        false,
        None,
        "test".into(),
        None,
        spatial_os::voice::VoiceState::boot(&config),
    )
}

#[tokio::test]
async fn health_exposes_runtime_status() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    common::load_env();
    let state = offline_app_state();
    let app = axum::Router::new()
        .route("/health", axum::routing::get(spatial_os::routes::health::health))
        .with_state(state);
    let response = app
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["runtime"]["milestone"], "M11");
}

#[test]
fn business_toml_lists_voice_capabilities() {
    use adk_awp::BusinessContextLoader;

    let path = common::manifest_dir().join("business.toml");
    let loader = BusinessContextLoader::from_file(&path).expect("business.toml");
    let ctx = loader.load();
    let caps = &ctx.capabilities;
    let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"stream_voice"), "missing stream_voice capability");
    assert!(names.contains(&"voice_status"), "missing voice_status capability");

    let stream = caps.iter().find(|c| c.name == "stream_voice").expect("stream_voice");
    assert_eq!(stream.endpoint, "/ws/voice");
    let status = caps.iter().find(|c| c.name == "voice_status").expect("voice_status");
    assert_eq!(status.endpoint, "/api/voice/status");
}

#[test]
fn mcp_allowlist_catalog_covers_deck_agents() {
    let path = common::manifest_dir().join("mcp_allowlists.toml");
    let catalog = spatial_os::tools::allowlist::AllowlistCatalog::from_file(&path)
        .expect("mcp_allowlists.toml");

    for agent in [
        "excel_agent",
        "docs_agent",
        "slides_agent",
        "combine_agent",
        "brief_agent",
        "headlines_agent",
    ] {
        assert!(
            catalog.contains_agent(agent),
            "missing allowlist for {agent}"
        );
        assert!(
            !catalog.tools_for_agent(agent).is_empty(),
            "empty tools for {agent}"
        );
    }

    assert_eq!(catalog.tools_for_agent("excel_agent").len(), 10);
    assert!(catalog.tools_for_agent("brief_agent").contains(&"get_forecast".to_string()));
}

#[tokio::test]
async fn awp_gate_allows_anonymous_intent() {
    use axum::http::HeaderMap;

    let gate = awp_gate_fixture();
    let headers = HeaderMap::new();
    let trust = gate
        .check(&headers, "intent:sess-1", "submit_intent")
        .await
        .expect("anonymous intent should pass");
    assert_eq!(trust, awp_types::TrustLevel::Anonymous);
}

#[tokio::test]
async fn awp_gate_blocks_anonymous_action() {
    use axum::http::HeaderMap;

    let gate = awp_gate_fixture();
    let headers = HeaderMap::new();
    let err = gate
        .check(&headers, "action:sess-1", "submit_action")
        .await
        .expect_err("anonymous action should be forbidden");
    assert_eq!(err.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn awp_gate_jwt_bearer_unlocks_known_capabilities() {
    use axum::http::HeaderMap;
    use spatial_os::auth;

    let secret = "validate-jwt-secret";
    let gate = awp_gate_fixture();
    let token = auth::create_token(uuid::Uuid::new_v4(), secret).expect("jwt");

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );

    let trust = gate
        .check(&headers, "action:sess-1", "submit_action")
        .await
        .expect("known caller should pass submit_action");
    assert_eq!(trust, awp_types::TrustLevel::Known);
}

#[tokio::test]
async fn awp_gate_blocks_anonymous_subscribe() {
    use axum::http::HeaderMap;

    let gate = awp_gate_fixture();
    let headers = HeaderMap::new();
    let err = gate
        .check(&headers, "awp-subscribe", "subscribe_proactive")
        .await
        .expect_err("anonymous subscribe should be forbidden");
    assert_eq!(err.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn session_store_create_for_user() {
    let store = spatial_os::state::SessionStore::new();
    let record = store.create_for_user("user-abc".into()).await;
    assert_eq!(record.user_id, "user-abc");
    let loaded = store.get(&record.session_id).await.expect("session");
    assert_eq!(loaded.user_id, "user-abc");
}

#[tokio::test]
async fn postgres_schema_has_m9_tables() {
    let pool = common::postgres_pool().await;

    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT tablename::text FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename",
    )
    .fetch_all(&pool)
    .await
    .expect("list tables");

    let names: Vec<&str> = tables.iter().map(|(t,)| t.as_str()).collect();
    for expected in [
        "_sqlx_migrations",
        "agent_events",
        "agent_sessions",
        "ui_sessions",
        "users",
    ] {
        assert!(names.contains(&expected), "missing table {expected}, got {names:?}");
    }

    let migrations: Vec<(i64, String)> =
        sqlx::query_as("SELECT version, description FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("migrations");
    assert!(
        migrations.len() >= 3,
        "expected at least 3 migrations, got {migrations:?}"
    );
}

#[tokio::test]
async fn ui_session_persists_across_store_instances() {
    use spatial_os::state::{AgentRecord, SessionStore};

    let pool = common::postgres_pool().await;
    let store_a = SessionStore::with_postgres(pool.clone());
    assert!(store_a.postgres_enabled());

    let record = store_a.create_for_user("user-validate".into()).await;
    store_a
        .set_scenario(&record.session_id, "deck", Some("Build me a pitch deck"))
        .await;
    store_a
        .agent_active(
            &record.session_id,
            AgentRecord {
                id: "deck.agent".into(),
                title: "Deck".into(),
                glyph: "📊".into(),
                agent: "deck.agent".into(),
                rail: "active".into(),
                domain: Default::default(),
            },
        )
        .await;
    store_a
        .upsert_card(
            &record.session_id,
            0,
            serde_json::json!({"title": "Excel", "glyph": "📗"}),
            "resolved",
            Some(serde_json::json!({"big": "Done"})),
            false,
        )
        .await;

    let store_b = SessionStore::with_postgres(pool.clone());
    let loaded = store_b
        .get(&record.session_id)
        .await
        .expect("session should load from postgres via fresh store");
    assert_eq!(loaded.user_id, "user-validate");
    assert_eq!(loaded.scenario.as_deref(), Some("deck"));
    assert_eq!(loaded.origin_text.as_deref(), Some("Build me a pitch deck"));
    assert_eq!(loaded.agents_active.len(), 1);
    assert_eq!(loaded.agents_active[0].id, "deck.agent");
    assert_eq!(loaded.cards.len(), 1);
    assert_eq!(
        loaded.cards[0].card.get("title").and_then(|v| v.as_str()),
        Some("Excel")
    );

    let row: (String, serde_json::Value) = sqlx::query_as(
        "SELECT user_id, cards FROM ui_sessions WHERE session_id = $1",
    )
    .bind(&record.session_id)
    .fetch_one(&pool)
    .await
    .expect("ui_sessions row");
    assert_eq!(row.0, "user-validate");
    assert!(row.1.is_array());
}

#[tokio::test]
async fn artifact_paths_are_user_scoped() {
    use spatial_os::artifacts;

    let root = std::path::Path::new("/tmp/agentrix-artifacts");
    let user = "user-1";
    let session = "sess-1";
    let dir = artifacts::session_dir(root, user, session);
    assert!(dir.ends_with("user-1/sess-1"));
    let file = dir.join("pitch_deck.pptx");
    let url = artifacts::public_url(user, session, &file, root).expect("url");
    assert_eq!(url, "/artifacts/user-1/sess-1/pitch_deck.pptx");
    let outside = root.join("other/file");
    assert!(artifacts::public_url(user, session, &outside, root).is_none());
}

#[tokio::test]
async fn pg_agent_session_roundtrip() {
    use adk_session::{CreateRequest, GetRequest, SessionService};
    use spatial_os::pg_session::PgSessionService;

    let pool = common::postgres_pool().await;
    let svc = PgSessionService::new(pool);

    let session_id = uuid::Uuid::new_v4().to_string();
    let user_id = uuid::Uuid::new_v4().to_string();

    svc.create(CreateRequest {
        app_name: "agentrix-os-validate".into(),
        user_id: user_id.clone(),
        session_id: Some(session_id.clone()),
        state: Default::default(),
    })
    .await
    .expect("create agent session");

    let loaded = svc
        .get(GetRequest {
            app_name: "agentrix-os-validate".into(),
            user_id,
            session_id,
            num_recent_events: None,
            after: None,
        })
        .await
        .expect("get agent session");

    assert_eq!(loaded.app_name(), "agentrix-os-validate");
}

#[tokio::test]
async fn people_rail_unavailable_without_slack() {
    let rail = spatial_os::rails::people::fetch(None).await;
    assert_eq!(rail.source, "unavailable");
    assert!(rail.work.is_empty());
    assert!(rail.family.is_empty());
    assert!(rail.message.as_deref().unwrap_or("").contains("Slack"));
}

#[tokio::test]
async fn live_slides_from_mcp_news() {
    let paths = common::mcp_paths();
    common::assert_mcp_binaries_exist(&paths);

    let news = Arc::new(mcp::spawn_mcp_server(&paths.news).await.expect("news mcp"));
    let (source, slides) = spatial_os::rails::live::fetch_slides(news).await;
    assert!(
        source == "mcp-news" || source == "unavailable",
        "unexpected source: {source}"
    );
    if source == "mcp-news" {
        assert!(!slides.is_empty(), "expected headlines from hn_stories");
        assert!(!slides[0].body.is_empty());
    }
}

#[tokio::test]
async fn background_cards_reflect_empty_integrations() {
    use spatial_os::rails::background;

    let cards = background::build(&[], &[], &[], &[]);
    assert!(!cards.is_empty(), "should still emit flank card shells");
    let people = cards.iter().find(|c| c.kind == "people").expect("people card");
    assert_eq!(people.title, "People");
    assert!(people.rows.is_empty());
    let live = cards.iter().find(|c| c.kind == "live").expect("live card");
    assert_eq!(live.sub, "headlines");
}

#[tokio::test]
async fn ambient_store_tracks_agent_lifecycle() {
    use spatial_os::ambient::AmbientStore;

    let store = AmbientStore::new();
    store.set_working("research", "reading sources").await;
    let rec = store.get("research").await.expect("research agent");
    assert_eq!(rec.status, "working");
    assert_eq!(rec.task, "reading sources");

    store
        .set_done(
            "research",
            "ABC Corp brief",
            serde_json::json!({"big": "Brief ready", "sub": "done"}),
            Some("summary".into()),
        )
        .await;
    let rec = store.get("research").await.expect("research done");
    assert_eq!(rec.status, "done");
    assert!(rec.resolve.is_some());

    store.set_dnd(true).await;
    assert!(store.dnd().await);
}

#[tokio::test]
async fn proactive_mock_scenario_has_three_cards() {
    assert_eq!(mock::pick_scenario("Show me what you found"), "proactive");
    let types = mock::preview_intent_event_types("Show me what you found", 6).await;
    assert_eq!(types[0], "scenario");
    assert!(types.iter().filter(|t| *t == "card_spawn").count() >= 3);
}

#[tokio::test]
async fn ambient_agents_build_with_configured_model() {
    let api_key = common::google_api_key();
    let paths = common::mcp_paths();
    let news = Arc::new(mcp::spawn_mcp_server(&paths.news).await.expect("news"));

    spatial_os::agents::ambient::research::build(&api_key, &common::gemini_model(), news.clone())
        .await
        .expect("research agent");
    spatial_os::agents::ambient::scout::build(&api_key, &common::gemini_model(), None)
        .await
        .expect("scout agent");
    spatial_os::agents::ambient::maker::build(&api_key, &common::gemini_model())
        .await
        .expect("maker agent");
}

#[tokio::test]
async fn deck_agents_build_with_configured_model() {
    let api_key = common::google_api_key();
    let paths = common::mcp_paths();
    common::assert_mcp_binaries_exist(&paths);

    let pool = spatial_os::agents::deck::McpPool {
        worksheet: Arc::new(mcp::spawn_mcp_server(&paths.worksheet).await.unwrap()),
        docx: Arc::new(mcp::spawn_mcp_server(&paths.docx).await.unwrap()),
        slides: Arc::new(mcp::spawn_mcp_server(&paths.slides).await.unwrap()),
    };

    spatial_os::agents::deck::build_workflow(&api_key, &common::gemini_model(), &pool)
        .await
        .expect("deck workflow should build");
}

#[tokio::test]
#[ignore = "slow: full deck E2E (~2-5 min). Run: cargo test --test validate deck_workflow -- --ignored"]
async fn deck_workflow_writes_three_artifacts() {
    use adk_core::{SessionId, UserId};
    use adk_runner::Runner;
    use adk_session::{CreateRequest, InMemorySessionService, SessionService};

    let api_key = common::google_api_key();
    let paths = common::mcp_paths();
    common::assert_mcp_binaries_exist(&paths);

    let pool = spatial_os::agents::deck::McpPool {
        worksheet: Arc::new(mcp::spawn_mcp_server(&paths.worksheet).await.unwrap()),
        docx: Arc::new(mcp::spawn_mcp_server(&paths.docx).await.unwrap()),
        slides: Arc::new(mcp::spawn_mcp_server(&paths.slides).await.unwrap()),
    };

    let workflow =
        spatial_os::agents::deck::build_workflow(&api_key, &common::gemini_model(), &pool)
            .await
            .unwrap();

    let session_service = Arc::new(InMemorySessionService::new());
    let runner = Runner::builder()
        .app_name("agentrix-os-validate")
        .agent(workflow)
        .session_service(session_service.clone())
        .build()
        .unwrap();

    let session_id = uuid::Uuid::new_v4().to_string();
    let user_id = uuid::Uuid::new_v4().to_string();
    session_service
        .create(CreateRequest {
            app_name: "agentrix-os-validate".into(),
            user_id: user_id.clone(),
            session_id: Some(session_id.clone()),
            state: Default::default(),
        })
        .await
        .unwrap();

    let artifact_root = tempfile::tempdir().unwrap();
    let session_dir = artifact_root.path().join(&session_id);
    std::fs::create_dir_all(&session_dir).unwrap();

    let prompt = format!(
        "Build me a pitch deck\n\n[Save files to: {}/]\n[Sibling artifacts]\n(none yet)\n",
        session_dir.display()
    );

    let mut stream = runner
        .run(
            UserId::try_from(user_id.as_str()).unwrap(),
            SessionId::try_from(session_id.as_str()).unwrap(),
            Content::new("user").with_text(&prompt),
        )
        .await
        .unwrap();

    while let Some(ev) = stream.next().await {
        ev.expect("runner event");
    }

    for ext in ["xlsx", "docx", "pptx"] {
        let found = std::fs::read_dir(&session_dir)
            .unwrap()
            .flatten()
            .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some(ext));
        assert!(found, "expected .{ext} in {}", session_dir.display());
    }
}
// ---------------------------------------------------------------------------
// Phase 2 · S0 — domain model and agent contract
// ---------------------------------------------------------------------------

#[test]
fn domain_card_spawn_event_carries_domain() {
    use spatial_os::domain::Domain;
    use spatial_os::events::sse::FieldEvent;

    let card = serde_json::json!({"glyph":"✉️","title":"Needs you","agent":"inbox.agent"});
    let ev = FieldEvent::CardSpawn {
        index: 1,
        card: card.clone(),
        domain: Domain::for_card("morning", &card),
    };
    let json = serde_json::to_value(&ev).expect("serialize");
    assert_eq!(json["type"], "card_spawn");
    assert_eq!(json["domain"], "work");

    // Phase 1 JSON without a domain still deserializes (default = shared).
    let legacy = r#"{"index":0,"card":{},"status":"spawn","resolve":null,"pinned":false,"removed":false}"#;
    let rec: spatial_os::state::CardRecord = serde_json::from_str(legacy).expect("legacy card");
    assert_eq!(rec.domain, Domain::Shared);
}

#[tokio::test]
async fn domain_is_derived_when_cards_are_persisted() {
    use spatial_os::domain::Domain;
    let store = spatial_os::state::SessionStore::new();
    let rec = store.create_for_user("u-domain".into()).await;
    store.set_scenario(&rec.session_id, "deck", Some("Build me a pitch deck")).await;
    store
        .upsert_card(&rec.session_id, 0, serde_json::json!({"title":"Auto-Excel","agent":"auto-excel"}), "spawn", None, false)
        .await;
    store
        .upsert_card(&rec.session_id, 1, serde_json::json!({"title":"Family","agent":"x","domain":"home"}), "spawn", None, false)
        .await;
    let cards = store.list_cards(&rec.session_id).await.expect("cards");
    assert_eq!(cards[0].domain, Domain::Work);
    assert_eq!(cards[1].domain, Domain::Home);
}

#[test]
fn mcp_allowlist_specs_declare_world_and_mode() {
    use spatial_os::domain::Domain;
    use spatial_os::permissions::Mode;
    use spatial_os::tools::allowlist::AllowlistCatalog;

    let path = common::manifest_dir().join("mcp_allowlists.toml");
    let catalog = AllowlistCatalog::from_file(&path).expect("catalog");
    let inbox = catalog.spec_for("inbox_agent").expect("inbox spec");
    assert_eq!(inbox.world, Domain::Work);
    assert_eq!(inbox.mode, Mode::Suggest);
    assert!(inbox.tool_names().contains(&"create_draft".to_string()));

    let money = catalog.spec_for("money_agent").expect("money spec");
    assert_eq!(money.world, Domain::Home);
    assert_eq!(money.mode, Mode::Observe);

    // Unknown agents fall back to the safe defaults.
    assert_eq!(catalog.mode_for("nobody"), Mode::Suggest);
    assert_eq!(catalog.world_for("nobody"), Domain::Shared);
    // S0 only reports missing effects; S2 makes them a boot failure.
    catalog.validate_effects(false).expect("lenient validation");
    assert!(catalog.effects_for_unknown_tools().is_empty());
}

#[test]
fn mcp_allowlist_schema_is_backward_compatible() {
    use spatial_os::tools::allowlist::AllowlistCatalog;
    let legacy: Vec<spatial_os::tools::allowlist::AllowlistEntry> = toml::from_str::<toml::Value>(
        r#"
[[allowlist]]
agent = "legacy_agent"
mcp_server = "news"
tools = ["search_news"]
"#,
    )
    .expect("toml")
    .get("allowlist")
    .cloned()
    .expect("array")
    .try_into()
    .expect("entries");
    let catalog = AllowlistCatalog::from_entries(legacy);
    let spec = catalog.spec_for("legacy_agent").expect("spec");
    assert_eq!(spec.mode, spatial_os::permissions::Mode::Suggest);
    assert_eq!(spec.world, spatial_os::domain::Domain::Shared);
    assert_eq!(catalog.tools_missing_effects(), vec![("legacy_agent".to_string(), "search_news".to_string())]);
}


// ---------------------------------------------------------------------------
// Phase 2 · S1 — Mother Agent
// ---------------------------------------------------------------------------

fn ev_types(events: &[serde_json::Value]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| e.get("type").and_then(|t| t.as_str()).map(str::to_string))
        .collect()
}

#[tokio::test]
async fn mother_multi_target_merges_two_scenarios_offline() {
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-mother".into()).await;
    let response = dispatch_intent(IntentDispatch {
        state: &state,
        session_id: rec.session_id.clone(),
        user_id: rec.user_id.clone(),
        text: "What's happening with work?".into(),
    })
    .await;
    let events = collect_sse_events(response).await;
    let types = ev_types(&events);

    // One scenario header covering both delegated workflows (morning 3 + people 3) plus the
    // Work Mother's two labeled stubs (S4) — re-indexed into one field.
    assert_eq!(types.iter().filter(|t| *t == "scenario").count(), 1, "{types:?}");
    assert_eq!(events[0]["total_cards"], 8);
    let spawns: Vec<u64> = events
        .iter()
        .filter(|e| e["type"] == "card_spawn")
        .map(|e| e["index"].as_u64().unwrap())
        .collect();
    assert_eq!(spawns, (0..8).collect::<Vec<u64>>(), "cards must be re-indexed across targets");
    assert!(events.iter().filter(|e| e["type"] == "card_spawn").all(|e| e["domain"].is_string()));
    assert_eq!(types.iter().filter(|t| *t == "card_resolve").count(), 8);
    // Exactly one synthesis from the Mother, then done.
    assert_eq!(types.iter().filter(|t| *t == "suzy_summary").count(), 1);
    let summary = events.iter().find(|e| e["type"] == "suzy_summary").unwrap();
    assert_eq!(summary["key"], "mother");
    assert!(summary["html"].as_str().unwrap().contains("Work:"), "{}", summary["html"]);
    assert_eq!(types.last().map(String::as_str), Some("done"));
    // Suggested actions carry a mode badge.
    assert!(events.iter().any(|e| e["type"] == "suggest" && e["text"].as_str().unwrap().starts_with(|c: char| "👁💡⚡".contains(c))));

    // Merged cards were persisted under the primary scenario with re-indexed positions.
    let cards = state.sessions.list_cards(&rec.session_id).await.expect("cards");
    assert_eq!(cards.len(), 8);
    assert!(cards.iter().all(|c| c.resolve.is_some()));
    let stored = state.sessions.get(&rec.session_id).await.unwrap();
    assert_eq!(stored.scenario.as_deref(), Some("morning"));
    assert!(stored.chat_history.iter().any(|t| t.role == "mother"));
}

#[tokio::test]
async fn mother_single_target_passes_phase1_scenario_through() {
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-deck".into()).await;
    let response = dispatch_intent(IntentDispatch {
        state: &state,
        session_id: rec.session_id.clone(),
        user_id: rec.user_id.clone(),
        text: "Build me a pitch deck".into(),
    })
    .await;
    let events = collect_sse_events(response).await;
    assert_eq!(events[0]["type"], "scenario");
    assert_eq!(events[0]["key"], "deck");
    let cards = state.sessions.list_cards(&rec.session_id).await.expect("cards");
    assert!(cards.len() >= 3, "deck scenario persists its cards");
    assert_eq!(events[0]["total_cards"].as_u64().unwrap() as usize, cards.len());
    assert!(cards.iter().all(|c| c.domain == spatial_os::domain::Domain::Work));
}

#[tokio::test]
async fn mother_clarifies_ambiguous_intent() {
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-hmm".into()).await;
    let response = dispatch_intent(IntentDispatch {
        state: &state,
        session_id: rec.session_id.clone(),
        user_id: rec.user_id.clone(),
        text: "hmm".into(),
    })
    .await;
    let events = collect_sse_events(response).await;
    let types = ev_types(&events);
    assert_eq!(types[0], "suzy_summary");
    assert_eq!(events[0]["key"], "clarify");
    assert!(types.iter().filter(|t| *t == "suggest").count() >= 3);
    assert!(!types.contains(&"card_spawn".to_string()));
}

#[test]
fn mother_merge_reindexes_and_drops_inner_wrappers() {
    use spatial_os::domain::Domain;
    use spatial_os::mother::delegate::merge_target_events;
    use spatial_os::mother::intake::Target;

    let targets = vec![
        Target { world: Domain::Work, agent: "productivity".into(), task: "t".into(), scenario: "morning".into() },
        Target { world: Domain::Home, agent: "family".into(), task: "t".into(), scenario: "people".into() },
    ];
    let a = vec![
        serde_json::json!({"type":"scenario","key":"morning","text":"x","total_cards":2}),
        serde_json::json!({"type":"card_spawn","index":0,"card":{"title":"Today","agent":"calendar.agent"},"domain":"work"}),
        serde_json::json!({"type":"card_spawn","index":1,"card":{"title":"Needs you","agent":"inbox.agent"},"domain":"work"}),
        serde_json::json!({"type":"card_resolve","index":1,"resolve":{"big":"2 to reply","actions":["Draft replies"]}}),
        serde_json::json!({"type":"suzy_summary","key":"morning","html":"inner"}),
        serde_json::json!({"type":"done"}),
    ];
    let b = vec![
        serde_json::json!({"type":"scenario","key":"people","text":"x","total_cards":1}),
        serde_json::json!({"type":"card_spawn","index":0,"card":{"title":"Connections","agent":"crm.agent"}}),
        serde_json::json!({"type":"card_status","index":0,"status":"working","line":"Finding…"}),
        serde_json::json!({"type":"card_resolve","index":0,"resolve":{"lines":["Birthday: Mara"],"actions":["Send notes"]}}),
        serde_json::json!({"type":"done"}),
    ];
    let merged = merge_target_events(&targets, vec![(a, false), (b, true)]);
    assert_eq!(merged.total_cards, 3);
    let json: Vec<serde_json::Value> = merged.events.iter().map(|e| serde_json::to_value(e).unwrap()).collect();
    let types = ev_types(&json);
    assert!(!types.iter().any(|t| t == "scenario" || t == "suzy_summary" || t == "done"));
    let third_spawn = json.iter().find(|e| e["type"] == "card_spawn" && e["index"] == 2).expect("re-indexed spawn");
    assert_eq!(third_spawn["card"]["title"], "Connections");
    // The people-scenario card inherits the Home target's world when the card itself is unspecific.
    assert_eq!(third_spawn["domain"], "home");
    assert!(json.iter().any(|e| e["type"] == "card_status" && e["index"] == 2));
    assert_eq!(merged.results.len(), 2);
    assert!(merged.results[1].timed_out);
    assert_eq!(merged.results[0].cards[1].resolve.as_ref().unwrap()["big"], "2 to reply");
    assert_eq!(merged.card_at(2).unwrap()["title"], "Connections");
}

#[tokio::test]
async fn mother_chat_route_streams_and_records_history() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;
    use tower::ServiceExt;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-chat".into()).await;
    let app = axum::Router::new()
        .route(
            "/api/sessions/{session_id}/chat",
            axum::routing::post(spatial_os::routes::chat::chat).get(spatial_os::routes::chat::history),
        )
        .with_state(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/api/sessions/{}/chat", rec.session_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"text":"Prepare me for my afternoon."}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let events = collect_sse_events(response).await;
    assert_eq!(events[0]["type"], "scenario");
    assert_eq!(events[0]["total_cards"], 8);

    let response = app
        .oneshot(Request::get(format!("/api/sessions/{}/chat", rec.session_id)).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let turns = json["turns"].as_array().unwrap();
    assert_eq!(turns[0]["role"], "user");
    assert_eq!(turns[0]["text"], "Prepare me for my afternoon.");
    assert_eq!(turns.last().unwrap()["role"], "mother");
}

#[test]
fn business_toml_lists_chat_mother_capability() {
    use adk_awp::BusinessContextLoader;
    let path = common::manifest_dir().join("business.toml");
    let ctx = BusinessContextLoader::from_file(&path).expect("business.toml").load();
    assert!(ctx.capabilities.iter().any(|c| c.name == "chat_mother" && c.endpoint.contains("/chat")));
}

// ---------------------------------------------------------------------------
// Phase 2 · S2 — activity ledger, permission gate, pending actions, audit
// ---------------------------------------------------------------------------

/// A fake toolset with one tool per effect class the tests need. Tool names are chosen so
/// the allowlist catalog classifies them for `inbox_agent` (`list_inbox` read, `create_draft`
/// write_local) or falls back to the most restrictive effect (`send_anything` → send_external).
struct FakeInboxTools;

#[async_trait::async_trait]
impl adk_core::Toolset for FakeInboxTools {
    fn name(&self) -> &str {
        "fake-inbox"
    }
    async fn tools(
        &self,
        _ctx: std::sync::Arc<dyn adk_core::ReadonlyContext>,
    ) -> adk_core::Result<Vec<std::sync::Arc<dyn adk_core::Tool>>> {
        use adk_tool::FunctionTool;
        let list = FunctionTool::new("list_inbox", "list", |_c, _a| async { Ok(serde_json::json!({"output": "[]"})) });
        let draft = FunctionTool::new("create_draft", "draft", |_c, a| async move { Ok(serde_json::json!({"output": "draft ok", "echo": a})) });
        let send = FunctionTool::new("send_anything", "send", |_c, _a| async { Ok(serde_json::json!({"output": "SENT"})) });
        Ok(vec![Arc::new(list), Arc::new(draft), Arc::new(send)])
    }
}

/// The gate tests share the process-wide permission store for the `anonymous` tool-context
/// user, so they run one at a time.
fn gate_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn gated_tools(agent: &str) -> std::collections::HashMap<String, Arc<dyn adk_core::Tool>> {
    let gate = spatial_os::permissions::PermissionGate::wrap(agent, Arc::new(FakeInboxTools));
    let ctx: Arc<dyn adk_core::ReadonlyContext> = Arc::new(adk_tool::SimpleToolContext::new("test"));
    gate.tools(ctx).await.unwrap().into_iter().map(|t| (t.name().to_string(), t)).collect()
}

fn tool_ctx(session: &str) -> Arc<dyn adk_core::ToolContext> {
    Arc::new(adk_tool::SimpleToolContext::new("gate-test").with_session_id(session))
}

#[test]
fn effects_cover_every_allowlisted_tool() {
    use spatial_os::permissions::Effect;
    use spatial_os::tools::allowlist::AllowlistCatalog;
    let path = common::manifest_dir().join("mcp_allowlists.toml");
    let catalog = AllowlistCatalog::from_file(&path).expect("catalog");
    let missing = catalog.validate_effects(true).expect("every tool classified");
    assert!(missing.is_empty());
    assert_eq!(catalog.effect_for("inbox_agent", "list_inbox"), Some(Effect::Read));
    assert_eq!(catalog.effect_for("inbox_agent", "create_draft"), Some(Effect::WriteLocal));
    assert_eq!(catalog.effect_for("excel_agent", "write_cells"), Some(Effect::WriteLocal));
    // Unclassified / unknown tools are treated as the most restrictive non-financial effect.
    assert_eq!(spatial_os::tools::allowlist::effect_for("inbox_agent", "send_anything"), Effect::SendExternal);
    assert!(spatial_os::tools::allowlist::EFFECTS_REQUIRED);
}

#[tokio::test]
async fn gate_observe_allows_reads_and_denies_writes() {
    let _serial = gate_lock().lock().await;
    use spatial_os::permissions::{gate, Mode};
    let svc = gate::services();
    let user = adk_tool::SimpleToolContext::new("x").user_id().to_string();
    svc.permissions.set_mode(&user, "inbox_agent", Mode::Observe).await;
    let tools = gated_tools("inbox_agent").await;

    let read = tools["list_inbox"].execute(tool_ctx("s-observe"), serde_json::json!({})).await.unwrap();
    assert_eq!(read["output"], "[]");
    let write = tools["create_draft"].execute(tool_ctx("s-observe"), serde_json::json!({"to": "a@b"})).await.unwrap();
    assert_eq!(write["status"], "denied");
    assert!(write["message"].as_str().unwrap().contains("observe mode"));
    assert!(tools["list_inbox"].is_read_only());
    assert!(!tools["create_draft"].is_read_only());

    let denied = svc.audit.list(&user, 50).await.into_iter().filter(|e| e.decision == "denied" && e.tool == "create_draft").count();
    assert!(denied >= 1);
    svc.permissions.set_mode(&user, "inbox_agent", Mode::Suggest).await;
}

#[tokio::test]
async fn gate_suggest_runs_local_writes_and_queues_external_sends() {
    let _serial = gate_lock().lock().await;
    use spatial_os::permissions::{gate, Mode, PendingStatus};
    let svc = gate::services();
    let user = adk_tool::SimpleToolContext::new("x").user_id().to_string();
    svc.permissions.set_mode(&user, "inbox_agent", Mode::Suggest).await;
    let tools = gated_tools("inbox_agent").await;

    let draft = tools["create_draft"].execute(tool_ctx("s-suggest"), serde_json::json!({"to": "a@b", "body": "hi"})).await.unwrap();
    assert_eq!(draft["output"], "draft ok", "write_local runs immediately in suggest mode");

    let send = tools["send_anything"].execute(tool_ctx("s-suggest"), serde_json::json!({"thread_id": "t-1", "body": "secret"})).await.unwrap();
    assert_eq!(send["status"], "queued_for_approval");
    let id: uuid::Uuid = send["action_id"].as_str().unwrap().parse().unwrap();
    let pending = svc.pending.get(id).await.expect("pending action");
    assert_eq!(pending.status, PendingStatus::Pending);
    assert_eq!(pending.session_id.as_deref(), Some("s-suggest"));
    assert!(pending.summary.contains("send_anything"));
    assert!(pending.summary.contains("thread_id"), "summary lists argument keys");
    assert!(!pending.summary.contains("secret"), "summary never carries argument values");

    // The ledger saw the call, content-free, with the subject hashed.
    svc.ledger.flush().await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let calls = svc
        .ledger
        .query(&spatial_os::intelligence::LedgerQuery { user_id: user.clone(), kind: Some("tool_call".into()), agent_id: Some("inbox_agent".into()), ..Default::default() })
        .await;
    let queued = calls.iter().find(|e| e.meta["decision"] == "queued").expect("queued tool_call in ledger");
    assert_eq!(queued.subject_hash.as_ref().map(String::len), Some(24));
    assert!(serde_json::to_string(&queued.meta).unwrap().contains("send_external"));
    assert!(!serde_json::to_string(queued).unwrap().contains("secret"));

    // Approval executes the un-gated tool and audits it.
    let state = offline_app_state();
    let resolved = spatial_os::routes::actions::approve_one(&state, &user, pending).await;
    assert_eq!(resolved.status, PendingStatus::Approved);
    assert_eq!(resolved.result.as_ref().unwrap()["output"], "SENT");
    let approved = svc.audit.list(&user, 50).await.into_iter().filter(|e| e.decision == "approved" && e.approval_id == Some(id)).count();
    assert_eq!(approved, 1);
}

#[tokio::test]
async fn gate_pause_blocks_writes_and_resume_restores() {
    let _serial = gate_lock().lock().await;
    use spatial_os::permissions::{gate, Mode, PauseScope};
    use spatial_os::domain::Domain;
    let svc = gate::services();
    let user = adk_tool::SimpleToolContext::new("x").user_id().to_string();
    svc.permissions.set_mode(&user, "inbox_agent", Mode::Suggest).await;
    let tools = gated_tools("inbox_agent").await;

    svc.permissions.pause(PauseScope::Work, None).await;
    assert!(svc.permissions.is_paused(Domain::Work).await);
    assert!(!svc.permissions.is_paused(Domain::Home).await);
    let paused = tools["create_draft"].execute(tool_ctx("s-pause"), serde_json::json!({})).await.unwrap();
    assert_eq!(paused["status"], "paused");
    let read = tools["list_inbox"].execute(tool_ctx("s-pause"), serde_json::json!({})).await.unwrap();
    assert_eq!(read["output"], "[]", "reads keep working while paused");
    svc.permissions.resume(PauseScope::Work).await;
    let ok = tools["create_draft"].execute(tool_ctx("s-pause"), serde_json::json!({})).await.unwrap();
    assert_eq!(ok["output"], "draft ok");
}

#[tokio::test]
async fn permission_store_resolves_override_then_user_then_default() {
    use spatial_os::permissions::{Mode, PermissionStore};
    let store = PermissionStore::in_memory();
    assert_eq!(store.mode_for("u", "money_agent", None).await, Mode::Observe, "catalog default");
    assert_eq!(store.mode_for("u", "inbox_agent", None).await, Mode::Suggest);
    store.set_mode("u", "inbox_agent", Mode::Observe).await;
    assert_eq!(store.mode_for("u", "inbox_agent", Some("create_draft")).await, Mode::Observe);
    store.set_tool_override("u", "inbox_agent", "create_draft", Some(Mode::Suggest)).await;
    assert_eq!(store.mode_for("u", "inbox_agent", Some("create_draft")).await, Mode::Suggest);
    assert_eq!(store.mode_for("u", "inbox_agent", Some("list_inbox")).await, Mode::Observe);
    store.set_tool_override("u", "inbox_agent", "create_draft", None).await;
    assert_eq!(store.mode_for("u", "inbox_agent", Some("create_draft")).await, Mode::Observe);
    let view = store.list("u").await;
    let inbox = view.iter().find(|a| a.agent_id == "inbox_agent").unwrap();
    assert_eq!(inbox.mode, Mode::Observe);
    assert_eq!(inbox.default_mode, Mode::Suggest);
    assert!(inbox.tools.iter().any(|t| t.name == "create_draft" && t.effect == "write_local"));
}

#[tokio::test]
async fn pending_actions_batch_approve_and_reject_via_routes() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use spatial_os::domain::Domain;
    use spatial_os::permissions::{Effect, PendingStatus};
    use tower::ServiceExt;

    let (state, headers) = known_app_state();
    let rec = state.sessions.create_for_user("u-actions".into()).await;
    let a = state.pending.create(&rec.user_id, Some(&rec.session_id), "inbox_agent", Domain::Work, "send_anything", Effect::SendExternal, serde_json::json!({}), None).await;
    let b = state.pending.create(&rec.user_id, Some(&rec.session_id), "inbox_agent", Domain::Work, "send_anything", Effect::SendExternal, serde_json::json!({}), None).await;
    let c = state.pending.create(&rec.user_id, Some(&rec.session_id), "calendar_agent", Domain::Work, "create_event", Effect::ScheduleWithOthers, serde_json::json!({}), None).await;

    let app = axum::Router::new()
        .route("/api/actions", axum::routing::get(spatial_os::routes::actions::list))
        .route("/api/actions/approve", axum::routing::post(spatial_os::routes::actions::approve_batch))
        .route("/api/actions/{id}/reject", axum::routing::post(spatial_os::routes::actions::reject))
        .with_state(state.clone());

    // Anonymous callers hit the AWP trust gate.
    let anon = app.clone().oneshot(Request::get(format!("/api/actions?session_id={}", rec.session_id)).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(anon.status(), StatusCode::FORBIDDEN);

    let mut req = Request::get(format!("/api/actions?session_id={}", rec.session_id)).body(Body::empty()).unwrap();
    req.headers_mut().extend(headers.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(json["actions"].as_array().unwrap().len(), 3);

    let mut req = Request::post("/api/actions/approve")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"session_id": rec.session_id, "ids": [a.id, b.id]}).to_string()))
        .unwrap();
    req.headers_mut().extend(headers.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // Fake tools were registered by the gate tests under inbox_agent, so approval executes (or
    // fails honestly when the registry has no such tool) — either way the action is resolved.
    assert!(matches!(state.pending.get(a.id).await.unwrap().status, PendingStatus::Approved | PendingStatus::Failed));
    assert!(matches!(state.pending.get(b.id).await.unwrap().status, PendingStatus::Approved | PendingStatus::Failed));

    let mut req = Request::post(format!("/api/actions/{}/reject", c.id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"session_id": rec.session_id}).to_string()))
        .unwrap();
    req.headers_mut().extend(headers.clone());
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(state.pending.get(c.id).await.unwrap().status, PendingStatus::Rejected);
    assert!(state.audit.count(&rec.user_id, Some("rejected")).await >= 1);
}

#[tokio::test]
async fn permission_routes_set_mode_and_pause() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use spatial_os::domain::Domain;
    use tower::ServiceExt;

    let (state, headers) = known_app_state();
    let rec = state.sessions.create_for_user("u-perms".into()).await;
    let app = axum::Router::new()
        .route("/api/permissions", axum::routing::get(spatial_os::routes::permissions::get).put(spatial_os::routes::permissions::put))
        .route("/api/pause", axum::routing::post(spatial_os::routes::permissions::pause))
        .route("/api/resume", axum::routing::post(spatial_os::routes::permissions::resume))
        .with_state(state.clone());

    let mut req = Request::put("/api/permissions")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"session_id": rec.session_id, "agent_id": "money_agent", "mode": "automate"}).to_string()))
        .unwrap();
    req.headers_mut().extend(headers.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(json["agent"]["mode"], "automate");
    assert_eq!(json["agent"]["default_mode"], "observe");

    let mut req = Request::post("/api/pause")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"session_id": rec.session_id, "scope": "home", "minutes": 30}).to_string()))
        .unwrap();
    req.headers_mut().extend(headers.clone());
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(state.permissions.is_paused(Domain::Home).await);
    assert!(!state.permissions.is_paused(Domain::Work).await);

    let mut req = Request::post("/api/resume")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({"session_id": rec.session_id, "scope": "home"}).to_string()))
        .unwrap();
    req.headers_mut().extend(headers.clone());
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(!state.permissions.is_paused(Domain::Home).await);
}

#[tokio::test]
async fn ledger_records_mother_intents_and_ui_events() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;
    use tower::ServiceExt;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-ledger".into()).await;
    let response = dispatch_intent(IntentDispatch { state: &state, session_id: rec.session_id.clone(), user_id: rec.user_id.clone(), text: "Build me a pitch deck".into() }).await;
    let _ = collect_sse_events(response).await;

    let app = axum::Router::new()
        .route("/api/sessions/{session_id}/events", axum::routing::post(spatial_os::routes::events::record))
        .with_state(state.clone());
    let res = app
        .oneshot(
            Request::post(format!("/api/sessions/{}/events", rec.session_id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":[{"kind":"ui_notification","count":4,"domain":"work"},{"kind":"ui_secret","count":1}]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);

    state.ledger.flush().await;
    let intents = state.ledger.query(&spatial_os::intelligence::LedgerQuery { user_id: rec.user_id.clone(), kind: Some("intent".into()), ..Default::default() }).await;
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].meta["scenario"], "deck");
    assert_eq!(intents[0].meta["entry"], "intent");
    let ui = state.ledger.query(&spatial_os::intelligence::LedgerQuery { user_id: rec.user_id.clone(), agent_id: Some("ui".into()), ..Default::default() }).await;
    assert_eq!(ui.len(), 1, "unknown UI kinds are ignored");
    assert_eq!(ui[0].kind, "ui_notification");
    assert_eq!(ui[0].meta["count"], 4);
}

#[tokio::test]
async fn commit_creates_an_approved_action_with_audit() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let (state, headers) = known_app_state();
    let rec = state.sessions.create_for_user("u-commit".into()).await;
    state.sessions.set_scenario(&rec.session_id, "morning", Some("Start my day")).await;
    state.sessions.upsert_card(&rec.session_id, 1, serde_json::json!({"title":"Needs you","agent":"inbox.agent"}), "resolved", None, false).await;
    let app = axum::Router::new()
        .route("/api/sessions/{session_id}/commit", axum::routing::post(spatial_os::routes::commit::commit_action))
        .with_state(state.clone());
    let mut req = Request::post(format!("/api/sessions/{}/commit", rec.session_id))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"card_title":"Needs you","action_label":"Draft replies"}"#))
        .unwrap();
    req.headers_mut().extend(headers);
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    let id: uuid::Uuid = json["action_id"].as_str().unwrap().parse().unwrap();
    let action = state.pending.get(id).await.unwrap();
    assert_eq!(action.status, spatial_os::permissions::PendingStatus::Approved);
    assert_eq!(action.agent_id, "inbox_agent");
    assert_eq!(action.effect, spatial_os::permissions::Effect::WriteLocal);
    assert!(state.audit.list(&rec.user_id, 10).await.iter().any(|e| e.approval_id == Some(id) && e.decision == "approved"));
}

#[test]
fn sse_permission_request_event_shape() {
    use spatial_os::domain::Domain;
    use spatial_os::events::sse::FieldEvent;
    let ev = FieldEvent::PermissionRequest {
        action_id: "a1".into(),
        agent_id: "inbox_agent".into(),
        domain: Domain::Work,
        effect: "send_external".into(),
        summary: "inbox_agent · send_draft (send_external) with draft_id".into(),
        expires_at: "2026-10-01T00:00:00Z".into(),
    };
    let json = serde_json::to_value(&ev).unwrap();
    assert_eq!(json["type"], "permission_request");
    assert_eq!(json["domain"], "work");
}

/// `AppState` whose AWP gate verifies JWTs, plus a `Bearer` header for a known user.
fn known_app_state() -> (spatial_os::state::AppState, axum::http::HeaderMap) {
    use adk_awp::BusinessContextLoader;
    let mut state = offline_app_state();
    let path = common::manifest_dir().join("business.toml");
    let loader = BusinessContextLoader::from_file(&path).expect("business.toml");
    state.awp = Arc::new(spatial_os::awp_gate::AwpGate::new(Some("test-secret".into()), loader.context_ref()));
    let token = spatial_os::auth::create_token(uuid::Uuid::new_v4(), "test-secret").expect("jwt");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    (state, headers)
}

// ---------------------------------------------------------------------------
// Phase 2 · S3 — personal memory
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_propose_confirm_correct_forget_export() {
    use spatial_os::domain::Domain;
    use spatial_os::memory::{Kind, MemoryService, NewItem, Provenance, Scope, Sensitivity};
    let m = MemoryService::in_memory();

    let proposed = m
        .propose("u", 0.8, NewItem {
            domain: Domain::Work,
            category: "routine",
            key: "routine.work.start",
            value: serde_json::json!("09:40"),
            sensitivity: Sensitivity::Normal,
            source_agent: "patterns",
            provenance: Provenance::new("pattern").agent("patterns").note("28-day median"),
        })
        .await;
    assert_eq!(proposed.kind, Kind::Assumed);
    assert_eq!(proposed.confidence, Some(0.8));
    assert_eq!(proposed.note(), "routine.work.start: 09:40 (I think)");

    // The user's statement wins and a later proposal cannot override it.
    let known = m
        .remember("u", NewItem {
            domain: Domain::Work,
            category: "preference",
            key: "preference.meetings.earliest_start",
            value: serde_json::json!("10:00"),
            sensitivity: Sensitivity::Normal,
            source_agent: "mother",
            provenance: Provenance::new("user_statement"),
        })
        .await;
    assert_eq!(known.kind, Kind::Known);
    let again = m
        .propose("u", 0.9, NewItem {
            domain: Domain::Work,
            category: "preference",
            key: "preference.meetings.earliest_start",
            value: serde_json::json!("09:00"),
            sensitivity: Sensitivity::Normal,
            source_agent: "calendar_agent",
            provenance: Provenance::new("agent_proposal"),
        })
        .await;
    assert_eq!(again.kind, Kind::Known);
    assert_eq!(again.value, serde_json::json!("10:00"));

    // Confirm promotes with provenance; correct changes the value and promotes.
    let confirmed = m.confirm("u", proposed.id, Some("s1")).await.unwrap();
    assert_eq!(confirmed.kind, Kind::Known);
    assert_eq!(confirmed.provenance.last().unwrap().kind, "user_confirmation");
    assert_eq!(confirmed.provenance.len(), 2);
    let corrected = m.correct("u", proposed.id, serde_json::json!("09:30"), None).await.unwrap();
    assert_eq!(corrected.value, serde_json::json!("09:30"));
    assert_eq!(corrected.provenance.last().unwrap().kind, "user_correction");

    // Export carries provenance; forget soft-deletes; purge removes everything.
    let export = m.export("u").await;
    assert_eq!(export.len(), 2);
    assert!(export.iter().all(|i| !i.provenance.is_empty()));
    m.forget("u", known.id).await.unwrap();
    assert_eq!(m.export("u").await.len(), 1);
    assert_eq!(m.purge("u").await, 2);
    assert!(m.export("u").await.is_empty());
    let _ = Scope::MOTHER;
}

#[tokio::test]
async fn memory_scopes_isolate_worlds_for_agent_tools() {
    use spatial_os::domain::Domain;
    use spatial_os::memory::{service_handle, NewItem, Provenance, Sensitivity};
    let m = service_handle();
    let user = adk_tool::SimpleToolContext::new("x").user_id().to_string();
    m.remember(&user, NewItem { domain: Domain::Home, category: "date", key: "date.birthday.sara", value: serde_json::json!("20 Oct"), sensitivity: Sensitivity::Normal, source_agent: "user", provenance: Provenance::new("user_statement") }).await;
    m.remember(&user, NewItem { domain: Domain::Shared, category: "profile", key: "profile.timezone", value: serde_json::json!("Africa/Nairobi"), sensitivity: Sensitivity::Normal, source_agent: "user", provenance: Provenance::new("user_statement") }).await;

    let ctx: Arc<dyn adk_core::ToolContext> = Arc::new(adk_tool::SimpleToolContext::new("scope-test"));
    // inbox_agent lives in Work: sees shared, not home.
    let work_read = spatial_os::memory::tools::read_memory_tool("inbox_agent");
    let out = work_read.execute(ctx.clone(), serde_json::json!({})).await.unwrap();
    let keys: Vec<&str> = out["items"].as_array().unwrap().iter().map(|i| i["key"].as_str().unwrap()).collect();
    assert!(keys.contains(&"profile.timezone"));
    assert!(!keys.contains(&"date.birthday.sara"), "work agent must not read home memory");
    // The Mother reads across.
    let mother_read = spatial_os::memory::tools::read_memory_tool("mother");
    let out = mother_read.execute(ctx.clone(), serde_json::json!({"keys": ["date."]})).await.unwrap();
    assert_eq!(out["items"].as_array().unwrap().len(), 1);
    // A work agent cannot propose into the home domain; its own domain is fine and lands as assumed.
    let propose = spatial_os::memory::tools::propose_memory_tool("inbox_agent");
    let rejected = propose.execute(ctx.clone(), serde_json::json!({"key": "x", "value": 1, "domain": "home"})).await.unwrap();
    assert_eq!(rejected["status"], "rejected");
    let ok = propose.execute(ctx, serde_json::json!({"key": "preference.email.tone", "value": "brief", "confidence": 0.7})).await.unwrap();
    assert_eq!(ok["status"], "proposed");
    assert_eq!(ok["kind"], "assumed");
}

#[tokio::test]
async fn memory_routes_remember_confirm_export_purge() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let (state, headers) = known_app_state();
    let rec = state.sessions.create_for_user("u-memory-routes".into()).await;
    let app = axum::Router::new()
        .route("/api/memory", axum::routing::get(spatial_os::routes::memory::list).post(spatial_os::routes::memory::remember).delete(spatial_os::routes::memory::purge))
        .route("/api/memory/export", axum::routing::get(spatial_os::routes::memory::export))
        .route("/api/memory/{id}", axum::routing::patch(spatial_os::routes::memory::patch).delete(spatial_os::routes::memory::forget))
        .with_state(state.clone());
    let send = |req: Request<Body>| {
        let app = app.clone();
        let headers = headers.clone();
        async move {
            let mut req = req;
            req.headers_mut().extend(headers);
            app.oneshot(req).await.unwrap()
        }
    };
    let json = |body: serde_json::Value| Body::from(body.to_string());

    // Anonymous is refused.
    let anon = app.clone().oneshot(Request::get(format!("/api/memory?session_id={}", rec.session_id)).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(anon.status(), StatusCode::FORBIDDEN);

    let res = send(Request::post("/api/memory").header("content-type", "application/json").body(json(serde_json::json!({"session_id": rec.session_id, "domain": "shared", "category": "profile", "key": "profile.home_location", "value": "Nairobi"}))).unwrap()).await;
    assert_eq!(res.status(), StatusCode::OK);
    let item: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(item["kind"], "known");

    // An assumed item shows up with kind and can be confirmed through PATCH.
    let assumed = state.memory.propose(&rec.user_id, 0.6, spatial_os::memory::NewItem { domain: spatial_os::domain::Domain::Work, category: "routine", key: "routine.work.end", value: serde_json::json!("17:30"), sensitivity: spatial_os::memory::Sensitivity::Normal, source_agent: "patterns", provenance: spatial_os::memory::Provenance::new("pattern") }).await;
    let res = send(Request::get(format!("/api/memory?session_id={}&kind=assumed", rec.session_id)).body(Body::empty()).unwrap()).await;
    let list: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let res = send(Request::patch(format!("/api/memory/{}", assumed.id)).header("content-type", "application/json").body(json(serde_json::json!({"session_id": rec.session_id, "op": "confirm"}))).unwrap()).await;
    assert_eq!(res.status(), StatusCode::OK);
    let confirmed: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(confirmed["kind"], "known");
    assert_eq!(confirmed["provenance"].as_array().unwrap().last().unwrap()["kind"], "user_confirmation");

    let res = send(Request::get(format!("/api/memory/export?session_id={}", rec.session_id)).body(Body::empty()).unwrap()).await;
    let export: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(export["format"], "agentrix-memory-export/v1");
    assert_eq!(export["items"].as_array().unwrap().len(), 2);

    // The greeting route uses the remembered home location.
    assert_eq!(state.memory.profile(&rec.user_id, "home_location").await.as_deref(), Some("Nairobi"));

    let res = send(Request::delete(format!("/api/memory?session_id={}", rec.session_id)).body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(state.memory.export(&rec.user_id).await.is_empty());
}

#[tokio::test]
async fn memory_chat_statements_are_handled_by_the_mother() {
    use spatial_os::memory::Kind;
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-memory-chat".into()).await;
    let run = |text: &str| {
        let state = state.clone();
        let sid = rec.session_id.clone();
        let uid = rec.user_id.clone();
        let text = text.to_string();
        async move {
            let r = dispatch_intent(IntentDispatch { state: &state, session_id: sid, user_id: uid, text }).await;
            collect_sse_events(r).await
        }
    };

    let events = run("Remember I never take meetings before 10").await;
    assert_eq!(events[0]["type"], "suzy_summary");
    assert_eq!(events[0]["key"], "mother");
    assert!(events[0]["html"].as_str().unwrap().contains("I'll remember"));
    assert_eq!(events.len(), 2, "a memory statement is a one-shot reply, no cards");
    let items = state.memory.export(&rec.user_id).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, Kind::Known);
    assert_eq!(items[0].key, "preference.meetings.earliest_start");
    assert_eq!(items[0].provenance[0].kind, "user_statement");

    // Synthesis cites the kind when it composes a multi-target answer.
    let events = run("What's happening with work?").await;
    let summary = events.iter().find(|e| e["type"] == "suzy_summary").unwrap();
    assert!(summary["html"].as_str().unwrap().contains("(you told me)"), "{}", summary["html"]);

    let events = run("why do you think that").await;
    assert!(events[0]["html"].as_str().unwrap().contains("not assuming anything"));

    let events = run("forget meetings").await;
    assert!(events[0]["html"].as_str().unwrap().contains("Forgotten"));
    assert!(state.memory.export(&rec.user_id).await.is_empty());

    state.ledger.flush().await;
    let stmts = state.ledger.query(&spatial_os::intelligence::LedgerQuery { user_id: rec.user_id.clone(), kind: Some("memory_statement".into()), ..Default::default() }).await;
    assert_eq!(stmts.len(), 3);
    assert!(stmts.iter().all(|e| !serde_json::to_string(e).unwrap().contains("meetings")), "the ledger never carries the statement text");
}

#[test]
fn business_toml_lists_r1_capabilities() {
    use adk_awp::BusinessContextLoader;
    let path = common::manifest_dir().join("business.toml");
    let ctx = BusinessContextLoader::from_file(&path).expect("business.toml").load();
    let names: Vec<&str> = ctx.capabilities.iter().map(|c| c.name.as_str()).collect();
    for cap in ["chat_mother", "list_actions", "approve_action", "get_permissions", "set_permissions", "pause_agents", "record_ui_events", "manage_memory"] {
        assert!(names.contains(&cap), "missing capability {cap}");
    }
    let known: Vec<&str> = ctx.capabilities.iter().filter(|c| c.access_level == awp_types::TrustLevel::Known).map(|c| c.name.as_str()).collect();
    for cap in ["approve_action", "set_permissions", "pause_agents", "manage_memory"] {
        assert!(known.contains(&cap), "{cap} must require known trust");
    }
}

// ---------------------------------------------------------------------------
// Phase 2 · team sprint A (Platform) — consents (S11-T4 pulled forward) and tasks (S4-T3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn consent_store_grants_checks_and_revokes_per_world() {
    use adk_awp::ConsentService;
    use spatial_os::domain::Domain;
    use spatial_os::memory::consent::ConsentStore;

    let store = ConsentStore::in_memory();
    let u = "u-consent";
    let first = store.grant(u, "email", Domain::Work, "read and draft replies").await.expect("valid category");
    let again = store.grant(u, "email", Domain::Work, "duplicate").await.unwrap();
    assert_eq!(first.id, again.id, "granting twice is idempotent");
    assert!(store.has(u, "email", Domain::Work).await);
    assert!(!store.has(u, "email", Domain::Home).await, "a work grant does not cover home");
    assert!(store.has(u, "email", Domain::Shared).await, "a cross-world check accepts any active grant");
    store.grant(u, "calendar", Domain::Shared, "scheduling").await.unwrap();
    assert!(store.has(u, "calendar", Domain::Home).await, "a shared grant covers both worlds");
    assert_eq!(store.revoke(u, "email", None).await, 1);
    assert!(!store.has(u, "email", Domain::Work).await);
    assert_eq!(store.list(u).await.len(), 2, "history keeps revoked grants");
    assert_eq!(store.active(u).await.len(), 1);
    assert!(store.grant(u, "Not Valid!", Domain::Shared, "x").await.is_none());

    // adk-awp view of the same store: subject = user, purpose = category.
    store.capture_consent("awp-subject", "reading").await.unwrap();
    assert!(store.check_consent("awp-subject", "reading").await.unwrap());
    store.revoke_consent("awp-subject", "reading").await.unwrap();
    assert!(!store.check_consent("awp-subject", "reading").await.unwrap());
    assert!(store.capture_consent("awp-subject", "bad purpose!").await.is_err());
}

#[tokio::test]
async fn consent_routes_get_and_put_require_known_trust() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use spatial_os::domain::Domain;
    use tower::ServiceExt;

    let (state, headers) = known_app_state();
    let rec = state.sessions.create_for_user("u-consent-routes".into()).await;
    let app = axum::Router::new()
        .route(
            "/api/consents",
            axum::routing::get(spatial_os::routes::consents::get).put(spatial_os::routes::consents::put),
        )
        .with_state(state.clone());
    let send = |req: Request<Body>| {
        let app = app.clone();
        let headers = headers.clone();
        async move {
            let mut req = req;
            req.headers_mut().extend(headers);
            app.oneshot(req).await.unwrap()
        }
    };
    let json = |body: serde_json::Value| Body::from(body.to_string());
    let read = |res: axum::response::Response| async move {
        serde_json::from_slice::<serde_json::Value>(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap()
    };

    let anon = app
        .clone()
        .oneshot(Request::get(format!("/api/consents?session_id={}", rec.session_id)).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::FORBIDDEN);

    let res = send(
        Request::put("/api/consents")
            .header("content-type", "application/json")
            .body(json(serde_json::json!({
                "session_id": rec.session_id, "category": "health", "world": "home",
                "purpose": "sleep and exercise facts for the wellbeing section", "granted": true
            })))
            .unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = read(res).await;
    assert_eq!(body["consents"].as_array().unwrap().len(), 1);
    assert_eq!(body["consents"][0]["world"], "home");
    assert!(body["consents"][0]["revoked_at"].is_null());

    let res = send(
        Request::put("/api/consents")
            .header("content-type", "application/json")
            .body(json(serde_json::json!({"session_id": rec.session_id, "category": "telepathy", "granted": true, "purpose": "x"})))
            .unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "unknown category");
    let res = send(
        Request::put("/api/consents")
            .header("content-type", "application/json")
            .body(json(serde_json::json!({"session_id": rec.session_id, "category": "finance", "granted": true})))
            .unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST, "granting needs a purpose");

    let res = send(Request::get(format!("/api/consents?session_id={}", rec.session_id)).body(Body::empty()).unwrap()).await;
    let body = read(res).await;
    assert_eq!(body["categories"].as_array().unwrap().len(), spatial_os::memory::consent::CATEGORIES.len());
    assert_eq!(body["persisted"], false);
    assert!(state.consents.has(&rec.user_id, "health", Domain::Home).await);

    let res = send(
        Request::put("/api/consents")
            .header("content-type", "application/json")
            .body(json(serde_json::json!({"session_id": rec.session_id, "category": "health", "granted": false})))
            .unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(!state.consents.has(&rec.user_id, "health", Domain::Home).await);
    assert!(read(res).await["consents"][0]["revoked_at"].is_string());
}

/// Needs `DATABASE_URL` (CI service container or `docker compose up -d`).
#[tokio::test]
async fn consent_pg_roundtrip_persists_grant_and_revocation() {
    use spatial_os::domain::Domain;
    use spatial_os::memory::consent::ConsentStore;

    let pool = common::postgres_pool().await;
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrations apply");
    let user = format!("u-consent-pg-{}", uuid::Uuid::new_v4());

    let store = ConsentStore::new(Some(pool.clone()));
    let granted = store.grant(&user, "finance", Domain::Home, "read-only budgets").await.unwrap();

    let fresh = ConsentStore::new(Some(pool.clone()));
    assert!(fresh.has(&user, "finance", Domain::Home).await, "grant survives a new store instance");
    assert_eq!(fresh.revoke(&user, "finance", Some(Domain::Home)).await, 1);

    let third = ConsentStore::new(Some(pool.clone()));
    assert!(!third.has(&user, "finance", Domain::Home).await, "revocation survives too");
    let all = third.list(&user).await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, granted.id);
    assert!(all[0].revoked_at.is_some());

    assert_eq!(third.purge(&user).await, 1);
    assert!(ConsentStore::new(Some(pool)).list(&user).await.is_empty());
}

#[tokio::test]
async fn tasks_store_creates_postpones_completes_and_ledgers() {
    use spatial_os::domain::Domain;
    use spatial_os::intelligence::LedgerQuery;
    use spatial_os::permissions::gate;
    use spatial_os::tools::tasks::{NewTask, Priority, TaskFilter, TaskKind, TaskStatus, TaskStore};

    let store = TaskStore::in_memory();
    let u = "u-tasks-store";
    let due = chrono::Utc::now() + chrono::Duration::days(2);
    let t = store
        .create(
            u,
            NewTask {
                domain: Domain::Work,
                title: "Finish board memo",
                kind: TaskKind::Deadline,
                due: Some(due),
                duration_minutes: None,
                priority: Priority::High,
                source_agent: "calendar_agent",
                notes: Some("for Friday's board meeting"),
            },
        )
        .await;
    assert_eq!(t.status, TaskStatus::Open);
    assert_eq!(t.postponed_count, 0);

    let p = store.postpone(u, t.id, None, "calendar_agent").await.expect("open task");
    assert_eq!(p.postponed_count, 1);
    assert_eq!(p.due, Some(due + chrono::Duration::days(1)), "default postpone is one day");

    let done = store.complete(u, t.id, "calendar_agent").await.expect("open task");
    assert_eq!(done.status, TaskStatus::Done);
    assert!(done.completed_at.is_some());
    assert!(store.postpone(u, t.id, None, "calendar_agent").await.is_none(), "done tasks cannot be postponed");
    assert!(store.list(u, Domain::Shared, &TaskFilter { status: Some(TaskStatus::Open), ..Default::default() }).await.is_empty());
    assert_eq!(store.list(u, Domain::Shared, &TaskFilter::default()).await.len(), 1);

    // The ledger saw three content-free events with the task id hashed — never the title.
    let svc = gate::services();
    svc.ledger.flush().await;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let events = svc
        .ledger
        .query(&LedgerQuery { user_id: u.into(), agent_id: Some("calendar_agent".into()), ..Default::default() })
        .await;
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    for k in ["task_created", "task_postponed", "task_completed"] {
        assert!(kinds.contains(&k), "missing ledger kind {k} in {kinds:?}");
    }
    let postponed = events.iter().find(|e| e.kind == "task_postponed").unwrap();
    assert_eq!(postponed.meta["postponed_count"], 1);
    assert_eq!(postponed.meta["kind"], "deadline");
    assert_eq!(postponed.subject_hash.as_ref().map(String::len), Some(24));
    let dump = serde_json::to_string(&events).unwrap();
    assert!(!dump.contains("board memo") && !dump.contains("Friday"), "ledger must stay content-free");
}

#[tokio::test]
async fn tasks_tools_pass_through_the_permission_gate() {
    let _serial = gate_lock().lock().await;
    use spatial_os::permissions::{gate, Mode};

    let svc = gate::services();
    let user = adk_tool::SimpleToolContext::new("x").user_id().to_string();
    let gated = spatial_os::permissions::PermissionGate::wrap(
        "calendar_agent",
        spatial_os::tools::tasks::TasksTools::for_agent("calendar_agent"),
    );
    let ctx: Arc<dyn adk_core::ReadonlyContext> = Arc::new(adk_tool::SimpleToolContext::new("test"));
    let tools: std::collections::HashMap<String, Arc<dyn adk_core::Tool>> =
        gated.tools(ctx).await.unwrap().into_iter().map(|t| (t.name().to_string(), t)).collect();
    assert_eq!(tools.len(), 5);
    for name in spatial_os::tools::tasks::TOOL_NAMES {
        assert!(tools.contains_key(name), "missing tool {name}");
    }
    assert!(tools["list_tasks"].is_read_only());
    assert!(tools["plan_day"].is_read_only());
    assert!(!tools["create_task"].is_read_only());

    svc.permissions.set_mode(&user, "calendar_agent", Mode::Observe).await;
    let denied = tools["create_task"].execute(tool_ctx("s-tasks"), serde_json::json!({"title": "Renew passport"})).await.unwrap();
    assert_eq!(denied["status"], "denied");
    let listed = tools["list_tasks"].execute(tool_ctx("s-tasks"), serde_json::json!({})).await.unwrap();
    assert_eq!(listed["scope"], "work", "reads still work in observe mode");

    svc.permissions.set_mode(&user, "calendar_agent", Mode::Suggest).await;
    let created = tools["create_task"]
        .execute(tool_ctx("s-tasks"), serde_json::json!({"title": "Renew passport", "due": "tomorrow", "priority": "high", "kind": "errand"}))
        .await
        .unwrap();
    assert_eq!(created["status"], "created", "{created}");
    assert_eq!(created["task"]["domain"], "work", "calendar_agent lives in the work world");
    assert_eq!(created["task"]["kind"], "errand");
    let id = created["task"]["id"].as_str().unwrap().to_string();

    let home = tools["create_task"].execute(tool_ctx("s-tasks"), serde_json::json!({"title": "Dentist", "domain": "home"})).await.unwrap();
    assert_eq!(home["status"], "rejected", "a work agent cannot write home tasks");
    let bad_due = tools["create_task"].execute(tool_ctx("s-tasks"), serde_json::json!({"title": "x", "due": "next week"})).await.unwrap();
    assert_eq!(bad_due["status"], "rejected");

    let listed = tools["list_tasks"].execute(tool_ctx("s-tasks"), serde_json::json!({})).await.unwrap();
    assert!(listed["tasks"].as_array().unwrap().iter().any(|t| t["id"] == id));

    let postponed = tools["postpone_task"].execute(tool_ctx("s-tasks"), serde_json::json!({"task_id": id, "due": "2030-01-05"})).await.unwrap();
    assert_eq!(postponed["status"], "postponed", "{postponed}");
    assert_eq!(postponed["task"]["postponed_count"], 1);
    assert_eq!(postponed["task"]["due"], "2030-01-05T23:59:59Z");

    let plan = tools["plan_day"].execute(tool_ctx("s-tasks"), serde_json::json!({})).await.unwrap();
    assert_eq!(plan["status"], "ok");
    assert!(plan["plan"]["date"].is_string());
    assert!(plan["plan"]["open_total"].as_u64().unwrap() >= 1);
    let outside = tools["plan_day"].execute(tool_ctx("s-tasks"), serde_json::json!({"domain": "home"})).await.unwrap();
    assert_eq!(outside["status"], "rejected");

    let completed = tools["complete_task"].execute(tool_ctx("s-tasks"), serde_json::json!({"task_id": id})).await.unwrap();
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["task"]["status"], "done");
    let missing = tools["complete_task"].execute(tool_ctx("s-tasks"), serde_json::json!({"task_id": uuid::Uuid::new_v4()})).await.unwrap();
    assert_eq!(missing["status"], "rejected");

    // Audit shows the write_local decisions; the anonymous gate user gets a clean mode back.
    let allowed = svc
        .audit
        .list(&user, 200)
        .await
        .into_iter()
        .filter(|e| e.tool == "create_task" && e.decision == "allowed")
        .count();
    assert!(allowed >= 1);
    let denied_count = svc.audit.list(&user, 200).await.into_iter().filter(|e| e.tool == "create_task" && e.decision == "denied").count();
    assert!(denied_count >= 1);
}

#[tokio::test]
async fn tasks_toolset_is_attached_only_to_agents_that_allowlist_it() {
    let inner: Arc<dyn adk_core::Toolset> = Arc::new(FakeInboxTools);
    let ctx: Arc<dyn adk_core::ReadonlyContext> = Arc::new(adk_tool::SimpleToolContext::new("t"));

    let calendar = spatial_os::agents::gemini::filtered_for_agent("calendar_agent", inner.clone());
    let names: Vec<String> = calendar.tools(ctx.clone()).await.unwrap().iter().map(|t| t.name().to_string()).collect();
    assert!(names.contains(&"create_task".to_string()), "{names:?}");
    assert!(names.contains(&"plan_day".to_string()));
    assert!(names.contains(&"read_memory".to_string()), "memory tools still attached");
    assert!(!names.contains(&"send_anything".to_string()), "the MCP allowlist still filters");

    let inbox = spatial_os::agents::gemini::filtered_for_agent("inbox_agent", inner);
    let names: Vec<String> = inbox.tools(ctx).await.unwrap().iter().map(|t| t.name().to_string()).collect();
    assert!(!names.contains(&"create_task".to_string()), "inbox_agent has no tasks entry: {names:?}");
    assert!(names.contains(&"create_draft".to_string()));
}

#[tokio::test]
async fn tasks_route_lists_open_tasks_for_known_user() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use spatial_os::domain::Domain;
    use spatial_os::tools::tasks::{NewTask, Priority, TaskKind};
    use tower::ServiceExt;

    let (state, headers) = known_app_state();
    let rec = state.sessions.create_for_user("u-tasks-routes".into()).await;
    let work = state
        .tasks
        .create(
            &rec.user_id,
            NewTask { domain: Domain::Work, title: "Board memo", kind: TaskKind::Deadline, due: None, duration_minutes: None, priority: Priority::High, source_agent: "test", notes: None },
        )
        .await;
    state
        .tasks
        .create(
            &rec.user_id,
            NewTask { domain: Domain::Home, title: "Passport", kind: TaskKind::Errand, due: None, duration_minutes: None, priority: Priority::Normal, source_agent: "test", notes: None },
        )
        .await;
    state.tasks.complete(&rec.user_id, work.id, "test").await.unwrap();

    let app = axum::Router::new()
        .route("/api/tasks", axum::routing::get(spatial_os::routes::tasks::list))
        .with_state(state.clone());
    let anon = app
        .clone()
        .oneshot(Request::get(format!("/api/tasks?session_id={}", rec.session_id)).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::FORBIDDEN);

    let get = |path: String| {
        let app = app.clone();
        let headers = headers.clone();
        async move {
            let mut req = Request::get(path).body(Body::empty()).unwrap();
            req.headers_mut().extend(headers);
            let res = app.oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            serde_json::from_slice::<serde_json::Value>(&axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap()).unwrap()
        }
    };
    let open = get(format!("/api/tasks?session_id={}", rec.session_id)).await;
    assert_eq!(open["count"], 1);
    assert_eq!(open["tasks"][0]["title"], "Passport");
    assert_eq!(open["persisted"], false);
    let all = get(format!("/api/tasks?session_id={}&status=all", rec.session_id)).await;
    assert_eq!(all["count"], 2);
    let work_only = get(format!("/api/tasks?session_id={}&status=all&domain=work", rec.session_id)).await;
    assert_eq!(work_only["count"], 1);
    assert_eq!(work_only["tasks"][0]["status"], "done");
}

#[test]
fn tasks_allowlist_declares_builtin_toolset_with_effects() {
    use spatial_os::permissions::Effect;
    use spatial_os::tools::allowlist::AllowlistCatalog;
    use spatial_os::tools::tasks::{TOOLSET_ID, TOOL_NAMES};

    let path = common::manifest_dir().join("mcp_allowlists.toml");
    let catalog = AllowlistCatalog::from_file(&path).expect("catalog");
    let spec = catalog.spec_for("calendar_agent").expect("calendar_agent spec");
    assert!(spec.mcp_servers.iter().any(|m| m == TOOLSET_ID));
    for tool in TOOL_NAMES {
        assert!(catalog.effect_for("calendar_agent", tool).is_some(), "{tool} needs an effect");
    }
    assert_eq!(catalog.effect_for("calendar_agent", "create_task"), Some(Effect::WriteLocal));
    assert_eq!(catalog.effect_for("calendar_agent", "postpone_task"), Some(Effect::WriteLocal));
    assert_eq!(catalog.effect_for("calendar_agent", "complete_task"), Some(Effect::WriteLocal));
    assert_eq!(catalog.effect_for("calendar_agent", "list_tasks"), Some(Effect::Read));
    assert_eq!(catalog.effect_for("calendar_agent", "plan_day"), Some(Effect::Read));
    assert!(catalog.validate_effects(true).expect("classified").is_empty());
}

#[test]
fn tasks_and_consents_capabilities_require_known_trust() {
    use adk_awp::BusinessContextLoader;
    let path = common::manifest_dir().join("business.toml");
    let ctx = BusinessContextLoader::from_file(&path).expect("business.toml").load();
    for cap in ["list_tasks", "manage_consents"] {
        let c = ctx.capabilities.iter().find(|c| c.name == cap).unwrap_or_else(|| panic!("missing capability {cap}"));
        assert_eq!(c.access_level, awp_types::TrustLevel::Known, "{cap} must require known trust");
    }
}

/// Needs `DATABASE_URL` (CI service container or `docker compose up -d`).
#[tokio::test]
async fn tasks_pg_roundtrip_and_sprint_a_migrations() {
    use spatial_os::domain::Domain;
    use spatial_os::tools::tasks::{NewTask, Priority, TaskKind, TaskStore};

    let pool = common::postgres_pool().await;
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrations apply");

    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT tablename::text FROM pg_tables WHERE schemaname = 'public'").fetch_all(&pool).await.expect("tables");
    let names: Vec<&str> = tables.iter().map(|(t,)| t.as_str()).collect();
    for expected in ["tasks", "consents", "activity_daily", "baselines", "observations"] {
        assert!(names.contains(&expected), "missing table {expected}, got {names:?}");
    }

    let user = format!("u-tasks-pg-{}", uuid::Uuid::new_v4());
    let store = TaskStore::new(Some(pool.clone()));
    let t = store
        .create(
            &user,
            NewTask { domain: Domain::Home, title: "Book dentist", kind: TaskKind::Errand, due: None, duration_minutes: None, priority: Priority::Normal, source_agent: "test", notes: Some("ask about Saturday slots") },
        )
        .await;
    store.postpone(&user, t.id, None, "test").await.expect("open");

    let fresh = TaskStore::new(Some(pool.clone()));
    let got = fresh.get(&user, t.id).await.expect("persisted task");
    assert_eq!(got.postponed_count, 1);
    assert_eq!(got.domain, Domain::Home);
    assert_eq!(got.kind, TaskKind::Errand);
    assert!(got.due.is_some(), "postponing an undated task gives it a due date");
    assert_eq!(got.notes.as_deref(), Some("ask about Saturday slots"));

    assert_eq!(fresh.purge(&user).await, 1);
    assert!(TaskStore::new(Some(pool)).get(&user, t.id).await.is_none());
}

// ---------------------------------------------------------------------------------------------
// Phase 2 · S7 — patterns + personal baseline (S7-T1, S7-T2, S7-T9)
// Offline tests read the checked-in synthetic ledger; the Postgres round-trip needs DATABASE_URL.
// ---------------------------------------------------------------------------------------------

const S7_USER: &str = "00000000-0000-0000-0000-000000000001";

fn s7_fixture() -> Vec<spatial_os::intelligence::ActivityEvent> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/synth_ledger_6w_drift.jsonl");
    let text = std::fs::read_to_string(&path).expect("synthetic ledger fixture is readable");
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| serde_json::from_str(l).expect("fixture row parses as ActivityEvent"))
        .collect()
}

fn s7_date(s: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn s7_patterns_aggregate_fixture_into_daily_values() {
    use spatial_os::intelligence::patterns::{aggregate, dim};
    let events = s7_fixture();
    assert_eq!(events.len(), 934);
    let daily = aggregate(&events);
    let days: std::collections::BTreeSet<_> = daily.iter().map(|d| d.day).collect();
    assert_eq!(days.len(), 42);
    let work_end: Vec<_> = daily.iter().filter(|d| d.dimension == dim::WORK_END).collect();
    assert_eq!(work_end.len(), 30, "work end exists on weekdays only");
    assert!(work_end.iter().all(|d| d.value > 16.0 && d.value < 21.0));
    assert_eq!(daily.iter().filter(|d| d.dimension == dim::READING_MINUTES).count(), 42);
    assert!(daily.iter().all(|d| dim::ALL.contains(&d.dimension.as_str())), "only known dimension ids");
}

#[test]
fn s7_baseline_drift_yields_exactly_one_observation() {
    use spatial_os::intelligence::baseline::{evaluate, Config};
    use spatial_os::intelligence::patterns::{aggregate, dim};
    let daily = aggregate(&s7_fixture());
    let out = evaluate(S7_USER, &daily, s7_date("2026-09-19"), &[], &Config::default());
    assert!(out.warmup.is_none(), "28 baseline days are present");
    let dims: std::collections::BTreeSet<&str> = out.drifts.iter().map(|d| d.dimension.as_str()).collect();
    assert!(dims.contains(dim::WORK_END) && dims.contains(dim::READING_MINUTES), "drifting: {dims:?}");
    assert!(!dims.contains(dim::WORK_START), "work start did not move: {dims:?}");
    let obs = out.observation.expect("exactly one drift observation");
    assert_eq!(obs.kind, "drift");
    assert!(obs.dimensions.len() >= 2, "{:?}", obs.dimensions);
    assert!(obs.text.contains("two weeks"), "{}", obs.text);
    assert!(obs.text.contains("your usual"), "{}", obs.text);
    let lower = obs.text.to_lowercase();
    for banned in ["overwork", "too much", "should", "unhealthy", "procrastinat", "lazy"] {
        assert!(!lower.contains(banned), "verdict word {banned:?} in {}", obs.text);
    }
    assert_eq!(obs.facts["window_days"], 14);
    assert!(obs.facts["dimensions"].as_array().unwrap().len() >= 2);
    assert_eq!(obs.offer, "Would you like me to help you review what's changed?");
}

#[test]
fn s7_baseline_warmup_suppresses_observations() {
    use spatial_os::intelligence::baseline::{evaluate, Config};
    use spatial_os::intelligence::patterns::aggregate;
    let daily = aggregate(&s7_fixture());
    // As of 1 Sep the baseline window reaches back before the first fixture day: < 14 days seen.
    let out = evaluate(S7_USER, &daily, s7_date("2026-09-01"), &[], &Config::default());
    let w = out.warmup.expect("still learning");
    assert!(w.days_seen < w.days_needed, "{w:?}");
    assert!(out.observation.is_none() && out.drifts.is_empty());
}

#[test]
fn s7_baseline_cooldown_suppresses_repeat() {
    use spatial_os::intelligence::baseline::{evaluate, Config, ExistingObservation};
    use spatial_os::intelligence::patterns::aggregate;
    let daily = aggregate(&s7_fixture());
    let as_of = s7_date("2026-09-19");
    let first = evaluate(S7_USER, &daily, as_of, &[], &Config::default()).observation.expect("first run fires");
    let recent = [ExistingObservation { dimensions: first.dimensions.clone(), created_on: s7_date("2026-09-17") }];
    let again = evaluate(S7_USER, &daily, as_of, &recent, &Config::default());
    assert!(again.observation.is_none(), "inside the 7-day cooldown");
    assert!(!again.cooled_down.is_empty());
    let old = [ExistingObservation { dimensions: first.dimensions.clone(), created_on: s7_date("2026-09-01") }];
    assert!(evaluate(S7_USER, &daily, as_of, &old, &Config::default()).observation.is_some(), "after the cooldown");
}

#[test]
fn s7_baseline_disabled_dimension_is_ignored() {
    use spatial_os::intelligence::baseline::{evaluate, Config};
    use spatial_os::intelligence::patterns::{aggregate, dim};
    let daily = aggregate(&s7_fixture());
    let mut cfg = Config::default();
    for d in [dim::WORK_END, dim::WORK_MINUTES, dim::READING_MINUTES] {
        cfg.disabled.insert(d.to_string());
    }
    let out = evaluate(S7_USER, &daily, s7_date("2026-09-19"), &[], &cfg);
    assert!(out.drifts.iter().all(|d| !cfg.disabled.contains(&d.dimension)), "{:?}", out.drifts);
    assert!(out.baselines.iter().all(|b| !cfg.disabled.contains(&b.dimension)));
}

#[tokio::test]
async fn s7_store_roundtrip_daily_baselines_observation() {
    use spatial_os::intelligence::baseline::{evaluate, Config};
    use spatial_os::intelligence::patterns::aggregate;
    use spatial_os::intelligence::store;
    common::load_env();
    if std::env::var("DATABASE_URL").is_err() {
        return;
    }
    let pool = common::postgres_pool().await;
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrations apply");
    let user = format!("s7-test-{}", uuid::Uuid::new_v4());
    let mut daily = aggregate(&s7_fixture());
    for d in &mut daily {
        d.user_id = user.clone();
    }
    let n = store::upsert_daily(&pool, &daily).await.unwrap();
    assert_eq!(store::upsert_daily(&pool, &daily).await.unwrap(), n, "idempotent");
    let as_of = s7_date("2026-09-19");
    let out = evaluate(&user, &daily, as_of, &[], &Config::default());
    store::upsert_baselines(&pool, &out.baselines).await.unwrap();
    let obs = out.observation.expect("drift on the fixture");
    store::insert_observation(&pool, &obs).await.unwrap();
    let recent = store::recent_observations(&pool, &user, s7_date("2026-09-12")).await.unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].dimensions, obs.dimensions);
    assert_eq!(recent[0].created_on, as_of);
    for table in ["observations", "baselines", "activity_daily"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE user_id = $1")).bind(&user).execute(&pool).await.unwrap();
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Phase 2 · S4 — Work World
// ---------------------------------------------------------------------------

#[tokio::test]
async fn work_mother_folds_fan_out_into_one_result_with_stubs_and_follow_ups() {
    use chrono::{Duration, Utc};
    use spatial_os::domain::Domain;
    use spatial_os::intelligence::ledger::ActivityEvent;
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;
    use spatial_os::permissions::Effect;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-work-mother".into()).await;
    // A thread the inbox agent read four days ago and never answered (hashed subject only).
    let mut ev = ActivityEvent::new(&rec.user_id, Domain::Work, "inbox_agent", "tool_call")
        .effect(Effect::Read)
        .subject(state.ledger.hash_key(), "thread-contract");
    ev.ts = Utc::now() - Duration::days(4);
    state.ledger.record(ev);

    let response = dispatch_intent(IntentDispatch { state: &state, session_id: rec.session_id.clone(), user_id: rec.user_id.clone(), text: "What's happening with work?".into() }).await;
    let events = collect_sse_events(response).await;
    let spawns: Vec<&serde_json::Value> = events.iter().filter(|e| e["type"] == "card_spawn").collect();
    // morning (3) + people (3) + follow-ups (1) + two labeled stubs = 9, all re-indexed and all work.
    assert_eq!(spawns.len(), 9, "{:?}", spawns.iter().map(|s| &s["card"]["title"]).collect::<Vec<_>>());
    assert_eq!(events[0]["total_cards"], 9);
    assert!(spawns.iter().all(|s| s["domain"] == "work"));
    let titles: Vec<&str> = spawns.iter().map(|s| s["card"]["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Follow-ups") && titles.contains(&"Career") && titles.contains(&"Professional presence"));
    let resolves: Vec<&serde_json::Value> = events.iter().filter(|e| e["type"] == "card_resolve").collect();
    assert_eq!(resolves.len(), 9);
    assert!(resolves.iter().any(|r| r["resolve"]["big"] == "1 unanswered"), "3-day-old thread is flagged");
    assert_eq!(resolves.iter().filter(|r| r["resolve"]["big"].as_str().map(|b| b.starts_with("STUB")).unwrap_or(false)).count(), 2, "stubs are labeled, never fake");
    assert_eq!(events.iter().filter(|e| e["type"] == "suzy_summary").count(), 1);
    let html = events.iter().find(|e| e["type"] == "suzy_summary").unwrap()["html"].as_str().unwrap();
    assert!(html.contains("Follow-ups") && html.contains("unanswered"), "{html}");
    assert!(!serde_json::to_string(&events).unwrap().contains("thread-contract"), "subjects never leave the ledger");
}

#[test]
fn work_agents_carry_no_finance_or_health_tools() {
    use spatial_os::domain::Domain;
    use spatial_os::tools::allowlist::AllowlistCatalog;
    let catalog = AllowlistCatalog::from_file(&common::manifest_dir().join("mcp_allowlists.toml")).expect("catalog");
    let forbidden_servers = ["banking", "market_data", "real_estate"];
    let forbidden_tools = ["list_transactions", "search_transactions", "list_accounts", "yfinance_chart", "get_forecast"];
    for id in spatial_os::worlds::work::phase1_agent_ids() {
        let spec = catalog.spec_for(id).unwrap_or_else(|| panic!("work agent {id} missing from allowlist"));
        assert_eq!(spec.world, Domain::Work, "{id} must be tagged work");
        assert!(spec.mcp_servers.iter().all(|s| !forbidden_servers.contains(&s.as_str())), "{id} reaches a finance/home server");
        assert!(spec.tool_names().iter().all(|t| !forbidden_tools.contains(&t.as_str())), "{id} has a finance/health tool");
    }
    // Stubs exist in the catalog with no tools and Observe mode.
    for id in ["career_agent", "professional_social_agent"] {
        let spec = catalog.spec_for(id).expect(id);
        assert!(spec.tools.is_empty());
        assert_eq!(spec.mode, spatial_os::permissions::Mode::Observe);
    }
    catalog.validate_effects(true).expect("effects complete");
}

#[tokio::test]
async fn work_stub_agents_build_and_label_themselves() {
    let career = spatial_os::agents::career::build().await.expect("career stub");
    assert_eq!(career.name(), "career_agent");
    let social = spatial_os::agents::professional_social::build().await.expect("social stub");
    assert_eq!(social.name(), "professional_social_agent");
}

/// Regression: every runner is keyed by its own app name, but only the `agentrix-os` session was
/// ever created, so the router, Suzy, the Mother's synthesis and the workflow runners failed with
/// `session.not_found` and silently fell back. `ensure_runner_session` creates the session on
/// first use and is idempotent.
#[tokio::test]
async fn mother_runner_session_is_created_on_first_use() {
    use adk_agent::CustomAgentBuilder;
    use adk_core::{Content, Event, SessionId, UserId};
    use adk_runner::Runner;
    use adk_session::{GetRequest, InMemorySessionService};

    let echo: Arc<dyn adk_core::Agent> = Arc::new(
        CustomAgentBuilder::new("echo")
            .description("replies with a fixed word")
            .handler(|_ctx| async move {
                let mut event = Event::new("echo");
                event.author = "echo".into();
                event.llm_response.content = Some(Content::new("assistant").with_text("PONG"));
                Ok(Box::pin(futures::stream::iter(vec![Ok(event)])) as adk_core::EventStream)
            })
            .build()
            .expect("stub agent"),
    );
    let sessions: Arc<dyn adk_session::SessionService> = Arc::new(InMemorySessionService::new());
    let runner = Runner::builder().app_name("agentrix-os-test-app").agent(echo).session_service(sessions.clone()).build().expect("runner");

    // Without the helper the run fails exactly the way the live server did: `Runner::run`
    // returns a stream whose first item is the `session.not_found` error.
    let mut missing = runner
        .run(UserId::try_from("u-runner").unwrap(), SessionId::try_from("s-runner").unwrap(), Content::new("user").with_text("hi"))
        .await
        .expect("run returns a stream");
    let first = missing.next().await.expect("one item");
    let err = first.err().expect("a runner must not find a session nobody created").to_string();
    assert!(err.contains("not_found") || err.contains("not found"), "unexpected error: {err}");
    drop(missing);

    spatial_os::agents::ensure_runner_session(&runner, "u-runner", "s-runner").await;
    spatial_os::agents::ensure_runner_session(&runner, "u-runner", "s-runner").await; // idempotent
    assert!(sessions
        .get(GetRequest { app_name: "agentrix-os-test-app".into(), user_id: "u-runner".into(), session_id: "s-runner".into(), num_recent_events: None, after: None })
        .await
        .is_ok());

    let mut stream = runner
        .run(UserId::try_from("u-runner").unwrap(), SessionId::try_from("s-runner").unwrap(), Content::new("user").with_text("hi"))
        .await
        .expect("run succeeds once the session exists");
    let mut text = String::new();
    while let Some(ev) = stream.next().await {
        if let Some(c) = ev.expect("event").llm_response.content {
            text.extend(c.parts.iter().filter_map(|p| p.text().map(str::to_string)));
        }
    }
    assert!(text.contains("PONG"), "got {text:?}");
}

// ---------------------------------------------------------------------------
// Phase 2 · S5 — Home World
// ---------------------------------------------------------------------------

#[tokio::test]
async fn home_mother_folds_family_and_personal_cards_from_memory() {
    use chrono::{Duration, Local};
    use spatial_os::domain::Domain;
    use spatial_os::intelligence::ledger::ActivityEvent;
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-home-mother".into()).await;
    let run = |text: String| {
        let state = state.clone();
        let (sid, uid) = (rec.session_id.clone(), rec.user_id.clone());
        async move { collect_sse_events(dispatch_intent(IntentDispatch { state: &state, session_id: sid, user_id: uid, text }).await).await }
    };

    // The user states a date and a personal task in chat; the task was postponed twice (ledger).
    let soon = (Local::now().date_naive() + Duration::days(10)).format("%-d %b").to_string();
    run(format!("Remember Sara's birthday is {soon}")).await;
    run("remember to renew my passport".to_string()).await;
    for _ in 0..2 {
        state.ledger.record(
            ActivityEvent::new(&rec.user_id, Domain::Home, "personal_productivity", "task_postponed")
                .subject(state.ledger.hash_key(), "task.renew_my_passport"),
        );
    }
    let items = state.memory.export(&rec.user_id).await;
    assert!(items.iter().any(|i| i.key == "date.birthday.sara" && i.domain == Domain::Home), "{:?}", items.iter().map(|i| &i.key).collect::<Vec<_>>());
    assert!(items.iter().any(|i| i.key == "task.renew_my_passport" && i.domain == Domain::Home));

    let events = run("Remind me about family commitments.".to_string()).await;
    assert_eq!(events[0]["type"], "scenario");
    assert_eq!(events[0]["total_cards"], 3, "Family + Personal + labeled Personal social stub");
    let spawns: Vec<&serde_json::Value> = events.iter().filter(|e| e["type"] == "card_spawn").collect();
    assert!(spawns.iter().all(|s| s["domain"] == "home"));
    let titles: Vec<&str> = spawns.iter().map(|s| s["card"]["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Family", "Personal", "Personal social"]);
    let resolves: Vec<&serde_json::Value> = events.iter().filter(|e| e["type"] == "card_resolve").collect();
    assert_eq!(resolves[0]["resolve"]["big"], "1 date ahead");
    assert!(resolves[0]["resolve"]["sub"].as_str().unwrap().contains("birthday · Sara in 10 days"), "{}", resolves[0]["resolve"]["sub"]);
    assert_eq!(resolves[1]["resolve"]["big"], "1 personal task");
    assert!(resolves[1]["resolve"]["sub"].as_str().unwrap().contains("renew my passport (postponed 2×)"));
    assert!(resolves[2]["resolve"]["big"].as_str().unwrap().starts_with("STUB"));
    let summary = events.iter().find(|e| e["type"] == "suzy_summary").unwrap()["html"].as_str().unwrap();
    assert!(summary.contains("Home:") && summary.contains("Sara"), "{summary}");
    assert_eq!(events.iter().filter(|e| e["type"] == "suzy_summary").count(), 1);
}

#[tokio::test]
async fn home_personal_productivity_reads_the_shared_tasks_store() {
    use spatial_os::agents::personal_productivity::{self, SOURCE_STORE};
    use spatial_os::domain::Domain;
    use spatial_os::intelligence::ledger::LedgerService;
    use spatial_os::memory::MemoryService;
    use spatial_os::tools::tasks::{store_handle, NewTask, Priority, TaskKind, TOOL_NAMES};

    let user = "u-personal-store";
    let agent = "personal_productivity_agent";
    let store = store_handle();
    let new = |domain: Domain, title: &'static str, kind: TaskKind| NewTask {
        domain,
        title,
        kind,
        due: None,
        duration_minutes: None,
        priority: Priority::Normal,
        source_agent: agent,
        notes: None,
    };
    let passport = store.create(user, new(Domain::Home, "Renew passport", TaskKind::Errand)).await;
    store.postpone(user, passport.id, None, agent).await.expect("postponed once");
    store.postpone(user, passport.id, None, agent).await.expect("postponed twice");
    store.create(user, new(Domain::Shared, "Book the dentist", TaskKind::Task)).await;
    store.create(user, new(Domain::Work, "Ship the deck", TaskKind::Deadline)).await;
    let done = store.create(user, new(Domain::Home, "Water the plants", TaskKind::Household)).await;
    store.complete(user, done.id, agent).await.expect("completed");

    let facts = personal_productivity::facts(&MemoryService::in_memory(), &LedgerService::in_memory(), user).await;
    let titles: Vec<&str> = facts.tasks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Renew passport", "Book the dentist"], "home + shared, open only, most postponed first");
    assert_eq!(facts.tasks[0].postponed, 2);
    assert_eq!(facts.postponed_total, 2);
    assert!(facts.tasks.iter().all(|t| t.source == SOURCE_STORE && t.known));

    // The agent gets the five task tools behind the gate, scoped to home; no MCP tools leak in.
    let inner: Arc<dyn adk_core::Toolset> = Arc::new(FakeInboxTools);
    let ctx: Arc<dyn adk_core::ReadonlyContext> = Arc::new(adk_tool::SimpleToolContext::new("t"));
    let toolset = spatial_os::agents::gemini::filtered_for_agent(agent, inner);
    let names: Vec<String> = toolset.tools(ctx).await.unwrap().iter().map(|t| t.name().to_string()).collect();
    for tool in TOOL_NAMES {
        assert!(names.contains(&tool.to_string()), "{tool} missing: {names:?}");
    }
    assert!(!names.contains(&"create_draft".to_string()), "inbox tools must not reach the home agent");
    assert_eq!(spatial_os::tools::allowlist::catalog().world_for(agent), Domain::Home);
}

#[tokio::test]
async fn home_agents_keep_conservative_defaults_and_health_never_diagnoses() {
    use spatial_os::agents::week::{health_escalation, health_lint, health_sanitize};
    use spatial_os::domain::Domain;
    use spatial_os::permissions::{Effect, Mode};
    use spatial_os::tools::allowlist::AllowlistCatalog;

    let catalog = AllowlistCatalog::from_file(&common::manifest_dir().join("mcp_allowlists.toml")).expect("catalog");
    for id in spatial_os::worlds::home::phase1_agent_ids() {
        if let Some(spec) = catalog.spec_for(id) {
            assert_eq!(spec.world, Domain::Home, "{id} must be tagged home");
        }
    }
    let money = catalog.spec_for("money_agent").expect("money");
    assert_eq!(money.mode, Mode::Observe);
    assert!(money.tools.values().all(|e| *e == Some(Effect::Read)), "finance has zero non-read tools");
    for id in ["family_agent", "personal_social_agent"] {
        assert!(catalog.spec_for(id).expect(id).tools.is_empty());
    }
    // Personal Productivity carries exactly the shared tasks toolset (S4-T3), nothing that sends or publishes.
    let personal = catalog.spec_for("personal_productivity_agent").expect("personal_productivity_agent");
    assert!(personal.mcp_servers.iter().any(|m| m == spatial_os::tools::tasks::TOOLSET_ID));
    let mut names: Vec<&str> = personal.tools.keys().map(String::as_str).collect();
    names.sort_unstable();
    let mut expected = spatial_os::tools::tasks::TOOL_NAMES.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);
    assert!(personal.tools.values().all(|e| matches!(e, Some(Effect::Read) | Some(Effect::WriteLocal))), "{:?}", personal.tools);

    let bad = "You have insomnia and a sleep disorder. Steps steady; avg sleep 4.6h.";
    assert!(!health_lint(bad).is_empty());
    let clean = health_sanitize(bad);
    assert!(health_lint(&clean).is_empty() && clean.contains("Steps steady"), "{clean}");
    assert!(health_escalation(4.6, 6).unwrap().contains("health professional"));
    assert!(health_escalation(6.1, 6).is_none());
    assert!(health_escalation(4.0, 2).is_none(), "needs at least five nights");

    let fam = spatial_os::agents::family::build(spatial_os::memory::MemoryService::in_memory()).await.expect("family agent");
    assert_eq!(fam.name(), "family_agent");
    let social = spatial_os::agents::personal_social::build().await.expect("stub");
    assert_eq!(social.name(), "personal_social_agent");
}

// ---------------------------------------------------------------------------
// Phase 2 · S6 — agent bus and arbitration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bus_carries_requests_and_results_for_a_turn() {
    use spatial_os::domain::Domain;
    use spatial_os::mother::bus::{global, Kind};
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-bus".into()).await;
    let mut rx = global().subscribe();
    let _ = collect_sse_events(dispatch_intent(IntentDispatch { state: &state, session_id: rec.session_id.clone(), user_id: rec.user_id.clone(), text: "What's happening with work?".into() }).await).await;
    let mut msgs = Vec::new();
    while let Ok(m) = rx.try_recv() {
        msgs.push(m);
    }
    let trace = msgs.iter().find(|m| m.kind == Kind::Result && m.from.agent == "work_mother").map(|m| m.trace_id.clone()).expect("work_mother result");
    let turn = global().trace(&trace);
    assert!(turn.iter().all(|m| m.trace_id == trace));
    assert!(turn.iter().any(|m| m.kind == Kind::Request && m.from.agent == "mother" && m.to.agent == "work_mother" && m.depth == 1));
    assert!(turn.iter().any(|m| m.kind == Kind::Request && m.from.agent == "work_mother" && m.depth == 2));
    assert!(turn.iter().all(|m| m.depth <= spatial_os::mother::bus::MAX_DEPTH));
    let result = turn.iter().find(|m| m.kind == Kind::Result).unwrap();
    assert_eq!(result.domain, Domain::Shared);
    assert!(result.payload["facts"].as_array().unwrap().iter().any(|f| f.as_str().unwrap().contains("work agents ran")));
}

#[tokio::test]
async fn arbitration_asks_one_question_for_a_work_home_overlap() {
    use spatial_os::orchestrator::dispatch::{dispatch_intent, IntentDispatch};
    use spatial_os::orchestrator::sse_collect::collect_sse_events;

    let state = offline_app_state();
    let rec = state.sessions.create_for_user("u-arb".into()).await;
    let run = |text: &str| {
        let state = state.clone();
        let (sid, uid, text) = (rec.session_id.clone(), rec.user_id.clone(), text.to_string());
        async move { collect_sse_events(dispatch_intent(IntentDispatch { state: &state, session_id: sid, user_id: uid, text }).await).await }
    };
    run("Remember family dinner Thursday at 18:30").await;
    run("Remember the client review is Thursday 17:30-19:00").await;

    let events = run("I'm overwhelmed. Help me reorganize today.").await;
    let summary = events.iter().find(|e| e["type"] == "suzy_summary").unwrap()["html"].as_str().unwrap();
    assert!(summary.starts_with("You have a family commitment thursday at 18:30, but your current work schedule extends into that period"), "{summary}");
    assert_eq!(summary.matches("Would you like me to help reorganize").count(), 1, "exactly one question");
    assert!(!summary.contains("should") && !summary.contains("too much"), "neutral wording");
    let first_suggest = events.iter().find(|e| e["type"] == "suggest").unwrap();
    assert!(first_suggest["text"].as_str().unwrap().starts_with("💡 Reorganize my tasks"));

    state.ledger.flush().await;
    let conflicts = state.ledger.query(&spatial_os::intelligence::LedgerQuery { user_id: rec.user_id.clone(), kind: Some("conflict".into()), ..Default::default() }).await;
    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].trace_id.is_some());
    assert!(!serde_json::to_string(&conflicts).unwrap().contains("client review"), "ledger carries counts, not commitments");
}
