//! WebSocket voice streaming — mic PCM in, Suzy PCM out; with the camera channel on, JSON
//! `frame` messages in (forwarded to the model, never stored or logged) and `ui_gesture` tool
//! calls out (M10-T5, `crate::voice::camera`).

use axum::{
    extract::{Query, State, WebSocketUpgrade, ws},
    http::HeaderMap,
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
use crate::voice::health::{SessionEnd, classify_disconnect};

#[derive(Serialize)]
pub struct VoiceStatus {
    /// A Live session can open: a key is configured and Gemini has not rejected it.
    pub enabled: bool,
    /// Why voice is off, when it is (no key, or the key was rejected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub ws_path: &'static str,
    pub input_rate_hz: u32,
    pub output_rate_hz: u32,
    /// Camera channel available on this websocket (`AGENTRIX_CAMERA`, needs voice).
    pub camera: bool,
    /// Camera button for local gesture detection in the browser (`AGENTRIX_CAMERA`; no voice needed).
    pub gestures: bool,
}

pub async fn status(State(state): State<AppState>) -> Json<VoiceStatus> {
    let enabled = state.voice.available();
    Json(VoiceStatus {
        enabled,
        reason: state.voice.unavailable_reason(),
        ws_path: "/ws/voice",
        input_rate_hz: crate::voice::realtime::INPUT_RATE_HZ,
        output_rate_hz: crate::voice::realtime::OUTPUT_RATE_HZ,
        camera: enabled && state.voice.camera,
        gestures: state.voice.gestures,
    })
}

#[derive(Deserialize)]
pub struct VoiceWsQuery {
    pub session_id: Option<String>,
}

pub async fn ws_voice(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<VoiceWsQuery>,
) -> impl IntoResponse {
    let user = state
        .auth
        .as_ref()
        .and_then(|a| crate::auth::extract_user_id(&headers, &a.jwt_secret))
        .map(|u| u.to_string());
    ws.on_upgrade(move |socket| handle_voice_ws(socket, state, query.session_id, user))
}

/// The UI session this voice session belongs to — the id the client keeps and submits intents
/// with. The page's own session when it passes one that exists and is not someone else's
/// (signed in: it must be the JWT user's); otherwise a new one for the JWT user, created the
/// way `POST /api/sessions` does. A random owner here made wave → briefing and every later
/// intent run as a stranger.
pub async fn resolve_ui_session(state: &AppState, session_id: Option<&str>, user: Option<String>) -> String {
    if let Some(sid) = session_id
        && let Some(record) = state.sessions.get(sid).await
        && user.as_deref().is_none_or(|u| u == record.user_id)
    {
        return record.session_id;
    }
    crate::routes::session::create_ui_session(state, user).await.session_id
}

/// Tell the client why there is no session, then close: `{type: "error", reason, message}`.
async fn refuse<S>(sender: &mut S, reason: &str, message: &str)
where
    S: futures::Sink<ws::Message> + Unpin,
{
    let _ = sender.send(error_message(reason, message)).await;
    let _ = sender.send(close_frame(1011, reason)).await;
}

fn error_message(reason: &str, message: &str) -> ws::Message {
    ws::Message::Text(
        serde_json::json!({"type": "error", "reason": reason, "message": message})
            .to_string()
            .into(),
    )
}

fn close_frame(code: u16, reason: &str) -> ws::Message {
    ws::Message::Close(Some(ws::CloseFrame { code, reason: reason.to_string().into() }))
}

async fn handle_voice_ws(
    socket: ws::WebSocket,
    state: AppState,
    session_id: Option<String>,
    user: Option<String>,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    if let Some(why) = state.voice.unavailable_reason() {
        refuse(&mut ws_sender, "voice_unavailable", &why).await;
        return;
    }

    // Resolved before the runner is built, so Suzy's tools (submit_intent, session context)
    // are bound to this session instead of minting one per call. The runner's own id is not a
    // UI session: sending that as `session_id` made every intent raised from voice a 404.
    let ui_session_id = resolve_ui_session(&state, session_id.as_deref(), user).await;

    let runner = match crate::voice::realtime::build_suzy_runner(&state, Some(ui_session_id.clone())).await {
        Ok(r) => std::sync::Arc::new(r),
        Err(e) => {
            error!("voice runner init failed: {e:#}");
            refuse(&mut ws_sender, "init_failed", &format!("Voice could not start: {e}")).await;
            return;
        }
    };

    if let Err(e) = runner.connect().await {
        error!("Gemini Live connect failed: {e}");
        let end = classify_disconnect(None, &e.to_string());
        if end.key_rejected() {
            state.voice.mark_unavailable(end.message.clone());
            refuse(&mut ws_sender, end.reason, &end.message).await;
        } else {
            refuse(&mut ws_sender, "connect_failed", &format!("Could not reach Gemini Live: {e}")).await;
        }
        return;
    }

    info!("Gemini Live voice session connected");
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
    let tx_end = tx.clone();
    // Each task ends with `Some(detail)` when the Gemini side failed, `None` when the client
    // went away. A failed send used to be ignored, so the client kept an open socket to a
    // session that no longer existed.
    let mut send_handle = tokio::spawn(async move {
        let mut gemini_failed: Option<String> = None;
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                ws::Message::Binary(data) => {
                    let audio_b64 =
                        base64::engine::general_purpose::STANDARD.encode(&data);
                    if let Err(e) = runner_send.send_audio(&audio_b64).await {
                        gemini_failed = Some(e.to_string());
                        break;
                    }
                }
                ws::Message::Text(text) => {
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&text) {
                        match msg.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                if let Some(content) = msg.get("content").and_then(|c| c.as_str())
                                {
                                    if let Err(e) = runner_send.send_text(content).await {
                                        gemini_failed = Some(e.to_string());
                                        break;
                                    }
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
                                    if let Err(e) = runner_send.send_text(&prompt).await {
                                        gemini_failed = Some(e.to_string());
                                        break;
                                    }
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
                                        if let Err(e) = runner_send.send_video_frame(mime, data).await {
                                            gemini_failed = Some(e.to_string());
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
        gemini_failed
    });

    let runner_recv = runner.clone();
    let mut recv_handle = tokio::spawn(async move {
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

                    if let Some(msg) = ws_msg
                        && tx.send(msg).await.is_err()
                    {
                        return None;
                    }
                }
                Some(Err(e)) => {
                    warn!("Gemini voice stream error: {e}");
                    return Some(e.to_string());
                }
                None => {
                    let reason = runner_recv.disconnect_reason().await;
                    debug!(?reason, "voice: Gemini event stream ended");
                    return Some(String::new());
                }
            }
        }
    });

    let mut forward_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let closing = matches!(msg, ws::Message::Close(_));
            if ws_sender.send(msg).await.is_err() || closing {
                break;
            }
        }
    });

    let gemini_end = tokio::select! {
        r = &mut send_handle => { debug!("voice session ending: client stopped sending"); r.ok().flatten() }
        r = &mut recv_handle => { debug!("voice session ending: Gemini stream ended"); r.ok().flatten() }
        _ = &mut forward_handle => { debug!("voice session ending: client sink closed"); None }
    };

    if let Some(detail) = gemini_end {
        // A failed send can beat the close frame to us; give the event stream a moment to
        // record why Gemini hung up before asking.
        if !recv_handle.is_finished() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), &mut recv_handle).await;
        }
        let end: SessionEnd = match runner.disconnect_reason().await {
            Some(d) => classify_disconnect(d.code, &d.reason),
            None => classify_disconnect(None, &detail),
        };
        warn!(reason = end.reason, "voice: Gemini ended the live session");
        if end.key_rejected() {
            state.voice.mark_unavailable(end.message.clone());
        }
        let _ = tx_end.send(ws::Message::Text(end.to_json().to_string().into())).await;
        let code = if end.reason == "session_limit" { 1000 } else { 1011 };
        let _ = tx_end.send(close_frame(code, end.reason)).await;
    }

    // Stop both halves so neither keeps the client socket alive, then let the forwarder flush
    // what is queued (the reason above) before it ends with the last sender.
    send_handle.abort();
    recv_handle.abort();
    drop(tx_end);
    // (A finished JoinHandle must not be polled again.)
    if !forward_handle.is_finished()
        && tokio::time::timeout(std::time::Duration::from_secs(2), &mut forward_handle).await.is_err()
    {
        forward_handle.abort();
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
