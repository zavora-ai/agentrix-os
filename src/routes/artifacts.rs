use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, Response, StatusCode},
    response::IntoResponse,
};

use crate::artifacts;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct ScopedArtifactPath {
    user_id: String,
    session_id: String,
    path: String,
}

#[derive(serde::Deserialize)]
pub struct LegacyArtifactPath {
    session_id: String,
    filename: String,
}

async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
    user_id: &str,
    session_id: &str,
) -> Result<(), StatusCode> {
    let record = state
        .sessions
        .get(session_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    if record.user_id != user_id {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(jwt_user) = state.awp.user_id(headers)
        && jwt_user.to_string() != user_id
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn file_response(path: &std::path::Path) -> Result<Response<Body>, StatusCode> {
    let body = std::fs::read(path).map_err(|_| StatusCode::NOT_FOUND)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, artifacts::content_type(path))
        .body(Body::from(body))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// `GET /artifacts/{user_id}/{session_id}/{*path}`
pub async fn get_scoped(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(params): Path<ScopedArtifactPath>,
) -> impl IntoResponse {
    authorize(&state, &headers, &params.user_id, &params.session_id).await?;

    let session_root = artifacts::session_dir(
        &state.artifact_dir,
        &params.user_id,
        &params.session_id,
    );
    let file = artifacts::resolve_file(&session_root, &params.path)
        .ok_or(StatusCode::NOT_FOUND)?;
    file_response(&file)
}

/// `GET /artifacts/{session_id}/{filename}` — legacy URLs; resolves user from session store.
pub async fn get_legacy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(params): Path<LegacyArtifactPath>,
) -> impl IntoResponse {
    let record = state
        .sessions
        .get(&params.session_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    authorize(&state, &headers, &record.user_id, &params.session_id).await?;

    let session_root =
        artifacts::session_dir(&state.artifact_dir, &record.user_id, &params.session_id);
    let file = artifacts::resolve_file(&session_root, &params.filename)
        .ok_or(StatusCode::NOT_FOUND)?;
    file_response(&file)
}