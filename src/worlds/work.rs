//! Work World — `work_mother` (S4-T1) and the work agent registry (S4-T2).
//!
//! Re-homing is metadata, not file renames (docs/PROGRESS.md §4): each Phase 2 work agent maps
//! to the Phase 1 agents and scenario that fulfil it today. The Work Mother fans out the work
//! targets of a turn concurrently, folds the results into one structured [`WorldResult`], and
//! adds what only it can see across agents: unanswered email threads (S4-T4, from hashed
//! subjects in the ledger) and the labeled Career / Professional Social stubs (S4-T5/T6).

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::domain::Domain;
use crate::intelligence::ledger::{LedgerQuery, LedgerService};
use crate::mother::intake::Target;
use crate::permissions::Effect;
use crate::worlds::{card, one_card_events, WorldResult};

/// One Phase 2 work agent and how it is fulfilled today.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WorkAgent {
    /// Phase 2 id (concept §4).
    pub id: &'static str,
    /// Allowlist agent ids that do the work in Phase 1 (world/mode/effects live there).
    pub phase1_agents: &'static [&'static str],
    /// Scenario adapter that runs them.
    pub scenario: &'static str,
    /// Memory scope prefixes this agent may read.
    pub memory_scope: &'static [&'static str],
    /// Labeled stub until its MCP server exists (BK-101).
    pub stub: bool,
    pub mission: &'static str,
}

pub const WORK_AGENTS: &[WorkAgent] = &[
    WorkAgent { id: "productivity", phase1_agents: &["calendar_agent"], scenario: "morning", memory_scope: &["work.", "shared."], stub: false, mission: "tasks, calendar, meetings, deadlines, planning" },
    WorkAgent { id: "email", phase1_agents: &["inbox_agent"], scenario: "morning", memory_scope: &["work.", "shared."], stub: false, mission: "categorize, draft, flag urgent, track unanswered threads" },
    WorkAgent { id: "team_comms", phase1_agents: &["team_agent", "priya_agent"], scenario: "people", memory_scope: &["work.", "shared."], stub: false, mission: "channels, mentions, follow-ups, 1:1 prep" },
    WorkAgent { id: "project", phase1_agents: &["focus_agent", "connections_agent"], scenario: "week", memory_scope: &["work.", "shared."], stub: false, mission: "status, milestones, risks, progress" },
    WorkAgent { id: "research_knowledge", phase1_agents: &["research_agent"], scenario: "proactive", memory_scope: &["work.", "shared."], stub: false, mission: "research, reading, knowledge organization" },
    WorkAgent { id: "work_automation", phase1_agents: &["excel_agent", "docs_agent", "slides_agent", "combine_agent"], scenario: "deck", memory_scope: &["work.", "shared."], stub: false, mission: "documents, decks, reports; recipes from S12" },
    WorkAgent { id: "career", phase1_agents: &["career_agent"], scenario: "stub", memory_scope: &["work.", "shared."], stub: true, mission: "goals, skills, opportunities, learning plans" },
    WorkAgent { id: "professional_social", phase1_agents: &["professional_social_agent"], scenario: "stub", memory_scope: &["work.", "shared."], stub: true, mission: "professional presence and networking" },
];

pub fn agent(id: &str) -> Option<&'static WorkAgent> {
    WORK_AGENTS.iter().find(|a| a.id == id)
}

/// Every Phase 1 allowlist agent that belongs to the Work World.
pub fn phase1_agent_ids() -> Vec<&'static str> {
    WORK_AGENTS.iter().flat_map(|a| a.phase1_agents.iter().copied()).collect()
}

/// Runtime flag (`AGENTRIX_WORK_MOTHER=0` disables): the old direct fan-out stays available until
/// the R2 validation passes (docs/PROGRESS.md §8).
pub fn enabled() -> bool {
    !matches!(std::env::var("AGENTRIX_WORK_MOTHER").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

// ---------------------------------------------------------------------------
// S4-T4 · follow-up tracker
// ---------------------------------------------------------------------------

/// A thread the inbox agent read but nobody replied to (content-free: hashed subject only).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct FollowUp {
    pub subject_hash: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub age_days: i64,
    pub reads: usize,
}

/// Default age after which an unanswered thread is flagged.
pub const FOLLOW_UP_DAYS: i64 = 3;

fn is_reply_effect(e: Option<Effect>) -> bool {
    matches!(e, Some(Effect::WriteLocal) | Some(Effect::SendExternal))
}

/// Unanswered threads older than `min_age_days`: subjects the email agents read (any `read`
/// tool call carrying a subject hash) with no later draft or send for the same hash.
pub async fn unanswered_threads(ledger: &LedgerService, user_id: &str, min_age_days: i64) -> Vec<FollowUp> {
    let since = Utc::now() - Duration::days(45);
    let mut reads: std::collections::HashMap<String, (DateTime<Utc>, DateTime<Utc>, usize)> = Default::default();
    let mut replied: std::collections::HashMap<String, DateTime<Utc>> = Default::default();
    for agent in ["inbox_agent", "email"] {
        let events = ledger
            .query(&LedgerQuery { user_id: user_id.into(), agent_id: Some(agent.into()), since: Some(since), ..Default::default() })
            .await;
        for e in events {
            let Some(h) = e.subject_hash.clone() else { continue };
            if is_reply_effect(e.effect) {
                let t = replied.entry(h).or_insert(e.ts);
                if e.ts > *t {
                    *t = e.ts;
                }
            } else if e.kind == "tool_call" {
                let r = reads.entry(h).or_insert((e.ts, e.ts, 0));
                r.0 = r.0.min(e.ts);
                r.1 = r.1.max(e.ts);
                r.2 += 1;
            }
        }
    }
    let now = Utc::now();
    let mut out: Vec<FollowUp> = reads
        .into_iter()
        .filter(|(h, (first, _, _))| replied.get(h).map(|r| r < first).unwrap_or(true))
        .map(|(h, (first, last, n))| FollowUp { subject_hash: h, first_seen: first, last_seen: last, age_days: (now - first).num_days(), reads: n })
        .filter(|f| f.age_days >= min_age_days)
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.age_days));
    out
}

// ---------------------------------------------------------------------------
// S4-T1 · the Work Mother
// ---------------------------------------------------------------------------

/// Fold the work targets' collected events into one [`WorldResult`] and produce the extra
/// outcomes only the Work Mother can add (follow-ups card, labeled stubs). Returns the result and
/// `(target, events)` pairs to append to the field.
pub async fn fold(
    ledger: &LedgerService,
    user_id: &str,
    targets: &[Target],
    collected: &[(Vec<serde_json::Value>, bool)],
) -> (WorldResult, Vec<(Target, Vec<serde_json::Value>)>) {
    let mut result = WorldResult::from_targets(Domain::Work, targets, collected);
    let mut extra = Vec::new();
    if result.agents.is_empty() {
        return (result, extra);
    }

    // S4-T4 — unanswered threads, from hashed subjects only.
    let follow_ups = unanswered_threads(ledger, user_id, FOLLOW_UP_DAYS).await;
    if !follow_ups.is_empty() {
        let n = follow_ups.len();
        let oldest = follow_ups[0].age_days;
        result.facts.push(format!("{n} email thread{} unanswered for {FOLLOW_UP_DAYS}+ days (oldest {oldest} days)", if n == 1 { "" } else { "s" }));
        extra.push((
            Target { world: Domain::Work, agent: "email".into(), task: "unanswered threads".into(), scenario: "morning".into() },
            one_card_events(
                card("Follow-ups", "inbox.agent", "↩️", Domain::Work),
                serde_json::json!({
                    "big": format!("{n} unanswered"),
                    "sub": format!("read {FOLLOW_UP_DAYS}+ days ago, no reply yet · oldest {oldest} d"),
                    "actions": ["Draft replies", "Snooze"]
                }),
                Domain::Work,
            ),
        ));
    }

    // S4-T5/T6 — labeled stubs, never fake data.
    for a in WORK_AGENTS.iter().filter(|a| a.stub) {
        result.stubs.push(a.id);
        let (title, glyph, label, body) = match a.id {
            "career" => ("Career", "🎯", "STUB — BK-101", "Goals, skills and learning plans need a career/network source. Add goals with \"remember …\" meanwhile."),
            _ => ("Professional presence", "💼", "STUB — BK-101", "LinkedIn mentions and networking need a LinkedIn MCP. Import an export to enable Observe mode."),
        };
        extra.push((
            Target { world: Domain::Work, agent: a.id.into(), task: a.mission.into(), scenario: "stub".into() },
            one_card_events(
                card(title, &format!("{}.agent", a.id), glyph, Domain::Work),
                serde_json::json!({ "big": label, "sub": body, "actions": ["Learn more"] }),
                Domain::Work,
            ),
        ));
    }
    result.facts.push(format!("{} work agents ran, {} labeled stub(s)", result.agents.len(), result.stubs.len()));
    (result, extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intelligence::ledger::ActivityEvent;

    fn read(user: &str, ledger: &LedgerService, subject: &str, days_ago: i64) {
        let mut ev = ActivityEvent::new(user, Domain::Work, "inbox_agent", "tool_call").effect(Effect::Read).subject(ledger.hash_key(), subject);
        ev.ts = Utc::now() - Duration::days(days_ago);
        ledger.record(ev);
    }

    #[tokio::test]
    async fn work_follow_ups_flag_old_unanswered_threads_only() {
        let ledger = LedgerService::in_memory();
        read("u", &ledger, "thread-old", 4);
        read("u", &ledger, "thread-fresh", 1);
        read("u", &ledger, "thread-answered", 5);
        let mut reply = ActivityEvent::new("u", Domain::Work, "inbox_agent", "tool_call").effect(Effect::WriteLocal).subject(ledger.hash_key(), "thread-answered");
        reply.ts = Utc::now() - Duration::days(4);
        ledger.record(reply);

        let f = unanswered_threads(&ledger, "u", FOLLOW_UP_DAYS).await;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].subject_hash, ledger.hash("thread-old"));
        assert_eq!(f[0].age_days, 4);
        assert!(!serde_json::to_string(&f).unwrap().contains("thread-old"), "follow-ups carry hashes, never subjects");
    }

    #[test]
    fn work_registry_covers_the_concept_agents() {
        let ids: Vec<&str> = WORK_AGENTS.iter().map(|a| a.id).collect();
        for want in ["productivity", "email", "team_comms", "project", "career", "research_knowledge", "professional_social", "work_automation"] {
            assert!(ids.contains(&want), "{want}");
        }
        assert_eq!(WORK_AGENTS.iter().filter(|a| a.stub).count(), 2);
        assert!(enabled());
    }
}
