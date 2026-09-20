use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub web_dir: PathBuf,
    pub static_dir: PathBuf,
    pub audio_dir: PathBuf,
    pub business_toml: PathBuf,
    pub mcp_allowlists_toml: PathBuf,
    pub mcp_registry_path: Option<PathBuf>,
    pub artifact_dir: PathBuf,
    pub mcp_worksheet_path: PathBuf,
    pub mcp_docx_path: PathBuf,
    pub mcp_slides_path: PathBuf,
    pub mcp_calendar_path: PathBuf,
    pub mcp_email_path: PathBuf,
    pub mcp_news_path: PathBuf,
    pub mcp_weather_path: PathBuf,
    pub mcp_market_data_path: PathBuf,
    pub mcp_slack_path: PathBuf,
    pub mcp_crm_path: PathBuf,
    pub mcp_banking_path: PathBuf,
    pub mcp_github_path: PathBuf,
    pub mcp_maps_path: PathBuf,
    pub mcp_real_estate_path: PathBuf,
    pub google_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    /// Preferred text model; `claude-*` runs on Anthropic, anything else on Gemini.
    pub text_model: String,
    /// Gemini text model, used when `text_model` is Gemini or as the fallback.
    pub gemini_model: String,
    pub gemini_live_model: String,
    pub voice_name: String,
    pub database_url: Option<String>,
    pub jwt_secret: Option<String>,
    pub google_oauth_client_id: Option<String>,
    pub google_oauth_client_secret: Option<String>,
    pub base_url: String,
    pub signup_endpoint: Option<String>,
    pub linkedin_partner_id: Option<String>,
    pub linkedin_conversion_id: Option<u64>,
    pub allow_demo_mode: bool,
    /// `AGENTRIX_CAMERA` (default on): camera frames over the voice websocket when voice is enabled.
    pub camera_enabled: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        dotenvy::from_filename(".env.local").ok(); // local overrides, git-ignored
        dotenvy::dotenv().ok();

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        Ok(Self {
            host: std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "9847".into())
                .parse()
                .context("PORT must be a number")?,
            web_dir: manifest_dir.join("web"),
            static_dir: manifest_dir.join("web/static"),
            audio_dir: manifest_dir.join("audio"),
            business_toml: manifest_dir.join("business.toml"),
            mcp_allowlists_toml: manifest_dir.join("mcp_allowlists.toml"),
            mcp_registry_path: std::env::var("MCP_REGISTRY_PATH")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|raw| {
                    let path = PathBuf::from(&raw);
                    if path.is_absolute() {
                        path
                    } else {
                        manifest_dir.join(path)
                    }
                }),
            artifact_dir: manifest_dir
                .join(std::env::var("ARTIFACT_DIR").unwrap_or_else(|_| "./artifacts".into())),
            mcp_worksheet_path: resolve_path(
                &manifest_dir,
                "MCP_WORKSHEET_PATH",
                "../mcp-servers/worksheet-mcp/target/release/excel-mcp-server",
            ),
            mcp_docx_path: resolve_path(
                &manifest_dir,
                "MCP_DOCX_PATH",
                "../mcp-servers/docx-mcp/target/release/docx-mcp-server",
            ),
            mcp_slides_path: resolve_path(
                &manifest_dir,
                "MCP_SLIDES_PATH",
                "../mcp-servers/mcp-slides/target/release/slides-mcp-server",
            ),
            mcp_calendar_path: resolve_path(
                &manifest_dir,
                "MCP_CALENDAR_PATH",
                "../mcp-servers/mcp-calendar/target/release/mcp-calendar",
            ),
            mcp_email_path: resolve_path(
                &manifest_dir,
                "MCP_EMAIL_PATH",
                "../mcp-servers/mcp-email/target/release/mcp-email",
            ),
            mcp_news_path: resolve_path(
                &manifest_dir,
                "MCP_NEWS_PATH",
                "../mcp-servers/mcp-news/target/release/mcp-news",
            ),
            mcp_weather_path: resolve_path(
                &manifest_dir,
                "MCP_WEATHER_PATH",
                "../mcp-servers/mcp-weather/target/release/mcp-weather",
            ),
            mcp_market_data_path: resolve_path(
                &manifest_dir,
                "MCP_MARKET_DATA_PATH",
                "../mcp-servers/mcp-market-data/target/release/mcp-market-data",
            ),
            mcp_slack_path: resolve_path(
                &manifest_dir,
                "MCP_SLACK_PATH",
                "../mcp-servers/mcp-slack/target/release/mcp-slack",
            ),
            mcp_crm_path: resolve_path(
                &manifest_dir,
                "MCP_CRM_PATH",
                "../mcp-servers/mcp-crm/target/release/mcp-crm",
            ),
            mcp_banking_path: resolve_path(
                &manifest_dir,
                "MCP_BANKING_PATH",
                "../mcp-servers/mcp-banking/target/release/mcp-banking",
            ),
            mcp_github_path: resolve_path(
                &manifest_dir,
                "MCP_GITHUB_PATH",
                "../mcp-servers/mcp-github/target/release/adk-mcp-github",
            ),
            mcp_maps_path: resolve_path(
                &manifest_dir,
                "MCP_MAPS_PATH",
                "../mcp-servers/mcp-maps/target/release/mcp-maps",
            ),
            mcp_real_estate_path: resolve_path(
                &manifest_dir,
                "MCP_REAL_ESTATE_PATH",
                "../mcp-servers/mcp-real-estate/target/release/mcp-real-estate",
            ),
            google_api_key: std::env::var("GOOGLE_API_KEY").ok().filter(|k| !k.is_empty()),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .filter(|k| !k.is_empty()),
            text_model: std::env::var("TEXT_MODEL").unwrap_or_else(|_| "claude-fable-5-1".into()),
            gemini_model: std::env::var("GEMINI_MODEL")
                .unwrap_or_else(|_| "gemini-3.1-flash-lite".into()),
            gemini_live_model: std::env::var("GEMINI_LIVE_MODEL").unwrap_or_else(|_| {
                "models/gemini-3.8-live".into()
            }),
            voice_name: std::env::var("VOICE_NAME").unwrap_or_else(|_| "Aoede".into()),
            database_url: std::env::var("DATABASE_URL").ok().filter(|s| !s.is_empty()),
            jwt_secret: std::env::var("JWT_SECRET").ok().filter(|s| !s.is_empty()),
            google_oauth_client_id: std::env::var("GOOGLE_OAUTH_CLIENT_ID")
                .ok()
                .filter(|s| !s.is_empty()),
            google_oauth_client_secret: std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")
                .ok()
                .filter(|s| !s.is_empty()),
            base_url: std::env::var("BASE_URL").unwrap_or_else(|_| {
                let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
                let port = std::env::var("PORT").unwrap_or_else(|_| "9847".into());
                format!("http://{host}:{port}")
            }),
            signup_endpoint: std::env::var("AGENTRIX_SIGNUP_ENDPOINT")
                .ok()
                .filter(|s| !s.is_empty()),
            linkedin_partner_id: std::env::var("AGENTRIX_LINKEDIN_PARTNER_ID")
                .ok()
                .filter(|s| !s.is_empty()),
            linkedin_conversion_id: std::env::var("AGENTRIX_LINKEDIN_CONVERSION_ID")
                .ok()
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse().ok()),
            allow_demo_mode: std::env::var("AGENTRIX_ALLOW_DEMO")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            camera_enabled: std::env::var("AGENTRIX_CAMERA")
                .map(|v| !(v == "0" || v.eq_ignore_ascii_case("false")))
                .unwrap_or(true),
        })
    }

    /// Host portion for AWP discovery (`business.toml` domain override).
    pub fn public_domain(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if let Some(rest) = base
            .strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))
        {
            return rest.split('/').next().unwrap_or(rest).to_string();
        }
        format!("{}:{}", self.host, self.port)
    }

    pub fn postgres_enabled(&self) -> bool {
        self.database_url.is_some()
    }

    pub fn auth_enabled(&self) -> bool {
        self.jwt_secret.is_some() && self.postgres_enabled()
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Key and model for the text agents, or `None` when no usable key is set.
    pub fn text_backend(&self) -> Option<crate::llm::TextBackend> {
        crate::llm::select_text_backend(
            &self.text_model,
            self.anthropic_api_key.as_deref(),
            self.google_api_key.as_deref(),
            &self.gemini_model,
        )
    }

    pub fn deck_enabled(&self) -> bool {
        self.text_backend().is_some()
    }

    pub fn agents_enabled(&self) -> bool {
        self.deck_enabled()
    }

    pub fn voice_enabled(&self) -> bool {
        self.google_api_key.is_some()
    }
}

fn resolve_path(manifest_dir: &PathBuf, env_key: &str, default: &str) -> PathBuf {
    let raw = std::env::var(env_key).unwrap_or_else(|_| default.into());
    let path = PathBuf::from(&raw);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}