//! Text-model factory. Agents ask for "a model for this key and id" and never
//! name a provider; the id decides. `claude-*` → Anthropic, anything else → Gemini.
//! Voice stays on Gemini Live (`src/voice/`) regardless of this choice.

use std::sync::Arc;

use adk_core::{GenerateContentConfig, Llm};
use adk_model::anthropic::{AnthropicClient, AnthropicConfig, Effort};
use adk_model::gemini::GeminiModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    Gemini,
}

/// Which backend serves a model id.
pub fn provider_for(model: &str) -> Provider {
    if model.starts_with("claude-") {
        Provider::Anthropic
    } else {
        Provider::Gemini
    }
}

/// Whether the model accepts explicit `temperature` / `top_p` / `top_k`.
/// Claude 5-generation models reject them (the API returns 400); Gemini takes them.
pub fn allows_sampling(model: &str) -> bool {
    provider_for(model) == Provider::Gemini
}

/// Output ceiling for text agents on Anthropic. The API requires an explicit
/// value and the adk default (4096) truncates a long synthesis.
const ANTHROPIC_MAX_TOKENS: u32 = 16_000;

/// Generation settings for a text agent. Every `LlmAgentBuilder` goes through this.
///
/// `temperature` is sent only where the model accepts it ([`allows_sampling`]); adk-model rejects
/// explicit sampling for Claude 5-generation models before the request leaves the process.
/// On Anthropic, `max_tokens` also bounds the always-on adaptive thinking, so a cap sized for the
/// visible text (256–1024) can end the turn before any text is written: Anthropic gets at least
/// [`ANTHROPIC_MAX_TOKENS`]. Brevity comes from the instruction, not the cap.
pub fn content_config(model: &str, temperature: f32, max_output_tokens: i32) -> GenerateContentConfig {
    let max_output_tokens = match provider_for(model) {
        Provider::Anthropic => max_output_tokens.max(ANTHROPIC_MAX_TOKENS as i32),
        Provider::Gemini => max_output_tokens,
    };
    GenerateContentConfig {
        temperature: allows_sampling(model).then_some(temperature),
        max_output_tokens: Some(max_output_tokens),
        ..Default::default()
    }
}

/// Which key and model drive the text agents, resolved from what is configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBackend {
    pub api_key: String,
    pub model: String,
    /// True when the preferred model's key was missing and we fell back to Gemini.
    pub fallback: bool,
}

/// Prefer `text_model` when its provider's key is present. If the preferred model is
/// Anthropic and only `GOOGLE_API_KEY` is set, fall back to `gemini_model` so an
/// existing `.env` keeps booting live agents. Neither key → `None` (mock path).
pub fn select_text_backend(
    text_model: &str,
    anthropic_key: Option<&str>,
    google_key: Option<&str>,
    gemini_model: &str,
) -> Option<TextBackend> {
    let present = |k: Option<&str>| k.filter(|k| !k.trim().is_empty()).map(str::to_owned);
    match provider_for(text_model) {
        Provider::Anthropic => {
            if let Some(api_key) = present(anthropic_key) {
                return Some(TextBackend { api_key, model: text_model.to_owned(), fallback: false });
            }
            present(google_key).map(|api_key| TextBackend {
                api_key,
                model: gemini_model.to_owned(),
                fallback: true,
            })
        }
        Provider::Gemini => present(google_key).map(|api_key| TextBackend {
            api_key,
            model: text_model.to_owned(),
            fallback: false,
        }),
    }
}

/// Parse `ANTHROPIC_EFFORT`. Unset or unknown → `None`, which leaves the API default (`high`).
pub fn parse_effort(raw: Option<&str>) -> Option<Effort> {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("low") => Some(Effort::Low),
        Some("medium") => Some(Effort::Medium),
        Some("high") => Some(Effort::High),
        Some("xhigh") => Some(Effort::XHigh),
        Some("max") => Some(Effort::Max),
        _ => None,
    }
}

/// Build the model behind a text agent.
pub fn build(api_key: &str, model: &str) -> anyhow::Result<Arc<dyn Llm>> {
    match provider_for(model) {
        Provider::Anthropic => {
            // Thinking is left unset: Claude 5-generation models run adaptive thinking by
            // default and reject an explicit budget. Depth is tuned with effort only.
            let mut cfg = AnthropicConfig::new(api_key, model).with_max_tokens(ANTHROPIC_MAX_TOKENS);
            if let Some(effort) = parse_effort(std::env::var("ANTHROPIC_EFFORT").ok().as_deref()) {
                cfg = cfg.with_effort(effort);
            }
            Ok(Arc::new(AnthropicClient::new(cfg)?))
        }
        Provider::Gemini => Ok(Arc::new(GeminiModel::new(api_key, model)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_is_decided_by_model_id() {
        assert_eq!(provider_for("claude-fable-5-1"), Provider::Anthropic);
        assert_eq!(provider_for("claude-opus-5"), Provider::Anthropic);
        assert_eq!(provider_for("gemini-3.1-flash-lite"), Provider::Gemini);
        assert_eq!(provider_for("models/gemini-3.8-live"), Provider::Gemini);
    }

    #[test]
    fn claude_models_reject_sampling_gemini_accepts() {
        assert!(!allows_sampling("claude-fable-5-1"));
        assert!(allows_sampling("gemini-3.1-flash-lite"));
    }

    #[test]
    fn content_config_drops_sampling_for_claude_and_keeps_it_for_gemini() {
        let c = content_config("claude-fable-5-1", 0.35, 512);
        assert!(c.temperature.is_none() && c.top_p.is_none() && c.top_k.is_none());
        assert_eq!(c.max_output_tokens, Some(ANTHROPIC_MAX_TOKENS as i32));
        assert_eq!(content_config("claude-fable-5-1", 0.2, 32_768).max_output_tokens, Some(32_768));

        let g = content_config("gemini-3.1-flash-lite", 0.35, 512);
        assert_eq!(g.temperature, Some(0.35));
        assert_eq!(g.max_output_tokens, Some(512));
    }

    fn rust_sources(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let src = std::fs::read_to_string(&path).expect("read source");
                out.push((path.display().to_string(), src));
            }
        }
    }

    /// Agents are returned as `Arc<dyn Agent>`, so their config cannot be read back. Instead,
    /// every `LlmAgentBuilder` in `src/` must take its config from `content_config`; each call's
    /// arguments are then built for `claude-fable-5-1` and must carry no sampling parameters.
    #[test]
    fn every_agent_config_omits_sampling_for_claude_fable() {
        let mut files = Vec::new();
        rust_sources(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);

        let mut builders = 0;
        let mut calls: Vec<(String, f32, i32)> = Vec::new();
        for (path, src) in &files {
            if path.ends_with("src/llm.rs") {
                continue;
            }
            for needle in ["GenerateContentConfig {", ".temperature(", ".top_p(", ".top_k("] {
                assert!(!src.contains(needle), "{path} sets `{needle}` directly; use llm::content_config");
            }
            let built = src.matches("LlmAgentBuilder::new").count();
            let configured = src.matches(".generate_content_config(crate::llm::content_config(").count();
            assert_eq!(built, configured, "{path}: every LlmAgentBuilder takes llm::content_config");
            builders += built;
            for rest in src.split("crate::llm::content_config(").skip(1) {
                let args: Vec<&str> = rest.split(')').next().unwrap_or("").split(',').map(str::trim).collect();
                let temperature = args.get(1).and_then(|t| t.parse().ok()).unwrap_or_else(|| panic!("{path}: temperature literal"));
                let max_tokens = args.get(2).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("{path}: max_output_tokens literal"));
                calls.push((path.clone(), temperature, max_tokens));
            }
        }
        assert!(builders >= 20, "found only {builders} agent builders under src/");

        for (path, temperature, max_tokens) in &calls {
            let c = content_config("claude-fable-5-1", *temperature, *max_tokens);
            assert!(
                c.temperature.is_none() && c.top_p.is_none() && c.top_k.is_none(),
                "{path}: sampling sent to claude-fable-5-1"
            );
            assert!(c.max_output_tokens.unwrap_or(0) >= ANTHROPIC_MAX_TOKENS as i32, "{path}");
        }
    }

    #[test]
    fn effort_parses_case_insensitively_and_defaults_to_none() {
        assert!(matches!(parse_effort(Some("Medium")), Some(Effort::Medium)));
        assert!(matches!(parse_effort(Some(" xhigh ")), Some(Effort::XHigh)));
        assert!(parse_effort(Some("turbo")).is_none());
        assert!(parse_effort(None).is_none());
    }

    #[test]
    fn anthropic_key_selects_fable() {
        let b = select_text_backend("claude-fable-5-1", Some("sk-ant"), Some("goog"), "gemini-3.1-flash-lite")
            .expect("backend");
        assert_eq!(b.model, "claude-fable-5-1");
        assert_eq!(b.api_key, "sk-ant");
        assert!(!b.fallback);
    }

    #[test]
    fn missing_anthropic_key_falls_back_to_gemini_text_model() {
        let b = select_text_backend("claude-fable-5-1", None, Some("goog"), "gemini-3.1-flash-lite")
            .expect("backend");
        assert_eq!(b.model, "gemini-3.1-flash-lite");
        assert_eq!(b.api_key, "goog");
        assert!(b.fallback);
    }

    #[test]
    fn gemini_text_model_uses_google_key_only() {
        let b = select_text_backend("gemini-3.1-flash-lite", Some("sk-ant"), Some("goog"), "gemini-3.1-flash-lite")
            .expect("backend");
        assert_eq!(b.api_key, "goog");
        assert!(!b.fallback);
        assert!(select_text_backend("gemini-3.1-flash-lite", Some("sk-ant"), None, "gemini-3.1-flash-lite").is_none());
    }

    #[test]
    fn no_keys_means_mock_path() {
        assert!(select_text_backend("claude-fable-5-1", None, None, "gemini-3.1-flash-lite").is_none());
        assert!(select_text_backend("claude-fable-5-1", Some("  "), Some(""), "gemini-3.1-flash-lite").is_none());
    }

    #[test]
    fn build_constructs_both_backends_offline() {
        assert!(build("test-key", "claude-fable-5-1").is_ok());
        assert!(build("test-key", "gemini-3.1-flash-lite").is_ok());
    }
}
