//! Intake — the first hop of the Mother Agent (S1-T1).
//!
//! Classifies an utterance into a structured [`IntakeResult`]: which life domains it touches,
//! which agents (via which Phase 1 scenario adapter) should act, whether it is a question or an
//! action, how urgent it is, and whether a clarifying question is needed instead.
//!
//! Strategy: deterministic cross-cutting rules first (they are what makes multi-world fan-out
//! reliable), then the LLM router (`crate::agents::router`) when configured, then the Phase 1
//! keyword fallback. The seven scenario keys stay valid targets so the demo tour keeps working.

use serde::{Deserialize, Serialize};

use crate::domain::Domain;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Question,
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    Normal,
    High,
}

/// One unit of delegated work. `scenario` is the Phase 1 workflow that fulfils the target until
/// the dedicated world agents ship (S4/S5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub world: Domain,
    pub agent: String,
    pub task: String,
    pub scenario: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IntakeResult {
    pub domains: Vec<Domain>,
    pub targets: Vec<Target>,
    pub kind: Kind,
    pub urgency: Urgency,
    /// When set, the Mother Agent asks this instead of delegating.
    pub clarify: Option<String>,
    /// `rule` | `llm` | `keyword`
    pub source: &'static str,
}

impl IntakeResult {
    pub fn clarify(message: impl Into<String>) -> Self {
        Self {
            domains: vec![],
            targets: vec![],
            kind: Kind::Question,
            urgency: Urgency::Normal,
            clarify: Some(message.into()),
            source: "rule",
        }
    }

    /// Single-target result for a Phase 1 scenario key.
    pub fn from_scenario(scenario: &str, text: &str, source: &'static str) -> Self {
        let world = Domain::for_scenario(scenario);
        let (agent, task) = scenario_agent(scenario);
        Self {
            domains: match world {
                Domain::Shared => vec![Domain::Work, Domain::Home],
                d => vec![d],
            },
            targets: vec![Target {
                world,
                agent: agent.into(),
                task: task.into(),
                scenario: scenario.into(),
            }],
            kind: detect_kind(text),
            urgency: detect_urgency(text),
            clarify: None,
            source,
        }
    }

    pub fn is_multi_target(&self) -> bool {
        self.targets.len() > 1
    }

    pub fn primary_scenario(&self) -> Option<&str> {
        self.targets.first().map(|t| t.scenario.as_str())
    }

    pub fn scenarios(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for t in &self.targets {
            if !out.contains(&t.scenario) {
                out.push(t.scenario.clone());
            }
        }
        out
    }
}

/// Which Phase 2 agent a Phase 1 scenario stands in for, and the task phrasing.
pub fn scenario_agent(scenario: &str) -> (&'static str, &'static str) {
    match scenario {
        "morning" => ("briefing", "today's calendar, inbox and brief"),
        "deck" => ("work_automation", "build the numbers, narrative and slides"),
        "people" => ("team_comms", "who is waiting on you and what to prep"),
        "week" => ("briefing", "money, health and focus for the week"),
        "live" => ("entertainment", "headlines, markets and what is live now"),
        "lisbon" => ("travel", "flights, stay and itinerary"),
        "proactive" => ("research_knowledge", "what the background agents found"),
        "home" => ("home", "family and personal life"),
        _ => ("mother", "handle the request"),
    }
}

pub const CLARIFY_MESSAGE: &str = "I'm not sure which flow you want. Try <b>Start my day</b>, <b>Build me a pitch deck</b>, or <b>Plan a trip to Lisbon</b>.";

fn t(world: Domain, agent: &str, task: &str, scenario: &str) -> Target {
    Target {
        world,
        agent: agent.into(),
        task: task.into(),
        scenario: scenario.into(),
    }
}

/// Whether lowercased `text` mentions any of `needles` as whole words or phrases. A plural
/// `s`/`es` still matches ("trip" → "trips"), a longer word does not ("trip" ≠ "stripe",
/// "week" ≠ "weekend"). A needle ending in `*` matches as a word prefix ("summar*").
pub fn mentions(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| mentions_one(text, n))
}

fn mentions_one(text: &str, needle: &str) -> bool {
    let (needle, prefix) = match needle.strip_suffix('*') {
        Some(n) => (n.trim(), true),
        None => (needle.trim(), false),
    };
    if needle.is_empty() {
        return false;
    }
    let starts_word = |t: &str| t.chars().next().is_some_and(char::is_alphanumeric);
    let mut from = 0;
    while let Some(pos) = text[from..].find(needle) {
        let start = from + pos;
        let tail = &text[start + needle.len()..];
        let before = !text[..start].chars().next_back().is_some_and(char::is_alphanumeric);
        let tail = tail
            .strip_prefix("es")
            .filter(|t| !starts_word(t))
            .or_else(|| tail.strip_prefix('s').filter(|t| !starts_word(t)))
            .unwrap_or(tail);
        if before && (prefix || !starts_word(tail)) {
            return true;
        }
        from = start + needle.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Requests for a deliverable: a deck, slides or a report *about* travel, spending or health is
/// still a deliverable, so the topic rules below do not claim it.
const DELIVERABLES: &[&str] = &["deck", "pitch", "presentation", "slides", "slide", "document", "report"];

pub fn detect_kind(text: &str) -> Kind {
    let l = text.to_lowercase();
    let action = [
        "help me", "reorganize", "reorganise", "reschedule", "move ", "send", "draft", "book",
        "reply", "combine", "build me", "make me", "create", "schedule", "cancel", "remind me to",
        "set a reminder", "plan a", "plan me", "prepare", "apply", "handle it", "do it",
    ];
    if mentions(&l, &action) {
        Kind::Action
    } else {
        Kind::Question
    }
}

pub fn detect_urgency(text: &str) -> Urgency {
    let l = text.to_lowercase();
    if mentions(&l, &["urgent", "asap", "overwhelmed", "right now", "emergency", "immediately"]) {
        Urgency::High
    } else if mentions(&l, &["whenever", "no rush", "sometime", "eventually"]) {
        Urgency::Low
    } else {
        Urgency::Normal
    }
}

/// Deterministic cross-cutting rules. These fire before the LLM because they are what makes
/// multi-world fan-out reliable (concept §3.5). Returns `None` when no rule applies.
pub fn classify_rules(text: &str) -> Option<IntakeResult> {
    let l = text.trim().to_lowercase();
    if l.is_empty() || l.len() < 3 || !l.chars().any(|c| c.is_alphabetic()) {
        return Some(IntakeResult::clarify(CLARIFY_MESSAGE));
    }
    if matches!(l.as_str(), "hi" | "hello" | "hey" | "hmm" | "help" | "test" | "ok" | "yes" | "no") {
        return Some(IntakeResult::clarify(CLARIFY_MESSAGE));
    }
    let kind = detect_kind(text);
    let urgency = detect_urgency(text);
    let mk = |domains: Vec<Domain>, targets: Vec<Target>, kind: Kind| IntakeResult {
        domains,
        targets,
        kind,
        urgency,
        clarify: None,
        source: "rule",
    };

    // Daily briefing — both worlds through the shared briefing adapter.
    if mentions(&l, &["what do i need to know", "start my day", "good morning", "morning briefing", "my briefing", "brief me"]) {
        return Some(mk(
            vec![Domain::Work, Domain::Home],
            vec![t(Domain::Shared, "briefing", "today across work and home", "morning")],
            Kind::Question,
        ));
    }
    // Prepare for the afternoon — work productivity + team prep.
    if mentions(&l, &["prepare me for", "prep me for", "my afternoon", "afternoon meeting", "next meeting"]) {
        return Some(mk(
            vec![Domain::Work],
            vec![
                t(Domain::Work, "productivity", "calendar and priorities for the rest of the day", "morning"),
                t(Domain::Work, "team_comms", "who is waiting and 1:1 prep", "people"),
            ],
            Kind::Question,
        ));
    }
    // Overwhelmed / reorganize — both worlds, high urgency, an action.
    if mentions(&l, &["overwhelmed", "reorganize my day", "reorganise my day", "reorganize today", "lighter day", "too much today", "help me reorganize"]) {
        return Some(IntakeResult {
            domains: vec![Domain::Work, Domain::Home],
            targets: vec![
                t(Domain::Work, "productivity", "propose what to defer or move today", "morning"),
                t(Domain::Home, "personal_productivity", "propose which personal tasks to move", "home"),
            ],
            kind: Kind::Action,
            urgency: Urgency::High,
            clarify: None,
            source: "rule",
        });
    }
    // Work status — fan out across the work world.
    if mentions(&l, &["happening with work", "how is work", "how's work", "work status", "what's up at work", "whats up at work", "at work today"]) {
        return Some(mk(
            vec![Domain::Work],
            vec![
                t(Domain::Work, "productivity", "calendar, inbox and brief", "morning"),
                t(Domain::Work, "team_comms", "team threads and follow-ups", "people"),
            ],
            Kind::Question,
        ));
    }
    // Family commitments — home world (Family agent lands in S5; People adapter until then).
    if mentions(&l, &["family commitment", "family event", "family plans", "remind me about family", "what's happening at home", "whats happening at home", "home commitments"]) {
        return Some(mk(
            vec![Domain::Home],
            vec![t(Domain::Home, "family", "family commitments and important dates", "home")],
            Kind::Question,
        ));
    }
    // Personal tasks and errands — home world, native card.
    if mentions(&l, &["my personal tasks", "errand", "personal to-do", "personal todo", "what do i need to do at home", "household tasks"]) {
        return Some(mk(vec![Domain::Home], vec![t(Domain::Home, "personal_productivity", "personal tasks and errands", "home")], kind));
    }
    // Reading & knowledge.
    if mentions(&l, &["been reading", "what have i read", "my reading", "what am i learning", "reading lately"]) {
        return Some(mk(
            vec![Domain::Shared],
            vec![t(Domain::Shared, "reading_knowledge", "recent reading, topics and trend", "proactive")],
            Kind::Question,
        ));
    }
    // A deliverable goes to the router / keyword fallback (the deck flow), whatever its topic.
    if mentions(&l, DELIVERABLES) {
        return None;
    }
    // Travel — home world through the lisbon adapter.
    if mentions(&l, &["trip", "travel", "flight", "hotel", "itinerary", "vacation", "holiday", "lisbon"]) {
        return Some(mk(vec![Domain::Home], vec![t(Domain::Home, "travel", "flights, stay and itinerary", "lisbon")], kind));
    }
    // Money / health — home world through the week adapter.
    if mentions(&l, &["spending", "my budget", "my money", "how much did i spend", "subscriptions"]) {
        return Some(mk(vec![Domain::Home], vec![t(Domain::Home, "finance", "spending and budget", "week")], kind));
    }
    // "exercise" alone is also what you do with stock options.
    if mentions(&l, &["my sleep", "my health", "exercise", "workout", "wellness"])
        && !mentions(&l, &["option", "stock option", "shares", "equity", "rsu", "vesting"])
    {
        return Some(mk(vec![Domain::Home], vec![t(Domain::Home, "health_wellness", "sleep, exercise and habits", "week")], kind));
    }
    None
}

/// Full classification: rules → LLM router (if a runner is configured) → keyword fallback.
pub async fn classify(
    router: Option<&adk_runner::Runner>,
    user_id: &str,
    session_id: &str,
    text: &str,
) -> IntakeResult {
    if let Some(r) = classify_rules(text) {
        return r;
    }
    if let Some(runner) = router {
        match crate::agents::router::classify(runner, user_id, session_id, text, crate::events::mock::match_scenario).await {
            crate::agents::router::ClassifyOutcome::Scenario(key) => {
                return IntakeResult::from_scenario(&key, text, "llm");
            }
            crate::agents::router::ClassifyOutcome::Clarify(msg) => {
                return IntakeResult::clarify(msg);
            }
        }
    }
    classify_keywords(text)
}

/// Synchronous classification without an LLM (rules → keyword). Used by tests and fallbacks.
pub fn classify_offline(text: &str) -> IntakeResult {
    classify_rules(text).unwrap_or_else(|| classify_keywords(text))
}

/// The Phase 1 keyword fallback. Text that names no scenario gets a clarifying question rather
/// than a guess.
fn classify_keywords(text: &str) -> IntakeResult {
    match crate::events::mock::match_scenario(text) {
        Some(key) => IntakeResult::from_scenario(key, text, "keyword"),
        None => IntakeResult::clarify(CLARIFY_MESSAGE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intake_six_reference_prompts() {
        let r = classify_offline("What do I need to know today?");
        assert_eq!(r.domains, vec![Domain::Work, Domain::Home]);
        assert_eq!(r.primary_scenario(), Some("morning"));

        let r = classify_offline("Prepare me for my afternoon.");
        assert!(r.is_multi_target());
        assert!(r.targets.iter().all(|t| t.world == Domain::Work));

        let r = classify_offline("What's happening with work?");
        assert_eq!(r.domains, vec![Domain::Work]);
        assert_eq!(r.scenarios(), vec!["morning", "people"]);

        let r = classify_offline("Remind me about family commitments.");
        assert_eq!(r.domains, vec![Domain::Home]);
        assert_eq!(r.targets[0].agent, "family");

        let r = classify_offline("I'm overwhelmed. Help me reorganize today.");
        assert_eq!(r.kind, Kind::Action);
        assert_eq!(r.urgency, Urgency::High);
        assert!(r.is_multi_target());

        let r = classify_offline("What have I been reading lately?");
        assert_eq!(r.targets[0].agent, "reading_knowledge");
    }

    #[test]
    fn intake_matches_whole_words_not_substrings() {
        // Reported misroutes: "Stripe" contains "trip", a deck about spending is a deck, and
        // an option exercise is not a workout.
        let r = classify_offline("Build me a pitch deck on Stripe");
        assert_eq!(r.primary_scenario(), Some("deck"));
        assert_eq!(r.targets[0].agent, "work_automation");
        assert_eq!(classify_offline("Build me a deck on our cloud spending").primary_scenario(), Some("deck"));
        assert_eq!(classify_offline("Write a report on travel costs").primary_scenario(), Some("deck"));
        let r = classify_offline("When is my option exercise deadline?");
        assert!(r.targets.iter().all(|t| t.agent != "health_wellness"), "{:?}", r.targets);

        // The topic rules still fire on their own words, plurals included.
        assert_eq!(classify_offline("Plan a weekend trip to Lisbon").targets[0].agent, "travel");
        assert_eq!(classify_offline("Book me flights to Paris").targets[0].agent, "travel");
        assert_eq!(classify_offline("How's my spending?").targets[0].agent, "finance");
        assert_eq!(classify_offline("Are my subscriptions too high?").targets[0].agent, "finance");
        assert_eq!(classify_offline("How much exercise did I get?").targets[0].agent, "health_wellness");
        assert_eq!(classify_offline("How many workouts this week?").targets[0].agent, "health_wellness");
        assert_eq!(classify_offline("Summarize what changed this week").primary_scenario(), Some("week"));
        assert_eq!(classify_offline("How are the markets?").primary_scenario(), Some("live"));

        assert_eq!(classify_offline("What personal errands are due?").targets[0].agent, "personal_productivity");

        // Nothing recognisable: ask, never default to a Lisbon trip.
        let r = classify_offline("Where am I on my career goals?");
        assert_eq!(r.clarify.as_deref(), Some(CLARIFY_MESSAGE));
        assert!(r.targets.is_empty());
    }

    #[test]
    fn intake_word_matcher_respects_boundaries() {
        assert!(mentions("plan a trip", &["trip"]));
        assert!(mentions("two trips, one week", &["trip"]));
        assert!(!mentions("pitch deck on stripe", &["trip"]));
        assert!(!mentions("a weekend away", &["week"]));
        assert!(mentions("my weekly summary", &["summar*"]));
        assert!(mentions("what's happening at home?", &["what's happening at home"]));
        assert!(!mentions("a moved meeting", &["move "]));
        assert!(mentions("move it", &["move "]));
        assert!(mentions("reschedule my 1:1s", &["reschedule"]));
        assert!(!mentions("", &["trip"]) && !mentions("trip", &[""]));
        assert_eq!(detect_kind("Send it"), Kind::Action);
        assert_eq!(detect_kind("Who is sending the invoices?"), Kind::Question);
    }

    #[test]
    fn intake_keeps_phase1_scenarios_and_clarifies_noise() {
        assert_eq!(classify_offline("Build me a pitch deck").primary_scenario(), Some("deck"));
        assert_eq!(classify_offline("Plan a weekend trip to Lisbon").primary_scenario(), Some("lisbon"));
        assert_eq!(classify_offline("Build me a pitch deck").kind, Kind::Action);
        assert!(classify_offline("hmm").clarify.is_some());
        assert!(classify_offline("??").clarify.is_some());
    }
}
