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
use tracing::{error, info, warn};

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
        input_rate_hz: 16_000,
        output_rate_hz: 24_000,
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

    let runner = match crate::voice::realtime::build_suzy_runner(&state, session_id).await {
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
    let _ = ws_sender
        .send(ws::Message::Text(
            serde_json::json!({
                "type": "connected",
                "session_id": runner.session_id().await,
                "camera": state.voice.camera
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
    });

    let runner_recv = runner.clone();
    let recv_handle = tokio::spawn(async move {
        loop {
            match runner_recv.next_event().await {
                Some(Ok(event)) => {
                    let ws_msg = match &event {
                        ServerEvent::AudioDelta { delta, .. } => {
                            Some(ws::Message::Binary(delta.clone().into()))
                        }
                        ServerEvent::TextDelta { delta, .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "text_delta", "content": delta})
                                .to_string()
                                .into(),
                        )),
                        ServerEvent::TranscriptDelta { delta, .. } => Some(ws::Message::Text(
                            serde_json::json!({"type": "transcript", "content": delta})
                                .to_string()
                                .into(),
                        )),
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
                None => break,
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
        _ = send_handle => {}
        _ = recv_handle => {}
        _ = forward_handle => {}
    }

    let _ = runner.close().await;
    info!("voice websocket session closed");
}