use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use uuid::Uuid;

use adk_awp::error_response::AwpErrorResponse;
use adk_awp::{EventSubscription, EventSubscriptionService};

use crate::awp_gate;
use crate::state::AppState;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSubscriptionRequest {
    pub subscriber: String,
    pub callback_url: String,
    pub event_types: Vec<String>,
    pub secret: String,
}

/// `POST /awp/events/subscribe` — gated to `known` trust (JWT or Bearer).
pub async fn subscribe(
    State(app): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CreateSubscriptionRequest>,
) -> Response {
    let key = awp_gate::client_key(&headers, "awp-subscribe");
    if let Err(resp) = app
        .awp
        .check(&headers, &key, "subscribe_proactive")
        .await
    {
        return resp;
    }

    let subscription = EventSubscription {
        id: Uuid::now_v7(),
        subscriber: body.subscriber,
        callback_url: body.callback_url,
        event_types: body.event_types,
        secret: body.secret,
    };

    match app.event_service.create(subscription.clone()).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => AwpErrorResponse(e).into_response(),
    }
}

/// `GET /awp/events/subscriptions` — gated to `known`: callback URLs and subscriber names are not
/// public. (adk-awp's own handler has no trust check.)
pub async fn list_subscriptions(State(app): State<AppState>, headers: HeaderMap) -> Response {
    let key = awp_gate::client_key(&headers, "awp-subscriptions");
    if let Err(resp) = app.awp.check(&headers, &key, "list_subscriptions").await {
        return resp;
    }
    match app.event_service.list().await {
        Ok(subs) => Json(subs).into_response(),
        Err(e) => AwpErrorResponse(e).into_response(),
    }
}

/// `DELETE /awp/events/subscriptions/{id}` — gated to `known`, like creating one.
pub async fn delete_subscription(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let key = awp_gate::client_key(&headers, "awp-subscriptions");
    if let Err(resp) = app.awp.check(&headers, &key, "delete_subscription").await {
        return resp;
    }
    match app.event_service.delete(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => AwpErrorResponse(e).into_response(),
    }
}
