use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde::Serialize;

use crate::routes::actions::owned_session;
use crate::state::{AppState, CardRecord};

#[derive(Serialize)]
pub struct CardsResponse {
    pub session_id: String,
    pub scenario: Option<String>,
    pub origin_text: Option<String>,
    pub cards: Vec<CardRecord>,
}

pub async fn list_cards(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<CardsResponse>, Response> {
    let client_key = format!("cards:{session_id}");
    state
        .awp
        .check(&headers, &client_key, "list_cards")
        .await
        .map_err(|r| r)?;

    let record = owned_session(&state, &headers, &session_id).await?;
    let cards = record
        .cards
        .into_iter()
        .filter(|c| !c.removed)
        .collect();
    Ok(Json(CardsResponse {
        session_id: record.session_id,
        scenario: record.scenario,
        origin_text: record.origin_text,
        cards,
    }))
}