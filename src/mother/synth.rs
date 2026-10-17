//! Synthesis — one answer in Suzy's voice from many agent results (S1-T4).
//!
//! Generalizes `crate::agents::suzy::summarize` (one scenario) to N delegation results across
//! worlds. With an LLM runner the prose is generated from a structured facts block (never raw
//! content); without one a deterministic template composes the same facts. Every suggested
//! action carries the permission mode it will run under so the UI can render
//! "Approve" (Suggest) vs "Open" (Automate) vs facts-only (Observe).

use std::sync::Arc;

use adk_core::{Content, SessionId, UserId};
use adk_runner::Runner;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::domain::Domain;
use crate::mother::intake::Target;
use crate::permissions::Mode;

/// Outcome of one card inside a delegated scenario.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardOutcome {
    pub title: String,
    pub agent: String,
    pub domain: Domain,
    pub resolve: Option<serde_json::Value>,
    /// Scripted sample streamed because the scenario's live runner is not running. Never a fact.
    #[serde(default)]
    pub simulated: bool,
}

impl CardOutcome {
    /// One-line, content-free-ish fact for the synthesis block.
    pub fn headline(&self) -> String {
        if self.simulated {
            return format!("{}: simulated sample, not the user's data", self.title);
        }
        let Some(r) = &self.resolve else {
            return format!("{}: pending", self.title);
        };
        if let Some(big) = r.get("big").and_then(|v| v.as_str()) {
            let sub = r.get("sub").and_then(|v| v.as_str()).unwrap_or("");
            if sub.is_empty() {
                return format!("{}: {big}", self.title);
            }
            return format!("{}: {big} — {sub}", self.title);
        }
        if let Some(lines) = r.get("lines").and_then(|v| v.as_array()) {
            let first = lines
                .iter()
                .filter_map(|l| l.as_str())
                .map(strip_tags)
                .take(2)
                .collect::<Vec<_>>()
                .join("; ");
            return format!("{}: {first}", self.title);
        }
        format!("{}: done", self.title)
    }

    pub fn first_action(&self) -> Option<String> {
        if self.simulated {
            return None;
        }
        self.resolve
            .as_ref()?
            .get("actions")?
            .as_array()?
            .first()?
            .as_str()
            .map(str::to_string)
    }
}

/// All outcomes of one delegation target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetResult {
    pub target: Target,
    pub cards: Vec<CardOutcome>,
    pub timed_out: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub text: String,
    pub card: String,
    pub agent: String,
    pub domain: Domain,
    pub mode: Mode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Synthesis {
    pub html: String,
    pub actions: Vec<SuggestedAction>,
    /// `llm` | `template`
    pub source: &'static str,
}

pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn domain_label(d: Domain) -> &'static str {
    match d {
        Domain::Work => "Work",
        Domain::Home => "Home",
        Domain::Shared => "Today",
    }
}

/// Structured facts block handed to the LLM (and used by the template). Never raw content.
pub fn facts_block(intent: &str, results: &[TargetResult], memory_notes: &[String]) -> String {
    let mut lines = vec![format!("Intent: {intent}")];
    for r in results {
        lines.push(format!(
            "[{} · {} · via {}]{}",
            domain_label(r.target.world),
            r.target.agent,
            r.target.scenario,
            if r.timed_out { " (timed out — partial)" } else { "" }
        ));
        for c in &r.cards {
            lines.push(format!("  - {}", strip_tags(&c.headline())));
        }
    }
    if !memory_notes.is_empty() {
        lines.push("[Memory]".into());
        for m in memory_notes {
            lines.push(format!("  - {m}"));
        }
    }
    lines.join("\n")
}

/// Deterministic synthesis: grouped by world, key facts bold, actions tagged with a mode.
pub fn compose_template(
    intent: &str,
    results: &[TargetResult],
    memory_notes: &[String],
    mode_for: &(dyn Fn(&str) -> Mode + Sync),
) -> Synthesis {
    let mut parts: Vec<String> = Vec::new();
    let mut actions = Vec::new();

    for d in [Domain::Work, Domain::Home, Domain::Shared] {
        let cards: Vec<&CardOutcome> = results
            .iter()
            .flat_map(|r| r.cards.iter())
            .filter(|c| c.domain == d)
            .collect();
        if cards.is_empty() {
            continue;
        }
        let facts: Vec<String> = cards
            .iter()
            .map(|c| {
                let h = c.headline();
                match h.split_once(": ") {
                    Some((title, rest)) => format!("<b>{}</b> {}", title, strip_tags(rest)),
                    None => strip_tags(&h),
                }
            })
            .collect();
        parts.push(format!("{}: {}.", domain_label(d), facts.join(" · ")));
        for c in cards {
            if let Some(a) = c.first_action() {
                actions.push(SuggestedAction {
                    text: format!("{a} — {}", c.title),
                    card: c.title.clone(),
                    agent: c.agent.clone(),
                    domain: c.domain,
                    mode: mode_for(&c.agent),
                });
            }
        }
    }

    if results.iter().any(|r| r.timed_out) {
        parts.push("One agent did not finish in time; its card shows what it had.".into());
    }
    if let Some(note) = memory_notes.first() {
        parts.push(note.clone());
    }
    if parts.is_empty() {
        parts.push(format!("I looked into <b>{}</b> but nothing came back yet.", strip_tags(intent)));
    }

    Synthesis {
        html: parts.join(" "),
        actions,
        source: "template",
    }
}

/// Run a text prompt through a runner and collect the final text.
pub async fn run_prompt(
    runner: &Runner,
    user_id: &str,
    session_id: &str,
    prompt: &str,
) -> anyhow::Result<String> {
    crate::agents::ensure_runner_session(runner, user_id, session_id).await;
    let mut stream = runner
        .run(
            UserId::try_from(user_id)?,
            SessionId::try_from(session_id)?,
            Content::new("user").with_text(prompt),
        )
        .await?;
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        let event = chunk?;
        if let Some(content) = &event.llm_response.content {
            let t: String = content
                .parts
                .iter()
                .filter_map(|p| p.text().map(str::to_string))
                .collect::<Vec<_>>()
                .join("");
            if !t.is_empty() {
                text = t;
            }
        }
    }
    let trimmed = text.trim().to_string();
    anyhow::ensure!(!trimmed.is_empty(), "empty synthesis");
    Ok(trimmed)
}

/// LLM synthesis with template fallback. `suzy_runner` is the Phase 1 coordinator runner.
pub async fn compose(
    suzy_runner: Option<&Arc<Runner>>,
    user_id: &str,
    session_id: &str,
    intent: &str,
    results: &[TargetResult],
    memory_notes: &[String],
    mode_for: &(dyn Fn(&str) -> Mode + Sync),
) -> Synthesis {
    let template = compose_template(intent, results, memory_notes, mode_for);
    let Some(runner) = suzy_runner else {
        return template;
    };
    let prompt = format!(
        "You are the Mother Agent's voice (Suzy). Compose ONE 2–4 sentence HTML summary from these facts. \
         Group by world when both appear. Use <b> for key numbers and names. Cite memory as \"(you told me)\" \
         for known items and \"(I think)\" for assumed ones. No markdown, no wrapper tags.\n\n{}\n\nWrite the HTML now.",
        facts_block(intent, results, memory_notes)
    );
    match run_prompt(runner, user_id, session_id, &prompt).await {
        Ok(html) => Synthesis {
            html,
            actions: template.actions,
            source: "llm",
        },
        Err(e) => {
            tracing::warn!("mother synthesis fell back to template ({e:#})");
            template
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(world: Domain, scenario: &str, cards: Vec<(&str, &str, Domain, serde_json::Value)>) -> TargetResult {
        TargetResult {
            target: Target {
                world,
                agent: "x".into(),
                task: "t".into(),
                scenario: scenario.into(),
            },
            cards: cards
                .into_iter()
                .map(|(title, agent, domain, resolve)| CardOutcome {
                    title: title.into(),
                    agent: agent.into(),
                    domain,
                    simulated: crate::orchestrator::fallback::is_simulated(&resolve),
                    resolve: Some(resolve),
                })
                .collect(),
            timed_out: false,
        }
    }

    #[test]
    fn synth_template_groups_by_world_and_tags_modes() {
        let results = vec![
            res(Domain::Work, "morning", vec![
                ("Today", "calendar.agent", Domain::Work, serde_json::json!({"big":"3 meetings","sub":"first 9:30","actions":["Open"]})),
                ("Needs you", "inbox.agent", Domain::Work, serde_json::json!({"big":"2 to reply","sub":"","actions":["Draft replies"]})),
            ]),
            res(Domain::Home, "people", vec![
                ("Connections", "crm.agent", Domain::Home, serde_json::json!({"lines":["<b>Birthday:</b> Mara, Friday"],"actions":["Send notes"]})),
            ]),
        ];
        let s = compose_template("What do I need to know today?", &results, &["You prefer no meetings before 10 (you told me)".into()], &|agent| {
            if agent == "inbox.agent" { Mode::Suggest } else { Mode::Observe }
        });
        assert!(s.html.starts_with("Work: <b>Today</b> 3 meetings"), "{}", s.html);
        assert!(s.html.contains("Home: <b>Connections</b> Birthday: Mara, Friday"), "{}", s.html);
        assert!(s.html.contains("(you told me)"));
        assert_eq!(s.actions.len(), 3);
        let draft = s.actions.iter().find(|a| a.card == "Needs you").unwrap();
        assert_eq!(draft.mode, Mode::Suggest);
        assert_eq!(s.actions[0].mode, Mode::Observe);
        assert_eq!(s.source, "template");
    }

    #[test]
    fn simulated_cards_are_not_facts_or_actions() {
        let sample = crate::orchestrator::fallback::simulated_resolve(
            &serde_json::json!({"big":"$284 · TAP Air","sub":"Fri 6:40pm","actions":["Hold seat"]}),
        );
        let results = vec![res(Domain::Home, "lisbon", vec![("Flights", "travel.agent", Domain::Home, sample)])];
        let block = facts_block("Plan Lisbon", &results, &[]);
        assert!(block.contains("- Flights: simulated sample, not the user's data"), "{block}");
        assert!(!block.contains("$284"), "{block}");
        let s = compose_template("Plan Lisbon", &results, &[], &|_| Mode::Suggest);
        assert!(!s.html.contains("$284"), "{}", s.html);
        assert!(s.actions.is_empty());
    }

    #[test]
    fn synth_facts_block_is_structured_and_tagless() {
        let results = vec![res(Domain::Work, "deck", vec![("Auto-Excel", "auto-excel", Domain::Work, serde_json::json!({"big":"+38% QoQ","sub":"<i>4 sheets</i>","actions":["Save"]}))])];
        let block = facts_block("Build me a pitch deck", &results, &[]);
        assert!(block.contains("[Work · x · via deck]"));
        assert!(block.contains("- Auto-Excel: +38% QoQ — 4 sheets"));
        assert!(!block.contains("<i>"));
    }
}
