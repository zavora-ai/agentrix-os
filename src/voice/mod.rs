pub mod camera;
pub mod realtime;

use std::sync::Arc;

use adk_realtime::gemini::{GeminiLiveBackend, GeminiRealtimeModel};

use crate::config::AppConfig;

#[derive(Clone)]
pub struct VoiceState {
    pub enabled: bool,
    pub model: Option<Arc<GeminiRealtimeModel>>,
    pub voice_name: String,
    /// Spoken language pinned for the session (`VOICE_LANGUAGE`), see `AppConfig::voice_language`.
    pub language: Option<String>,
    /// Camera channel (M10-T5): frames over `/ws/voice`, gestures back as `ui_gesture` tool calls.
    pub camera: bool,
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
        }
    }
}