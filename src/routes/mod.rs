pub mod action;
pub mod actions;
pub mod artifacts;
pub mod awp;
pub mod ambient;
pub mod background;
pub mod agents;
pub mod cards;
pub mod chat;
pub mod commit;
pub mod consents;
pub mod events;
pub mod fuse;
pub mod greeting;
pub mod health;
pub mod intent;
pub mod live;
pub mod memory;
pub mod oauth;
pub mod people;
pub mod permissions;
pub mod public;
pub mod session;
pub mod tasks;
pub mod voice;
pub mod worlds;

/// Same-origin by default: no layer, so no CORS headers and other sites cannot read credentialed
/// responses. `CORS_ALLOWED_ORIGINS` opens the API to those origins only, with credentials.
pub fn cors_layer(allowed_origins: &[String]) -> Option<tower_http::cors::CorsLayer> {
    use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
    if allowed_origins.is_empty() {
        return None;
    }
    let origins = allowed_origins
        .iter()
        .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
        .collect::<Vec<_>>();
    Some(
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(AllowMethods::mirror_request())
            .allow_headers(AllowHeaders::mirror_request())
            .allow_credentials(true),
    )
}
