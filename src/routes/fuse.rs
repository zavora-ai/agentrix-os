use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    response::sse::Sse,
    Json,
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::events::sse::{to_event, ConductStep, FieldEvent};
use crate::routes::actions::owned_session;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct FuseRequest {
    pub source: String,
    pub target: String,
}

pub async fn fuse_cards(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(body): Json<FuseRequest>,
) -> Response {
    if body.source.trim().is_empty() || body.target.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "source and target required").into_response();
    }

    let client_key = format!("fuse:{session_id}");
    if let Err(resp) = state
        .awp
        .check(&headers, &client_key, "fuse_cards")
        .await
    {
        return resp;
    }

    if let Err(resp) = owned_session(&state, &headers, &session_id).await {
        return resp;
    }

    let (tx, rx) = mpsc::channel::<Result<axum::response::sse::Event, Infallible>>(8);
    let source = body.source;
    let target = body.target;

    tokio::spawn(async move {
        let _ = tx
            .send(Ok(to_event(&FieldEvent::Conduct {
                steps: vec![ConductStep {
                    op: "fuse".into(),
                    source: Some(source),
                    target: Some(target),
                    delay_ms: None,
                }],
            })))
            .await;
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    Sse::new(ReceiverStream::new(rx)).into_response()
}