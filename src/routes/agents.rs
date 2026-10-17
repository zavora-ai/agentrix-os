use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::routes::actions::owned_session;
use crate::state::{AgentRecord, AppState};

#[derive(Serialize)]
pub struct AgentsResponse {
    pub active: Vec<AgentRecord>,
    pub resting: Vec<AgentRecord>,
}

#[derive(Deserialize)]
pub struct AgentBody {
    pub session_id: String,
    pub title: String,
    pub glyph: Option<String>,
    pub agent: Option<String>,
}

pub async fn list_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<AgentsResponse>, Response> {
    let client_key = format!("agents:{session_id}");
    state
        .awp
        .check(&headers, &client_key, "list_agents")
        .await
        .map_err(|r| r)?;

    owned_session(&state, &headers, &session_id).await?;
    let Some((active, resting)) = state.sessions.list_agents(&session_id).await else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(AgentsResponse { active, resting }))
}

pub async fn snooze_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(body): Json<AgentBody>,
) -> Result<StatusCode, Response> {
    let client_key = format!("snooze:{}", body.session_id);
    state
        .awp
        .check(&headers, &client_key, "snooze_agent")
        .await
        .map_err(|r| r)?;

    owned_session(&state, &headers, &body.session_id).await?;
    let glyph = body.glyph.unwrap_or_else(|| "💤".into());
    let agent_name = body.agent.unwrap_or_else(|| agent_id.clone());
    state
        .sessions
        .agent_snooze(
            &body.session_id,
            AgentRecord {
                id: agent_id,
                title: body.title.clone(),
                glyph,
                agent: agent_name,
                rail: "resting".into(),
                domain: Default::default(),
            },
        )
        .await;
    state
        .sessions
        .remove_card(&body.session_id, &body.title)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn wake_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent_id): Path<String>,
    Json(body): Json<AgentBody>,
) -> Result<Json<AgentRecord>, Response> {
    let client_key = format!("wake:{}", body.session_id);
    state
        .awp
        .check(&headers, &client_key, "wake_agent")
        .await
        .map_err(|r| r)?;

    owned_session(&state, &headers, &body.session_id).await?;
    let Some(agent) = state.sessions.agent_wake(&body.session_id, &agent_id).await else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };
    Ok(Json(agent))
}