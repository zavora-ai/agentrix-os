use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::State,
    http::HeaderMap,
    response::sse::{Event, Sse},
    response::Response,
    Json,
};
use serde::Deserialize;
use tokio_stream::wrappers::ReceiverStream;

use crate::ambient::AmbientEvent;
use crate::state::AppState;

pub async fn list_ambient(State(state): State<AppState>) -> Json<serde_json::Value> {
    let (dnd, agents) = state.ambient.snapshot().await;
    Json(serde_json::json!({ "dnd": dnd, "agents": agents }))
}

#[derive(Deserialize)]
pub struct DndRequest {
    pub enabled: bool,
}

pub async fn set_dnd(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DndRequest>,
) -> Result<Json<serde_json::Value>, Response> {
    state.awp.check(&headers, "dnd", "set_dnd").await?;
    state.ambient.set_dnd(body.enabled).await;
    Ok(Json(serde_json::json!({ "dnd": body.enabled })))
}

pub async fn stream_ambient(
    State(state): State<AppState>,
) -> Sse<ReceiverStream<Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let store = state.ambient.clone();
    let mut sub = store.subscribe();

    tokio::spawn(async move {
        let (dnd, agents) = store.snapshot().await;
        let snap = AmbientEvent::Snapshot { dnd, agents };
        if tx
            .send(Ok(Event::default().data(serde_json::to_string(&snap).unwrap_or_default())))
            .await
            .is_err()
        {
            return;
        }

        loop {
            tokio::select! {
                msg = sub.recv() => {
                    match msg {
                        Ok(ev) => {
                            if tx.send(Ok(Event::default().data(
                                serde_json::to_string(&ev).unwrap_or_default()
                            ))).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(25)) => {
                    if tx.send(Ok(Event::default().comment("keepalive"))).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
}