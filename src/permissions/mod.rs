//! Authority model: Observe / Suggest / Automate × tool effect class (ADR-003).
//!
//! - types and the pure decision matrix (§10.2 of `docs/PERSONAL_AI_OS.md`) — S0
//! - [`store`] per-user modes and the pause switch, [`pending`] actions awaiting approval,
//!   [`audit`] log, and the [`gate`] toolset wrapper that enforces all of it — S2

pub mod audit;
pub mod gate;
pub mod pending;
pub mod store;

pub use audit::{AuditEntry, AuditLog};
pub use gate::{execute_approved, PermissionGate, PermissionServices, ToolRegistry};
pub use pending::{PendingAction, PendingActions, PendingEvent, PendingStatus, Transition};
pub use store::{AgentPermission, AgentPermissionView, PauseScope, PermissionStore};

use serde::{Deserialize, Serialize};

/// How much authority an agent has. Missing in config → `Suggest`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Observe,
    #[default]
    Suggest,
    Automate,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Observe => "observe",
            Mode::Suggest => "suggest",
            Mode::Automate => "automate",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "observe" => Some(Mode::Observe),
            "suggest" => Some(Mode::Suggest),
            "automate" => Some(Mode::Automate),
            _ => None,
        }
    }
    pub fn badge(&self) -> &'static str {
        match self {
            Mode::Observe => "👁",
            Mode::Suggest => "💡",
            Mode::Automate => "⚡",
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a tool does to the world outside the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    WriteLocal,
    ScheduleWithOthers,
    SendExternal,
    PublishPublic,
    Financial,
    Delete,
}

impl Effect {
    pub const ALL: [Effect; 7] = [
        Effect::Read,
        Effect::WriteLocal,
        Effect::ScheduleWithOthers,
        Effect::SendExternal,
        Effect::PublishPublic,
        Effect::Financial,
        Effect::Delete,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Effect::Read => "read",
            Effect::WriteLocal => "write_local",
            Effect::ScheduleWithOthers => "schedule_with_others",
            Effect::SendExternal => "send_external",
            Effect::PublishPublic => "publish_public",
            Effect::Financial => "financial",
            Effect::Delete => "delete",
        }
    }

    pub fn parse(s: &str) -> Option<Effect> {
        Effect::ALL.iter().copied().find(|e| e.as_str() == s.trim())
    }

    pub fn is_read(&self) -> bool {
        matches!(self, Effect::Read)
    }

    /// Effects that no recipe may ever automate (§10.2).
    pub fn never_automated(&self) -> bool {
        matches!(self, Effect::PublishPublic | Effect::Financial)
    }
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Outcome of the permission matrix for one tool call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Execute now (and log; audit when the effect is not `read`).
    Allow,
    /// Do not execute; create a pending action for the user to approve.
    Pending,
    /// Do not execute; tell the agent why.
    Deny(&'static str),
}

/// The pure decision matrix (§10.2). `recipe_allows` is only consulted in `Automate` mode and
/// means "an approved recipe covers this exact effect for this agent".
pub fn decide(mode: Mode, effect: Effect, recipe_allows: bool) -> Decision {
    use Effect::*;
    match (mode, effect) {
        (_, Read) => Decision::Allow,
        (Mode::Observe, _) => {
            Decision::Deny("not permitted in observe mode — describe what you would do instead")
        }
        (Mode::Suggest, WriteLocal) => Decision::Allow,
        (Mode::Suggest, _) => Decision::Pending,
        (Mode::Automate, WriteLocal) => Decision::Allow,
        (Mode::Automate, e) if e.never_automated() => {
            Decision::Deny("this effect is never automated — it needs an explicit approval")
        }
        (Mode::Automate, _) if recipe_allows => Decision::Allow,
        (Mode::Automate, _) => Decision::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_matrix_matches_concept_table() {
        for e in Effect::ALL {
            let d = decide(Mode::Observe, e, false);
            if e.is_read() {
                assert_eq!(d, Decision::Allow);
            } else {
                assert!(matches!(d, Decision::Deny(_)), "{e} must be denied in observe");
            }
        }
        assert_eq!(decide(Mode::Suggest, Effect::WriteLocal, false), Decision::Allow);
        assert_eq!(decide(Mode::Suggest, Effect::SendExternal, false), Decision::Pending);
        assert_eq!(decide(Mode::Suggest, Effect::Financial, false), Decision::Pending);
        assert_eq!(decide(Mode::Automate, Effect::SendExternal, true), Decision::Allow);
        assert_eq!(decide(Mode::Automate, Effect::SendExternal, false), Decision::Pending);
        assert!(matches!(decide(Mode::Automate, Effect::PublishPublic, true), Decision::Deny(_)));
        assert!(matches!(decide(Mode::Automate, Effect::Financial, true), Decision::Deny(_)));
        assert_eq!(decide(Mode::Automate, Effect::Delete, true), Decision::Allow);
    }

    #[test]
    fn effect_and_mode_parse() {
        assert_eq!(Effect::parse("send_external"), Some(Effect::SendExternal));
        assert_eq!(Mode::parse("Automate"), Some(Mode::Automate));
        assert_eq!(Mode::default(), Mode::Suggest);
        assert_eq!(serde_json::to_string(&Effect::WriteLocal).unwrap(), "\"write_local\"");
    }
}
