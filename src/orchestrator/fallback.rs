//! Honest fallbacks (CLAUDE.md rule 1).
//!
//! When an agent fails, is not connected, or returns nothing usable, the card says so. It never
//! shows a plausible default. Mock cards streamed in a live deployment (a scenario whose runner
//! did not boot) carry `"simulated": true` so the UI text, Suzy and the Mother's synthesis treat
//! them as samples, not as the user's data.
//!
//! The agent's own words go into card HTML escaped ([`crate::html::escape`]): they summarize
//! email, Slack and news that anyone can write.

use serde_json::{json, Value};

use crate::html;

/// Resolve for a card whose agent ran but produced nothing usable.
pub fn unavailable(sub: impl Into<String>) -> Value {
    json!({ "big": "Unavailable", "sub": sub.into(), "actions": ["Dismiss"], "unavailable": true })
}

/// Resolve for a card whose integration is not connected.
pub fn not_connected(sub: impl Into<String>) -> Value {
    json!({ "big": "Not connected", "sub": sub.into(), "actions": ["Dismiss"], "unavailable": true })
}

/// Up to `n` non-empty lines of an agent's own text, as card HTML: each line escaped, then given
/// the server's `<b>•</b>` bullet. A line that starts with `<b>` is escaped like any other; no
/// agent is asked for markup, so markup in its text came from the content it read.
pub fn agent_lines(text: &str, n: usize) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(n)
        .map(|l| format!("<b>•</b> {}", html::escape(l)))
        .collect()
}

/// First non-empty line of an agent's text, cut to `max` characters, as escaped card HTML.
pub fn first_line(text: &str, max: usize) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| html::escape(&l.chars().take(max).collect::<String>()))
}

/// A `lines` resolve from the agent's own text, or [`unavailable`] when it said nothing.
pub fn lines_or_unavailable(text: &str, unavailable_sub: &str, actions: &[&str]) -> Value {
    let lines = agent_lines(text, 3);
    if lines.is_empty() {
        return unavailable(unavailable_sub);
    }
    json!({ "lines": lines, "actions": actions })
}

/// Whether a card or resolve was marked simulated.
pub fn is_simulated(v: &Value) -> bool {
    v.get("simulated").and_then(Value::as_bool).unwrap_or(false)
}

/// Mark a mock card spawn as simulated, including the scripted resolve it carries.
pub fn simulated_card(card: &Value) -> Value {
    let mut c = card.clone();
    if let Some(obj) = c.as_object_mut() {
        obj.insert("simulated".into(), json!(true));
        if let Some(resolve) = obj.get("resolve").map(simulated_resolve) {
            obj.insert("resolve".into(), resolve);
        }
    }
    c
}

/// Mark a mock resolve as simulated and say so in the text the card shows.
pub fn simulated_resolve(resolve: &Value) -> Value {
    let mut r = resolve.clone();
    let Some(obj) = r.as_object_mut() else {
        return r;
    };
    obj.insert("simulated".into(), json!(true));
    if let Some(sub) = obj.get("sub").and_then(Value::as_str) {
        let sub = format!("Simulated sample · {sub}");
        obj.insert("sub".into(), json!(sub));
    } else if let Some(lines) = obj.get_mut("lines").and_then(Value::as_array_mut) {
        lines.insert(0, json!("<b>Simulated sample</b> — not your data"));
    }
    r
}

/// What Suzy says over simulated cards instead of summarizing them.
pub fn simulated_summary(scenario: &str) -> String {
    let mut chars = scenario.chars();
    let scenario: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    format!(
        "These <b>{scenario}</b> cards are <b>simulated samples</b> — the live agents for this aren't running, so nothing here is your data."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_agent_text_is_unavailable_not_a_default() {
        let r = lines_or_unavailable("  \n ", "Brief returned nothing", &["Read aloud"]);
        assert_eq!(r["big"], "Unavailable");
        assert_eq!(r["unavailable"], true);
        let r = lines_or_unavailable("Rates paused\nChips rally", "x", &["Open"]);
        assert_eq!(r["lines"][0], "<b>•</b> Rates paused");
        assert_eq!(r["actions"][0], "Open");
    }

    #[test]
    fn simulated_resolve_is_marked_and_labeled() {
        let big = simulated_resolve(&json!({"big": "$284", "sub": "TAP Air", "actions": ["Hold seat"]}));
        assert!(is_simulated(&big));
        assert_eq!(big["sub"], "Simulated sample · TAP Air");
        let lines = simulated_resolve(&json!({"lines": ["<b>Fri</b> arrive"], "actions": []}));
        assert!(lines["lines"][0].as_str().unwrap().contains("Simulated"));
        let card = simulated_card(&json!({"title": "Flights", "resolve": {"big": "$284", "sub": "TAP Air"}}));
        assert!(is_simulated(&card) && is_simulated(&card["resolve"]));
        assert_eq!(card["resolve"]["sub"], "Simulated sample · TAP Air");
        assert!(!is_simulated(&json!({"title": "Flights"})));
    }

    #[test]
    fn agent_lines_escape_markup_from_summarized_content() {
        let text = "Re: <img src=x onerror=alert(1)>\n<script>alert(document.cookie)</script>\n<b>Fri</b><img src=x onerror=alert(2)>";
        let lines = agent_lines(text, 3);
        assert_eq!(lines[0], "<b>•</b> Re: &lt;img src=x onerror=alert(1)&gt;");
        assert_eq!(lines[1], "<b>•</b> &lt;script&gt;alert(document.cookie)&lt;/script&gt;");
        assert_eq!(lines[2], "<b>•</b> &lt;b&gt;Fri&lt;/b&gt;&lt;img src=x onerror=alert(2)&gt;");
        for l in &lines {
            assert!(!l.contains("<img") && !l.contains("<script"), "{l}");
        }
        let r = lines_or_unavailable("Q&A moved <b>today</b>", "x", &["Open"]);
        assert_eq!(r["lines"][0], "<b>•</b> Q&amp;A moved &lt;b&gt;today&lt;/b&gt;");
    }

    #[test]
    fn first_line_is_escaped_after_cutting() {
        assert_eq!(
            first_line("\n  <img src=x onerror=alert(1)> moved to 3pm\nmore", 72).as_deref(),
            Some("&lt;img src=x onerror=alert(1)&gt; moved to 3pm")
        );
        assert_eq!(first_line("<script>", 4).as_deref(), Some("&lt;scr"));
        assert_eq!(first_line(" \n ", 72), None);
    }
}
