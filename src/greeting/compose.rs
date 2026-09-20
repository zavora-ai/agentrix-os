use std::sync::Arc;

use adk_runner::Runner;
use serde::Serialize;

use crate::agents::morning::MorningMcpPool;

use super::context::{gather, GreetingSnapshot};

#[derive(Clone, Debug, Serialize)]
pub struct GreetingPayload {
    pub salutation: String,
    pub body: String,
    pub full_text: String,
    pub source: &'static str,
    pub integrations: Vec<String>,
}

pub async fn compose(
    runner: Option<&Arc<Runner>>,
    pool: Option<&MorningMcpPool>,
    brand_body: &str,
    brand_tone: Option<&str>,
    home_location: Option<&str>,
) -> GreetingPayload {
    let snapshot = gather(pool, home_location).await;

    if snapshot.has_integration_facts() {
        if let Some(runner) = runner {
            if let Ok(body) =
                super::agent::compose_from_snapshot(runner, &snapshot, brand_tone).await
            {
                return build_payload(
                    &snapshot.salutation,
                    body,
                    "agent",
                    snapshot.connected.clone(),
                );
            }
        }
        let body = deterministic_body(&snapshot);
        return build_payload(
            &snapshot.salutation,
            body,
            "integrations",
            snapshot.connected.clone(),
        );
    }

    let body = sanitize_brand_body(brand_body);
    build_payload(&snapshot.salutation, body, "brand", vec![])
}

fn build_payload(
    salutation: &str,
    body: String,
    source: &'static str,
    integrations: Vec<String>,
) -> GreetingPayload {
    let body = body.trim().to_string();
    let full_text = if body.is_empty() {
        salutation.to_string()
    } else {
        format!("{salutation}. {body}")
    };
    GreetingPayload {
        salutation: salutation.into(),
        body,
        full_text,
        source,
        integrations,
    }
}

/// Strip salutation prefixes from brand copy — the server owns time-of-day salutation.
fn sanitize_brand_body(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_lowercase();
    for prefix in [
        "good morning.",
        "good morning,",
        "good afternoon.",
        "good afternoon,",
        "good evening.",
        "good evening,",
    ] {
        if lower.starts_with(prefix) {
            return trimmed[prefix.len()..].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn deterministic_body(snapshot: &GreetingSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(cal) = &snapshot.calendar {
        match cal.meeting_count {
            0 => parts.push("Your calendar is clear today.".into()),
            1 => {
                if let Some(title) = &cal.first_title {
                    parts.push(format!("You have one meeting today — {title}."));
                } else {
                    parts.push("You have one meeting today.".into());
                }
            }
            n => {
                if let Some(title) = &cal.first_title {
                    parts.push(format!(
                        "You have {n} meetings today — first up is {title}."
                    ));
                } else {
                    parts.push(format!("You have {n} meetings today."));
                }
            }
        }
    }

    if let Some(inbox) = &snapshot.inbox {
        match inbox.needs_reply {
            0 => {}
            1 => parts.push("One email needs your attention.".into()),
            n => parts.push(format!("{n} emails need your attention.")),
        }
    }

    if let Some(news) = &snapshot.news {
        if let Some(headline) = &news.headline {
            parts.push(format!("Top story: {headline}."));
        }
    }

    if let Some(weather) = &snapshot.weather {
        let loc = weather.location.as_deref().unwrap_or("your area");
        match (&weather.summary, weather.temperature_f) {
            (Some(s), Some(t)) => parts.push(format!("In {loc} it's {t:.0}° and {s}.")),
            (Some(s), None) => parts.push(format!("In {loc}: {s}.")),
            (None, Some(t)) => parts.push(format!("In {loc} it's {t:.0}°.")),
            _ => {}
        }
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::greeting::context::{
        CalendarFacts, GreetingSnapshot, NewsFacts,
    };

    #[test]
    fn brand_body_strips_duplicate_salutation() {
        assert_eq!(
            sanitize_brand_body("Good morning. I'm ready when you are."),
            "I'm ready when you are."
        );
    }

    #[test]
    fn deterministic_uses_only_provided_facts() {
        let snap = GreetingSnapshot {
            salutation: "Good evening".into(),
            calendar: Some(CalendarFacts {
                meeting_count: 2,
                first_title: Some("Standup".into()),
                first_start: None,
            }),
            news: Some(NewsFacts {
                headline: Some("Markets steady".into()),
                source: None,
            }),
            connected: vec!["calendar".into(), "news".into()],
            ..Default::default()
        };
        let body = deterministic_body(&snap);
        assert!(body.contains("2 meetings"));
        assert!(body.contains("Standup"));
        assert!(body.contains("Markets steady"));
        assert!(!body.contains("email"));
    }

    #[test]
    fn brand_fallback_when_no_integrations() {
        let payload = build_payload(
            "Good evening",
            "I'm ready when you are.".into(),
            "brand",
            vec![],
        );
        assert_eq!(payload.source, "brand");
        assert_eq!(payload.full_text, "Good evening. I'm ready when you are.");
        assert!(payload.integrations.is_empty());
    }
}