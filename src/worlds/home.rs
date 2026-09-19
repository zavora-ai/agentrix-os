//! Home World — `home_mother` (S5-T1) and the home agent registry (S5-T2).
//!
//! Conservative by design: finance and health run in Observe, social accounts are stubs, and
//! nothing here fabricates. Until the contacts migration (009, James) lands, the Family agent
//! reads important dates from memory and an optional contacts CSV; Personal Productivity reads
//! personal tasks from memory and postponement counts from the ledger.

use serde::Serialize;

use crate::agents::{family, personal_productivity};
use crate::domain::Domain;
use crate::intelligence::ledger::LedgerService;
use crate::memory::MemoryService;
use crate::mother::intake::Target;
use crate::worlds::{card, one_card_events, WorldResult};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct HomeAgent {
    pub id: &'static str,
    pub phase1_agents: &'static [&'static str],
    pub scenario: &'static str,
    pub memory_scope: &'static [&'static str],
    pub stub: bool,
    pub mission: &'static str,
}

pub const HOME_AGENTS: &[HomeAgent] = &[
    HomeAgent { id: "family", phase1_agents: &["family_agent"], scenario: "home", memory_scope: &["home.", "shared."], stub: false, mission: "family dates, events, household coordination" },
    HomeAgent { id: "personal_productivity", phase1_agents: &["personal_productivity_agent"], scenario: "home", memory_scope: &["home.", "shared."], stub: false, mission: "personal tasks, errands, routines" },
    HomeAgent { id: "health_wellness", phase1_agents: &["health_agent"], scenario: "week", memory_scope: &["home.", "shared."], stub: false, mission: "sleep, exercise, habits — never diagnoses" },
    HomeAgent { id: "finance", phase1_agents: &["money_agent"], scenario: "week", memory_scope: &["home.", "shared."], stub: false, mission: "spending and budget — information only" },
    HomeAgent { id: "entertainment", phase1_agents: &["headlines_agent", "markets_agent", "now_agent"], scenario: "live", memory_scope: &["home.", "shared."], stub: false, mission: "news, audio, what is live" },
    HomeAgent { id: "social_fun", phase1_agents: &["maker_agent"], scenario: "proactive", memory_scope: &["home.", "shared."], stub: false, mission: "fun, ideas, weekend plans" },
    HomeAgent { id: "travel", phase1_agents: &["planner_agent", "stay_agent", "flights_agent"], scenario: "lisbon", memory_scope: &["home.", "shared."], stub: false, mission: "trips, stays, itineraries" },
    HomeAgent { id: "personal_social", phase1_agents: &["personal_social_agent"], scenario: "stub", memory_scope: &["home."], stub: true, mission: "personal social presence" },
];

pub fn agent(id: &str) -> Option<&'static HomeAgent> {
    HOME_AGENTS.iter().find(|a| a.id == id)
}

pub fn phase1_agent_ids() -> Vec<&'static str> {
    HOME_AGENTS.iter().flat_map(|a| a.phase1_agents.iter().copied()).collect()
}

/// Runtime flag (`AGENTRIX_HOME_MOTHER=0` disables).
pub fn enabled() -> bool {
    !matches!(std::env::var("AGENTRIX_HOME_MOTHER").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

/// Fold the home targets into one [`WorldResult`] and add the Family, Personal and Personal Social
/// cards (the latter a labeled stub, BK-102).
pub async fn fold(
    memory: &MemoryService,
    ledger: &LedgerService,
    user_id: &str,
    targets: &[Target],
    collected: &[(Vec<serde_json::Value>, bool)],
) -> (WorldResult, Vec<(Target, Vec<serde_json::Value>)>) {
    let mut result = WorldResult::from_targets(Domain::Home, targets, collected);
    let mut extra = Vec::new();
    if result.agents.is_empty() {
        return (result, extra);
    }
    let wants = |id: &str| targets.iter().any(|t| t.world == Domain::Home && (t.agent == id || t.agent == "home"));

    // Family — dates from memory + optional contacts CSV (S5-T3).
    if wants("family") || wants("personal_productivity") {
        let f = family::facts(memory, user_id, chrono::Local::now().date_naive()).await;
        let (big, sub) = if f.upcoming.is_empty() {
            (
                if f.contacts == 0 { "Nothing on file".to_string() } else { format!("{} contacts, no dates soon", f.contacts) },
                "Tell me \"remember Sara's birthday is 20 Oct\" or set CONTACTS_CSV_PATH".to_string(),
            )
        } else {
            let first = &f.upcoming[0];
            (
                format!("{} date{} ahead", f.upcoming.len(), if f.upcoming.len() == 1 { "" } else { "s" }),
                format!("{} · {} in {} day{}", first.label, first.name, first.days_until, if first.days_until == 1 { "" } else { "s" }),
            )
        };
        result.facts.push(format!("Family: {big} — {sub}"));
        extra.push((
            Target { world: Domain::Home, agent: "family".into(), task: "important dates".into(), scenario: "home".into() },
            one_card_events(
                card("Family", "family.agent", "🏡", Domain::Home),
                serde_json::json!({ "big": big, "sub": sub, "actions": ["Add a date", "Open"], "source": f.sources }),
                Domain::Home,
            ),
        ));
    }

    // Personal productivity — open tasks from the shared store (S4-T3) plus chat-stated ones from memory (S5-T4).
    if wants("personal_productivity") || wants("family") {
        let p = personal_productivity::facts(memory, ledger, user_id).await;
        let (big, sub) = if p.tasks.is_empty() {
            ("No personal tasks".to_string(), "Ask me to add one, or say \"remember to renew my passport\"".to_string())
        } else {
            let top: Vec<String> = p.tasks.iter().take(3).map(|t| if t.postponed > 0 { format!("{} (postponed {}×)", t.title, t.postponed) } else { t.title.clone() }).collect();
            (format!("{} personal task{}", p.tasks.len(), if p.tasks.len() == 1 { "" } else { "s" }), top.join(" · "))
        };
        result.facts.push(format!("Personal: {big} — {sub}"));
        extra.push((
            Target { world: Domain::Home, agent: "personal_productivity".into(), task: "personal tasks".into(), scenario: "home".into() },
            one_card_events(
                card("Personal", "personal.agent", "✅", Domain::Home),
                serde_json::json!({ "big": big, "sub": sub, "actions": ["Plan them", "Snooze"], "postponed_total": p.postponed_total }),
                Domain::Home,
            ),
        ));
    }

    // Personal Social — labeled stub (S5-T5, BK-102).
    result.stubs.push("personal_social");
    extra.push((
        Target { world: Domain::Home, agent: "personal_social".into(), task: "personal social presence".into(), scenario: "stub".into() },
        one_card_events(
            card("Personal social", "personal_social.agent", "📱", Domain::Home),
            serde_json::json!({ "big": "STUB — BK-102", "sub": "Instagram / Facebook / TikTok / X need social MCPs. Import an export to enable Observe mode.", "actions": ["Learn more"] }),
            Domain::Home,
        ),
    ));
    result.facts.push(format!("{} home agents ran, {} labeled stub(s)", result.agents.len(), result.stubs.len()));
    (result, extra)
}
