//! WebSocket voice streaming — mic PCM in, Suzy PCM out; with the camera channel on, JSON
//! `frame` messages in (forwarded to the model, never stored or logged) and `ui_gesture` tool
//! calls out (M10-T5, `crate::voice::camera`).

use axum::{
    extract::{Query, State, WebSocketUpgrade, ws},
    response::IntoResponse,
    Json,
};
use base64::Engine as _;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use adk_realtime::events::ServerEvent;

use crate::state::AppState;

#[derive(Serialize)]
pub struct VoiceStatus {
    pub enabled: bool,
    pub ws_path: &'static str,
    pub input_rate_hz: u32,
    pub output_rate_hz: u32,
    /// Camera channel available on this websocket (`AGENTRIX_CAMERA`, needs voice).
    pub camera: bool,
}

pub async fn status(State(state): State<AppState>) -> Json<VoiceStatus> {
    Json(VoiceStatus {
        enabled: state.voice.enabled,
        ws_path: "/ws/voice",
        input_rate_hz: crate::voice::realtime::INPUT_RATE_HZ,
        output_rate_hz: crate::voice::realtime::OUTPUT_RATE_HZ,
        camera: state.voice.camera,
    })
}

#[derive(Deserialize)]
pub struct VoiceWsQuery {
    pub session_id: Option<String>,
}

pub async fn ws_voice(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<VoiceWsQuery>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_voice_ws(socket, state, query.session_id))
}

async fn handle_voice_ws(socket: ws::WebSocket, state: AppState, session_id: Option<String>) {
    if !state.voice.enabled {
        let mut socket = socket;
        let _ = socket
            .send(ws::Message::Text(
                serde_json::json!({
                    "type": "error",
                    "message": "Voice unavailable — set GOOGLE_API_KEY"
                })
                .to_string()
                .into(),
            ))
            .await;
        return;
    }

    let (mut ws_sender, mut ws_receiver) = socket.split();

    let runner = match crate::voice::realtime::build_suzy_runner(&state, session_id.clone()).await {
        Ok(r) => std::sync::Arc::new(r),
        Err(e) => {
            error!("voice runner init failed: {e:#}");
            let _ = ws_sender
                .send(ws::Message::Text(
                    serde_json::json!({"type": "error", "message": format!("Init failed: {e}")})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };

    if let Err(e) = runner.connect().await {
        error!("Gemini Live connect failed: {e}");
        let _ = ws_sender
            .send(ws::Message::Text(
                serde_json::json!({"type": "error", "message": format!("Connect failed: {e}")})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    }

    info!("Gemini Live voice session connected");
    // The UI session this voice session belongs to — the id the client keeps and submits intents
    // with. The realtime runner has an id of its own that is not a UI session; sending that as
    // `session_id` made every intent raised from voice (submit_intent, camera gestures) a 404.
    let ui_session_id = match session_id.as_deref() {
        Some(sid) if state.sessions.get(sid).await.is_some() => sid.to_string(),
        _ => state.sessions.create().await.session_id,
    };
    let _ = ws_sender
        .send(ws::Message::Text(
            serde_json::json!({
                "type": "connected",
                "session_id": ui_session_id,
                "runner_session_id": runner.session_id().await,
                "camera": state.voice.camera,
                // Negotiated like the mia example's `ready`: the client sizes its audio contexts
                // from these instead of assuming.
                "input_rate": crate::voice::realtime::INPUT_RATE_HZ,
                "output_rate": crate::voice::realtime::OUTPUT_RATE_HZ
            })
            .to_string()
            .into(),
        ))
        .await;

    let (tx, mut rx) = mpsc::channel::<ws::Message>(64);

    let runner_send = runner.clone();
    let mut frame_gate = crate::voice::camera::FrameGate::new(state.voice.camera);
    let tx_frames = tx.clone();
    let send_handle = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                ws::Message::Binary(data) => {
                    let audio_b64 =
                        base64::engine::general_purpose::STANDARD.encode(&data);
                    if runner_send.send_audio(&audio_b64).await.is_err() {
                        break;
                    }
                }
                ws::Message::Text(text) => {
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&text) {
                        match msg.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                if let Some(content) = msg.get("content").and_then(|c| c.as_str())
                                {
                                    let _ = runner_send.send_text(content).await;
                                    let _ = runner_send.create_response().await;
                                }
                            }
                            // Read-aloud (greeting, a summary): framed so the model says the
                            // words instead of answering them as a user turn.
                            Some("speak") => {
                                if let Some(content) = msg.get("content").and_then(|c| c.as_str())
                                    && !content.trim().is_empty()
                                {
                                    let prompt = crate::voice::realtime::read_aloud_prompt(content);
                                    let _ = runner_send.send_text(&prompt).await;
                                    let _ = runner_send.create_response().await;
                                }
                            }
                            Some("commit_audio") => {
                                let _ = runner_send.commit_audio().await;
                            }
                            Some("create_response") => {
                                let _ = runner_send.create_response().await;
                            }
                            Some("interrupt") => {
                                let _ = runner_send.interrupt().await;
                            }
                            Some("frame") => {
                                // Camera frame: admit, forward, forget. The payload is never logged.
                                let mime = msg.get("mime").and_then(|m| m.as_str()).unwrap_or("");
                                let data = msg.get("data").and_then(|d| d.as_str()).unwrap_or("");
                                match frame_gate.check(mime, data, std::time::Instant::now()) {
                                    Ok(()) => {
                                        if runner_send.send_video_frame(mime, data).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(crate::voice::camera::FrameReject::TooFast) => {}
                                    Err(reason) => {
                                        let _ = tx_frames
                                            .send(ws::Message::Text(
                                                serde_json::json!({"type": "frame_rejected", "reason": reason.as_str()})
                                                    .to_string()
                                                    .into(),
                                            ))
                                            .await;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                ws::Message::Close(_) => break,
                _ => {}
            }
        }
        if frame_gate.accepted + frame_gate.rejected > 0 {
            tracing::debug!(accepted = frame_gate.accepted, rejected = frame_gate.rejected, "camera frames relayed");
        }
        debug!("voice: client stopped sending");
    });

    let runner_recv = runner.clone();
    let recv_handle = tokio::spawn(async move {
        loop {
            match runner_recv.next_event().await {
                Some(Ok(event)) => {
                    // The pull-style `next_event` relays tool calls but does not run them (only
                    // `RealtimeRunner::run` and the integrated runner do). Without a function
                    // response Gemini waits forever and Suzy never speaks again — every turn
                    // after her first `submit_intent` came back as silence. Run the handler off
                    // this loop, like the runner's own dispatch, then trigger the follow-up
                    // response once the result is in.
                    if let Some((call_id, name, arguments)) = tool_call_to_dispatch(&event) {
                        let r = runner_recv.clone();
                        tokio::spawn(async move {
                            if let Err(e) = r.dispatch_tool_call(&call_id, &name, &arguments).await {
                                warn!(tool = %name, %call_id, "voice tool dispatch failed: {e}");
                            }
                            if let Err(e) = r.respond_after_tools().await {
                                warn!(tool = %name, "post-tool response trigger failed: {e}");
                            }
                        });
                    }
                    if matches!(event, ServerEvent::ResponseDone { .. })
                        && let Err(e) = runner_recv.respond_after_tools().await
                    {
                        warn!("post-tool response trigger failed: {e}");
                    }
                    let ws_msg = match &event {
                        ServerEvent::AudioDelta { delta, .. } => {
                            Some(ws::Message::Binary(delta.clone().into()))
                        }
                        ServerEvent::TextDelta { delta, .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "text_delta", "content": delta})
                                .to_string()
                                .into(),
                        )),
                        // Suzy's spoken words (output transcription).
                        ServerEvent::TranscriptDelta { delta, .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "transcript", "content": delta})
                                .to_string()
                                .into(),
                        )),
                        // The user's words (input transcription) — Gemini streams deltas; the
                        // client coalesces them and fills the intent bar.
                        ServerEvent::InputTranscriptDelta { delta, .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "user_transcript_delta", "content": delta})
                                .to_string()
                                .into(),
                        )),
                        ServerEvent::InputTranscriptCompleted { transcript, .. } => {
                            Some(ws::Message::Text(
                                serde_json::json!({"type": "user_transcript", "content": transcript})
                                    .to_string()
                                    .into(),
                            ))
                        }
                        ServerEvent::SpeechStarted { .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "speech_started"})
                                .to_string()
                                .into(),
                        )),
                        ServerEvent::SpeechStopped { .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "speech_stopped"})
                                .to_string()
                                .into(),
                        )),
                        ServerEvent::ResponseDone { .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "response_done"})
                                .to_string()
                                .into(),
                        )),
                        ServerEvent::FunctionCallDone { name, arguments, .. } => Some(
                            ws::Message::Text(
                                serde_json::json!({
                                    "type": "tool_call",
                                    "name": name,
                                    "arguments": arguments
                                })
                                .to_string()
                                .into(),
                            ),
                        ),
                        ServerEvent::Error { error, .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "error", "message": error.message})
                                .to_string()
                                .into(),
                        )),
                        _ => None,
                    };

                    if let Some(msg) = ws_msg {
                        if tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
                Some(Err(e)) => {
                    warn!("Gemini voice stream error: {e}");
                    break;
                }
                None => {
                    let reason = runner_recv.disconnect_reason().await;
                    debug!(?reason, "voice: Gemini event stream ended");
                    break;
                }
            }
        }
    });

    let forward_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = send_handle => debug!("voice session ending: client stopped sending"),
        _ = recv_handle => debug!("voice session ending: Gemini stream ended"),
        _ = forward_handle => debug!("voice session ending: client sink closed"),
    }

    let _ = runner.close().await;
    info!("voice websocket session closed");
}

/// A tool call the route must answer itself: `(call_id, name, arguments)`.
fn tool_call_to_dispatch(event: &ServerEvent) -> Option<(String, String, String)> {
    match event {
        ServerEvent::FunctionCallDone { call_id, name, arguments, .. } => {
            Some((call_id.clone(), name.clone(), arguments.clone()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_function_calls_are_dispatched() {
        let call = ServerEvent::FunctionCallDone {
            event_id: "e".into(),
            response_id: String::new(),
            item_id: String::new(),
            output_index: 0,
            call_id: "call_1".into(),
            name: "submit_intent".into(),
            arguments: r#"{"text":"brief me"}"#.into(),
        };
        assert_eq!(
            tool_call_to_dispatch(&call),
            Some(("call_1".into(), "submit_intent".into(), r#"{"text":"brief me"}"#.into()))
        );
        let done = ServerEvent::ResponseDone { event_id: "e".into(), response: serde_json::json!({}) };
        assert_eq!(tool_call_to_dispatch(&done), None);
    }
}
