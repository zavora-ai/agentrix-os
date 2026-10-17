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
    /// `message` is often a runner or tool error (`e.to_string()`) that can echo tool output, and
    /// the field UI shows it with `innerHTML`. It is escaped when the event is serialized, so
    /// every construction site is covered; escaping is idempotent, so the Mother re-wrapping an
    /// already-serialized error does not double-encode it.
    Error {
        #[serde(serialize_with = "serialize_escaped")]
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

fn serialize_escaped<S: serde::Serializer>(text: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&crate::html::escape(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(ev: &FieldEvent) -> serde_json::Value {
        serde_json::to_value(ev).unwrap()
    }

    #[test]
    fn error_message_is_escaped_on_the_wire() {
        let ev = FieldEvent::Error {
            message: "tool failed: <img src=x onerror=alert(1)> & retried".into(),
        };
        assert_eq!(
            wire(&ev),
            serde_json::json!({
                "type": "error",
                "message": "tool failed: &lt;img src=x onerror=alert(1)&gt; &amp; retried",
            })
        );
    }

    #[test]
    fn rewrapped_error_is_not_double_escaped() {
        // The Mother copies `message` out of a target's serialized event and emits it again.
        let first = wire(&FieldEvent::Error { message: "<script>x</script> AT&T".into() });
        let copied = first["message"].as_str().unwrap().to_string();
        assert_eq!(wire(&FieldEvent::Error { message: copied }), first);
    }

    #[test]
    fn constant_error_text_is_unchanged() {
        let msg = "Missing deck artifacts — run a deck intent first";
        assert_eq!(wire(&FieldEvent::Error { message: msg.into() })["message"], msg);
    }
}
