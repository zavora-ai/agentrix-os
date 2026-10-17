//! Whether Gemini Live can actually serve voice, and why a live session ended.
//!
//! A key that is present is not a key that works: with a revoked key `/api/voice/status` said
//! `enabled: true`, every Live session died within a second (close 1008 "API key not valid"),
//! and the user saw nothing. The boot check below asks Gemini once (`models.list`, one row) and
//! a live session that is rejected for its key flips the same switch, so the UI hides the mic
//! instead of failing on click. Offline or a slow network leaves voice on: the check never
//! blocks boot and never guesses that a key it could not verify is bad.

use std::time::Duration;

const MODELS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models?pageSize=1";
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Outcome of the boot-time key check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCheck {
    Valid,
    /// Gemini answered and refused the key; voice stays off until the key is fixed.
    Rejected(String),
    /// No usable answer (offline, timeout, quota, server error); voice stays on.
    Unverified(String),
}

/// Classify the `models.list` answer. 400 counts only when Gemini names the key (it answers a
/// bad key with 400 `API_KEY_INVALID`); 401/403 are a key without access.
pub fn classify_key_check(status: u16, body: &str) -> KeyCheck {
    match status {
        200..=299 => KeyCheck::Valid,
        400 if mentions_api_key(body) => KeyCheck::Rejected(provider_message(body, status)),
        401 | 403 => KeyCheck::Rejected(provider_message(body, status)),
        _ => KeyCheck::Unverified(provider_message(body, status)),
    }
}

/// Ask Gemini once whether `api_key` works. The key goes in a header, never in the URL.
pub async fn check_key(api_key: &str) -> KeyCheck {
    let client = match reqwest::Client::builder().timeout(CHECK_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => return KeyCheck::Unverified(format!("http client: {e}")),
    };
    match client.get(MODELS_URL).header("x-goog-api-key", api_key).send().await {
        Ok(res) => {
            let status = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            classify_key_check(status, &body)
        }
        Err(e) if e.is_timeout() => KeyCheck::Unverified("Gemini did not answer within 5 s".into()),
        Err(e) if e.is_connect() => KeyCheck::Unverified("could not reach Gemini (offline?)".into()),
        Err(e) => KeyCheck::Unverified(format!("request failed: {e}")),
    }
}

/// Why a live session ended, in the shape the websocket sends the client before closing:
/// `{type: "closed", reason, message}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnd {
    /// `key_rejected`, `session_limit` or `gemini_closed`.
    pub reason: &'static str,
    /// One sentence for the toast. Provider text is quoted, never interpreted.
    pub message: String,
}

impl SessionEnd {
    pub fn key_rejected(&self) -> bool {
        self.reason == "key_rejected"
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "type": "closed", "reason": self.reason, "message": self.message })
    }
}

/// Classify the close Gemini sent (`code`, `reason` from adk-realtime's `DisconnectReason`).
/// 1008 + "API key" is a rejected key; a session-length cap (documented: about ten minutes per
/// connection, and without context window compression two with video, fifteen audio-only)
/// arrives as a deadline / limit close.
pub fn classify_disconnect(code: Option<u16>, reason: &str) -> SessionEnd {
    let r = reason.to_ascii_lowercase();
    if mentions_api_key(reason) || r.contains("permission denied") {
        return SessionEnd {
            reason: "key_rejected",
            message: "Voice is off — Gemini rejected the API key. Fix GOOGLE_API_KEY and restart the server.".into(),
        };
    }
    let limit = ["deadline", "limit", "duration", "exceeded", "expired", "too long"];
    if code == Some(1000) || limit.iter().any(|w| r.contains(w)) {
        return SessionEnd {
            reason: "session_limit",
            message: "Suzy's live session reached its time limit — tap 🎤 or 📷 to start a new one.".into(),
        };
    }
    let detail = match (code, reason.trim()) {
        (Some(c), "") => format!(" (code {c})"),
        (Some(c), t) => format!(" (code {c}: {})", short(t)),
        (None, "") => String::new(),
        (None, t) => format!(" ({})", short(t)),
    };
    SessionEnd {
        reason: "gemini_closed",
        message: format!("Suzy's live session ended{detail}. Tap 🎤 or 📷 to reconnect."),
    }
}

fn mentions_api_key(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("api key") || t.contains("api_key")
}

/// Gemini's `error.message` when the body has one, else the status.
fn provider_message(body: &str, status: u16) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(short))
        .unwrap_or_else(|| format!("HTTP {status}"))
}

fn short(text: &str) -> String {
    let t = text.trim();
    match t.char_indices().nth(140) {
        Some((i, _)) => format!("{}…", &t[..i]),
        None => t.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAD_KEY: &str = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#;

    #[test]
    fn key_check_rejects_only_what_gemini_refuses() {
        assert_eq!(classify_key_check(200, "{}"), KeyCheck::Valid);
        assert_eq!(
            classify_key_check(400, BAD_KEY),
            KeyCheck::Rejected("API key not valid. Please pass a valid API key.".into())
        );
        assert!(matches!(classify_key_check(403, "{}"), KeyCheck::Rejected(m) if m == "HTTP 403"));
        // A 400 that is not about the key, quota and server errors leave voice on.
        assert!(matches!(classify_key_check(400, r#"{"error":{"message":"bad pageSize"}}"#), KeyCheck::Unverified(_)));
        assert!(matches!(classify_key_check(429, ""), KeyCheck::Unverified(_)));
        assert!(matches!(classify_key_check(503, "<html>"), KeyCheck::Unverified(m) if m == "HTTP 503"));
    }

    #[test]
    fn disconnects_are_classified_for_the_client() {
        let key = classify_disconnect(Some(1008), "API key not valid. Please pass a valid API key.");
        assert!(key.key_rejected());
        assert_eq!(key.to_json()["type"], "closed");
        assert_eq!(key.to_json()["reason"], "key_rejected");

        assert_eq!(classify_disconnect(Some(1011), "Deadline expired before operation could complete.").reason, "session_limit");
        assert_eq!(classify_disconnect(Some(1000), "").reason, "session_limit");

        let other = classify_disconnect(Some(1011), "Internal error occurred.");
        assert_eq!(other.reason, "gemini_closed");
        assert!(other.message.contains("code 1011: Internal error occurred."));
        assert_eq!(classify_disconnect(None, "").message, "Suzy's live session ended. Tap 🎤 or 📷 to reconnect.");
    }

    #[test]
    fn provider_text_is_shortened() {
        let long = "x".repeat(400);
        let end = classify_disconnect(Some(1011), &long);
        assert!(end.message.chars().count() < 220);
        assert!(end.message.contains('…'));
    }
}
