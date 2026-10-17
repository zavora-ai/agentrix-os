pub mod camera;
pub mod health;
pub mod realtime;

use std::sync::{Arc, RwLock};

use adk_realtime::gemini::{GeminiLiveBackend, GeminiRealtimeModel};

use crate::config::AppConfig;

#[derive(Clone)]
pub struct VoiceState {
    /// A Gemini key is configured and the Live model is built. Whether it works is
    /// [`VoiceState::available`].
    pub enabled: bool,
    pub model: Option<Arc<GeminiRealtimeModel>>,
    pub voice_name: String,
    /// Spoken language pinned for the session (`VOICE_LANGUAGE`), see `AppConfig::voice_language`.
    pub language: Option<String>,
    /// Camera channel (M10-T5): frames over `/ws/voice`, gestures back as `ui_gesture` tool calls.
    pub camera: bool,
    /// Browser origins `/ws/voice` accepts (`AppConfig::trusted_origins`).
    pub allowed_origins: Vec<String>,
    /// Camera button for local gesture detection (`AGENTRIX_CAMERA`). MediaPipe runs in the
    /// browser, so this needs no key: it stays on when voice is off or Gemini rejects the key.
    pub gestures: bool,
    /// Why a configured key cannot serve voice (boot key check, or a live session closed for
    /// its key). `None` while voice is believed to work.
    unavailable: Arc<RwLock<Option<String>>>,
}

impl VoiceState {
    pub fn boot(config: &AppConfig) -> Self {
        let Some(api_key) = config.google_api_key.as_deref() else {
            return Self {
                enabled: false,
                model: None,
                voice_name: config.voice_name.clone(),
                language: config.voice_language.clone(),
                camera: false,
                allowed_origins: config.trusted_origins(),
                gestures: config.camera_enabled,
                unavailable: Arc::default(),
            };
        };

        let backend = GeminiLiveBackend::studio(api_key);
        let model = Arc::new(GeminiRealtimeModel::new(
            backend,
            &config.gemini_live_model,
        ));

        Self {
            enabled: true,
            model: Some(model),
            voice_name: config.voice_name.clone(),
            language: config.voice_language.clone(),
            camera: config.camera_enabled,
            allowed_origins: config.trusted_origins(),
            gestures: config.camera_enabled,
            unavailable: Arc::default(),
        }
    }

    /// Voice can open a Live session: a key is configured and nothing has shown it to be bad.
    pub fn available(&self) -> bool {
        self.enabled && self.unavailable_reason().is_none()
    }

    /// Why voice is off, for `/api/voice/status` and the websocket's refusal.
    pub fn unavailable_reason(&self) -> Option<String> {
        if !self.enabled {
            return Some("Voice is off — no GOOGLE_API_KEY is configured.".into());
        }
        self.unavailable.read().ok().and_then(|r| r.clone())
    }

    /// Turn voice off for this process (a rejected key does not fix itself).
    pub fn mark_unavailable(&self, reason: impl Into<String>) {
        if let Ok(mut r) = self.unavailable.write() {
            *r = Some(reason.into());
        }
    }

    /// Check the configured key once, in the background: boot never waits on the network.
    /// A rejected key turns voice off with a reason; an unverifiable one (offline) leaves it on.
    pub fn spawn_key_check(&self, config: &AppConfig) {
        let Some(api_key) = config.google_api_key.clone().filter(|_| self.enabled) else {
            return;
        };
        let voice = self.clone();
        tokio::spawn(async move {
            match health::check_key(&api_key).await {
                health::KeyCheck::Valid => tracing::info!("Gemini key verified — voice available"),
                health::KeyCheck::Rejected(why) => {
                    tracing::warn!(
                        "Voice disabled: Gemini rejected GOOGLE_API_KEY ({why}). Fix the key in .env \
                         and restart; a GOOGLE_API_KEY exported in the shell wins over .env."
                    );
                    voice.mark_unavailable(format!(
                        "Voice is off — Gemini rejected the API key ({why})."
                    ));
                }
                health::KeyCheck::Unverified(why) => tracing::warn!(
                    "Could not verify GOOGLE_API_KEY ({why}); voice stays on and reports failures when used"
                ),
            }
        });
    }
}
