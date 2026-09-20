use axum::response::sse::Event;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldEvent {
    Scenario {
        key: String,
        text: String,
        total_cards: usize,
    },
    CardSpawn {
        index: usize,
        card: serde_json::Value,
        /// Life domain of the card (ADR-002).
        domain: crate::domain::Domain,
    },
    CardStatus {
        index: usize,
        status: String,
        line: Option<String>,
    },
    CardResolve {
        index: usize,
        resolve: serde_json::Value,
    },
    CardSurface {
        index: usize,
        surface: String,
        slide: u32,
        total: u32,
    },
    Error {
        message: String,
    },
    SuzySummary {
        key: String,
        html: String,
    },
    Suggest {
        text: String,
        kind: String,
    },
    Conduct {
        steps: Vec<ConductStep>,
    },
    DeckFinish {
        big: String,
        sub: String,
        artifact_url: Option<String>,
        slide_count: Option<u32>,
    },
    /// A tool call was queued for the user's approval (S2-T7).
    PermissionRequest {
        action_id: String,
        agent_id: String,
        domain: crate::domain::Domain,
        effect: String,
        summary: String,
        expires_at: String,
    },
    /// A pending action was resolved (approved / rejected / failed / expired).
    ActionResult {
        action_id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        audit_id: Option<String>,
    },
    Done,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConductStep {
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
}

pub fn to_event(ev: &FieldEvent) -> Event {
    Event::default().data(serde_json::to_string(ev).unwrap_or_else(|_| "{}".into()))
}