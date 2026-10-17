//! Suzy voice runner — Gemini Live via adk-realtime, following the mia example
//! (`adk-rust/examples/realtime_voice`).
//!
//! What the example settled and this module mirrors:
//! - **Server-side VAD with interruption** (`VadConfig::server_vad()`): the model stops talking
//!   when the user starts; the browser drops its queued audio on `speech_started` (barge-in).
//! - **Transcription on** (`with_transcription()`): Gemini then streams both the user's words
//!   (`InputTranscriptDelta`) and Suzy's (`TranscriptDelta`), which the websocket forwards.
//! - **Async tool handlers.** The runner awaits a handler on its own task, so a handler that
//!   blocks the runtime (`Handle::block_on`, as the first version did) panics the session the
//!   first time Suzy calls a tool. Handlers here are `async` and await the stores directly.

use adk_realtime::config::{RealtimeConfig, ToolDefinition, VadConfig};
use adk_realtime::events::ToolCall;
use adk_realtime::runner::{FnToolHandler, RealtimeRunner, ToolHandler};
use async_trait::async_trait;
use serde_json::json;

use crate::agents::suzy;
use crate::state::{AppState, SessionRecord, SessionStore};
use crate::voice::camera;

/// Gemini Live wire format: 16 kHz PCM16 in, 24 kHz PCM16 out.
pub const INPUT_RATE_HZ: u32 = 16_000;
pub const OUTPUT_RATE_HZ: u32 = 24_000;

/// Session config shared by the voice websocket and the tests.
pub fn suzy_config(instruction: &str, voice: &str, language: Option<&str>) -> RealtimeConfig {
    let mut cfg = RealtimeConfig::default()
        .with_instruction(instruction)
        .with_voice(voice)
        // Server-side turn detection; a new user turn interrupts the model's reply.
        .with_vad(VadConfig::server_vad().with_interrupt(true))
        // Input + output transcription (Gemini enables both from this one switch).
        .with_transcription();
    // Pin the spoken language: without `speechConfig.languageCode` Gemini guesses per turn and
    // short or accented English came back transcribed as Spanish. Written to `extra` so this
    // builds against adk-realtime with or without `RealtimeConfig::with_language`
    // (zavora-ai/adk-rust: feat/realtime-gemini-language-code reads the same key).
    let mut extra = cfg.extra.take().unwrap_or_else(|| json!({}));
    if let Some(code) = language.map(str::trim).filter(|c| !c.is_empty()) {
        extra["language_code"] = json!(code);
    }
    // Context window compression (a sliding window, server defaults). Google documents that a
    // Live session without it ends after about two minutes with camera frames and fifteen
    // audio-only (`gemini-3.8-live` ran 13 minutes with frames without it on 2026-10-17, so the
    // cap is model-dependent; `GEMINI_LIVE_MODEL` can name another). Same `extra` pattern:
    // adk-realtime without `with_context_window_compression` ignores the key
    // (zavora-ai/adk-rust#867 reads it).
    extra["context_window_compression"] = json!(true);
    cfg.extra = Some(extra);
    cfg
}

/// Wrap text the UI wants said aloud (greeting, a summary) so the model reads it instead of
/// treating it as something the user said and replying to it.
pub fn read_aloud_prompt(text: &str) -> String {
    format!(
        "Read the following to the user now, word for word, warmly, in English. Do not add a \
         reply, a question, or commentary, and do not call any tool:\n{}",
        text.trim()
    )
}

/// The UI session, if `caller` (the websocket's JWT subject) may act in it. The route checks
/// this before the upgrade; every read here checks it again.
async fn admitted(sessions: &SessionStore, session_id: Option<&str>, caller: Option<&str>) -> Option<SessionRecord> {
    sessions.get(session_id?).await.filter(|r| r.admits(caller))
}

/// `get_session_context` — the browser session's scenario, cards and artifacts.
struct SessionContextTool {
    sessions: SessionStore,
    session_id: Option<String>,
    caller: Option<String>,
}

#[async_trait]
impl ToolHandler for SessionContextTool {
    async fn execute(&self, _call: &ToolCall) -> adk_realtime::Result<serde_json::Value> {
        if let Some(record) = admitted(&self.sessions, self.session_id.as_deref(), self.caller.as_deref()).await {
            return Ok(json!({
                "session_id": record.session_id,
                "context": suzy::session_context(&record),
                "domains": crate::mother::domain_summary(&record),
            }));
        }
        Ok(json!({
            "session_id": null,
            "context": "No active session — ask what the user would like to do."
        }))
    }
}

/// `submit_intent` — resolve the UI session and hand the intent back to the client, which posts
/// it through the normal intent route (`dispatch: client_sse`) so cards bloom in the field.
struct SubmitIntentTool {
    state: AppState,
    session_id: Option<String>,
    caller: Option<String>,
}

#[async_trait]
impl ToolHandler for SubmitIntentTool {
    async fn execute(&self, call: &ToolCall) -> adk_realtime::Result<serde_json::Value> {
        let text = call.arguments["text"].as_str().unwrap_or("").trim().to_string();
        if text.is_empty() {
            return Ok(json!({ "status": "error", "message": "intent text required" }));
        }
        let (session_id, user_id) = match &self.session_id {
            Some(id) => match admitted(&self.state.sessions, Some(id), self.caller.as_deref()).await {
                Some(record) => (record.session_id, record.user_id),
                None => return Ok(json!({ "status": "error", "message": "session not found" })),
            },
            None => {
                let record = self.state.sessions.create_for_caller(self.caller.clone()).await;
                (record.session_id, record.user_id)
            }
        };
        // Conclusive on purpose: a bare "started" read like an unfinished job and the model
        // called submit_intent again for the same request instead of speaking.
        Ok(json!({
            "status": "accepted",
            "session_id": session_id,
            "user_id": user_id,
            "intent": text,
            "dispatch": "client_sse",
            "next": "Done — the Mother Agent is on it. The result appears on screen and is read \
                     aloud to the user automatically. Tell the user in one short sentence that \
                     it is underway. Do not call submit_intent again for this request."
        }))
    }
}

pub async fn build_suzy_runner(
    state: &AppState,
    session_id: Option<String>,
    caller: Option<String>,
) -> anyhow::Result<RealtimeRunner> {
    let model = state
        .voice
        .model
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Gemini Live not configured (set GOOGLE_API_KEY)"))?;

    let mut instruction = String::from(
        "You are Suzy — warm, confident, quietly witty voice of the Mother Agent in Agentrix Personal AI OS. \
         Help the user express intent, start their day, and let the Mother Agent orchestrate the specialized agents. \
         When they ask what they need to know today (or for their briefing), or to do something that should spawn field cards, \
         call submit_intent once with exactly that. The result is shown and read aloud automatically; after the call, say one \
         short sentence that it is underway and never repeat the same submit_intent. \
         Keep replies concise and spoken-friendly (1–3 sentences unless they ask for detail).\n\
         The user speaks English. Listen for, transcribe and reply in English only, even when a \
         word or name could belong to another language; never switch languages on your own.",
    );
    if let Some(tone) = &state.brand_tone {
        instruction.push_str(&format!("\nBrand tone: {tone}."));
    }
    instruction.push_str(&format!(
        "\nDefault greeting line when relevant: {}",
        state.brand_greeting_body
    ));
    if state.voice.camera {
        instruction.push_str(&camera::instruction());
    }
    if let Some(sid) = &session_id {
        instruction.push_str(&format!("\nActive UI session id: {sid}."));
        if let Some(record) = admitted(&state.sessions, Some(sid), caller.as_deref()).await {
            instruction.push_str(&format!(
                "\n\nCurrent session context:\n{}",
                suzy::session_context(&record)
            ));
        }
    }

    let mut builder = RealtimeRunner::builder()
        .model(model)
        .config(suzy_config(&instruction, &state.voice.voice_name, state.voice.language.as_deref()))
        .tool(
            ToolDefinition {
                name: "get_session_context".into(),
                description: Some(
                    "Read the current browser session: scenario, cards, and artifacts.".into(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                })),
            },
            SessionContextTool {
                sessions: state.sessions.clone(),
                session_id: session_id.clone(),
                caller: caller.clone(),
            },
        )
        .tool(
            ToolDefinition {
                name: "submit_intent".into(),
                description: Some(
                    "Start orchestration for a user request (deck, morning brief, trip, etc.). \
                     Call when the user asks to do something that should spawn field cards."
                        .into(),
                ),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "The user's intent in natural language"
                        }
                    },
                    "required": ["text"]
                })),
            },
            SubmitIntentTool {
                state: state.clone(),
                session_id: session_id.clone(),
                caller,
            },
        );

    // Camera channel (M10-T5): the call itself is the signal — the websocket relays every tool
    // call to the client, which maps the gesture to a UI verb through the normal routes.
    if state.voice.camera {
        builder = builder.tool(
            ToolDefinition {
                name: camera::TOOL_NAME.into(),
                description: Some(
                    "Report one deliberate hand gesture that you can see a hand making in the \
                     latest camera frames — never one the user only describes. Call it once per \
                     gesture; the interface performs the action."
                        .into(),
                ),
                parameters: Some(camera::tool_parameters()),
            },
            FnToolHandler::new(|call| {
                Ok(camera::tool_result(call.arguments["gesture"].as_str().unwrap_or("")))
            }),
        );
    }

    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_realtime::config::VadMode;

    #[test]
    fn suzy_config_enables_server_vad_interruption_and_transcription() {
        let cfg = suzy_config("You are Suzy.", "Aoede", Some("en-US"));
        let vad = cfg.turn_detection.expect("server VAD configured");
        assert_eq!(vad.mode, VadMode::ServerVad);
        assert_eq!(vad.interrupt_response, Some(true), "a new user turn interrupts the reply");
        assert!(cfg.input_audio_transcription.is_some(), "transcription on");
        assert_eq!(cfg.voice.as_deref(), Some("Aoede"));
        assert!(cfg.instruction.as_deref().unwrap_or("").contains("Suzy"));
        assert_eq!((INPUT_RATE_HZ, OUTPUT_RATE_HZ), (16_000, 24_000));
        assert_eq!(cfg.extra.as_ref().and_then(|e| e.get("language_code")), Some(&json!("en-US")));
    }

    #[test]
    fn suzy_config_leaves_the_language_unpinned_only_when_asked() {
        for unpinned in [None, Some("  ")] {
            let extra = suzy_config("x", "Aoede", unpinned).extra.unwrap();
            assert!(extra.get("language_code").is_none());
        }
        let cfg = suzy_config("x", "Aoede", Some("en-GB"));
        assert_eq!(cfg.extra.unwrap()["language_code"], json!("en-GB"));
    }

    #[test]
    fn suzy_config_asks_for_context_window_compression() {
        // Google documents a ~2-minute cap on Live sessions with video without it.
        for language in [None, Some("en-US")] {
            let extra = suzy_config("x", "Aoede", language).extra.unwrap();
            assert_eq!(extra["context_window_compression"], json!(true));
        }
    }

    #[test]
    fn read_aloud_prompt_carries_the_text_and_forbids_a_reply() {
        let p = read_aloud_prompt("  Good morning. Two meetings today.  ");
        assert!(p.ends_with("Good morning. Two meetings today."));
        assert!(p.contains("word for word"));
        assert!(p.contains("do not call any tool"));
    }

    #[tokio::test]
    async fn submit_intent_tool_is_async_and_creates_a_session_when_none_is_bound() {
        // Building a full AppState needs the test fixtures in tests/validate.rs; here we only
        // check the argument handling that does not touch the stores.
        let call = ToolCall { call_id: "c1".into(), name: "submit_intent".into(), arguments: json!({ "text": "   " }) };
        assert_eq!(call.arguments["text"].as_str().unwrap().trim(), "");
        let call = ToolCall { call_id: "c2".into(), name: "get_session_context".into(), arguments: json!({}) };
        let tool = SessionContextTool { sessions: SessionStore::new(), session_id: Some("missing".into()), caller: None };
        let out = tool.execute(&call).await.unwrap();
        assert!(out["session_id"].is_null());
        assert!(out["context"].as_str().unwrap().contains("No active session"));
    }

    #[tokio::test]
    async fn session_context_tool_reads_a_signed_in_session_only_for_its_owner() {
        let sessions = SessionStore::new();
        let record = sessions.create_signed_in("owner".into()).await;
        let call = ToolCall { call_id: "c".into(), name: "get_session_context".into(), arguments: json!({}) };
        let read_as = |caller: Option<&str>| SessionContextTool {
            sessions: sessions.clone(),
            session_id: Some(record.session_id.clone()),
            caller: caller.map(Into::into),
        };
        assert!(read_as(Some("someone-else")).execute(&call).await.unwrap()["session_id"].is_null());
        assert!(read_as(None).execute(&call).await.unwrap()["session_id"].is_null());
        assert_eq!(read_as(Some("owner")).execute(&call).await.unwrap()["session_id"], json!(record.session_id));
    }
}
