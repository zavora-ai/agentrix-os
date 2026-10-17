//! Memory statements in chat (S3-T4): "remember …", "forget …", "why do you think …",
//! "what do you know about me". Deterministic; runs before intake so these never get delegated.

use crate::domain::Domain;
use crate::memory::service::{Kind, MemoryService, NewItem, Provenance, Scope, Sensitivity};

/// What a memory statement resolved to; the caller renders it.
#[derive(Debug)]
pub enum MemoryReply {
    Remembered { key: String, value: String },
    Forgot(usize),
    Explained(Vec<(String, String, Vec<String>)>),
    Listed(Vec<String>),
    Nothing(&'static str),
}

impl MemoryReply {
    pub fn html(&self) -> String {
        match self {
            MemoryReply::Remembered { key, value } => format!("Got it — I'll remember <b>{}</b> ({}). You can edit or forget it any time.", esc(value), esc(key)),
            MemoryReply::Forgot(0) => "I couldn't find anything matching that to forget.".into(),
            MemoryReply::Forgot(n) => format!("Forgotten — <b>{n}</b> item{} removed from what I know.", if *n == 1 { "" } else { "s" }),
            MemoryReply::Explained(items) if items.is_empty() => "I'm not assuming anything about that — I only hold things you told me or confirmed.".into(),
            MemoryReply::Explained(items) => {
                let lines: Vec<String> = items
                    .iter()
                    .map(|(key, value, why)| format!("<b>{}</b> = {} — because: {}", esc(key), esc(value), esc(&why.join("; "))))
                    .collect();
                format!("Here is what I think and why: {}", lines.join(" · "))
            }
            MemoryReply::Listed(lines) if lines.is_empty() => "I don't hold anything about you yet. Tell me things with \"remember …\" and I'll keep them as known facts you control.".into(),
            MemoryReply::Listed(lines) => format!("What I hold about you: {}", lines.iter().map(|l| esc(l)).collect::<Vec<_>>().join(" · ")),
            MemoryReply::Nothing(msg) => (*msg).into(),
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
        if out.len() >= 48 {
            break;
        }
    }
    out.trim_matches('_').to_string()
}

/// Turn a free-text statement into (domain, category, key, value, sensitivity).
pub fn structure_statement(text: &str) -> (Domain, &'static str, String, serde_json::Value, Sensitivity) {
    let l = text.trim().trim_end_matches('.').to_lowercase();
    let clean = text.trim().trim_end_matches('.').to_string();
    // A few well-known shapes get structured keys; everything else is a note under context.*
    if let Some(rest) = l.strip_prefix("my name is ").or_else(|| l.strip_prefix("call me ")) {
        return (Domain::Shared, "profile", "profile.name".into(), serde_json::json!(title(rest)), Sensitivity::Normal);
    }
    for p in ["i live in ", "my home is in ", "my home city is ", "my city is ", "i'm based in ", "i am based in "] {
        if let Some(rest) = l.strip_prefix(p) {
            return (Domain::Shared, "profile", "profile.home_location".into(), serde_json::json!(title(rest)), Sensitivity::Normal);
        }
    }
    if let Some(rest) = l.strip_prefix("my timezone is ") {
        return (Domain::Shared, "profile", "profile.timezone".into(), serde_json::json!(rest.trim()), Sensitivity::Normal);
    }
    if l.contains("no meetings before") || l.contains("never take meetings before") || l.contains("don't take meetings before") {
        let hour = l.split("before").nth(1).map(|h| h.trim().trim_end_matches("am").trim().to_string()).unwrap_or_default();
        return (Domain::Work, "preference", "preference.meetings.earliest_start".into(), serde_json::json!(hour), Sensitivity::Normal);
    }
    if l.contains("protect") && (l.contains("evening") || l.contains("weekend")) {
        return (Domain::Home, "preference", "preference.protected_time".into(), serde_json::json!(clean), Sensitivity::Normal);
    }
    if l.contains("birthday") || l.contains("anniversary") {
        let (name, label) = crate::agents::family::parse_person_and_label(&clean);
        return (Domain::Home, "date", format!("date.{label}.{}", slug(&name)), serde_json::json!(clean), Sensitivity::Normal);
    }
    if let Some(task) = l.strip_prefix("to ").or_else(|| l.strip_prefix("i need to ")).or_else(|| l.strip_prefix("i have to ")) {
        let task_text = clean[clean.len() - task.len()..].trim().to_string();
        let domain = if has_work_words(&l) { Domain::Work } else { Domain::Home };
        return (domain, "task", format!("task.{}", slug(&task_text)), serde_json::json!(task_text), sensitivity_of(&l));
    }
    let sensitivity = sensitivity_of(&l);
    let domain = if l.contains("work") || l.contains("meeting") || l.contains("boss") || l.contains("client") || l.contains("project") {
        Domain::Work
    } else if l.contains("family") || l.contains("home") || l.contains("kids") || l.contains("partner") || l.contains("wife") || l.contains("husband") {
        Domain::Home
    } else {
        Domain::Shared
    };
    (domain, "context", format!("context.{}", slug(&clean)), serde_json::json!(clean), sensitivity)
}

/// Health and money statements are encrypted at rest and get a content-free key.
fn sensitivity_of(l: &str) -> Sensitivity {
    if l.contains("health") || l.contains("medication") || l.contains("doctor") || l.contains("sleep") {
        Sensitivity::Health
    } else if l.contains("salary") || l.contains("bank") || l.contains("mortgage") || l.contains("budget") {
        Sensitivity::Financial
    } else {
        Sensitivity::Normal
    }
}

fn has_work_words(l: &str) -> bool {
    ["work", "meeting", "boss", "client", "project", "deck", "report", "colleague"].iter().any(|w| l.contains(w))
}

fn title(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Handle a memory statement. Returns `None` when `text` is not one.
pub async fn try_handle(memory: &MemoryService, user_id: &str, session_id: &str, text: &str) -> Option<MemoryReply> {
    let l = text.trim().to_lowercase();

    for prefix in ["remind me to ", "remember that ", "remember: ", "remember ", "please remember that ", "please remember "] {
        if let Some(rest) = text.trim().get(prefix.len()..).filter(|_| l.starts_with(prefix)) {
            let rest = rest.trim();
            if rest.is_empty() {
                return Some(MemoryReply::Nothing("What should I remember?"));
            }
            let statement = if l.starts_with("remind me to ") { format!("to {rest}") } else { rest.to_string() };
            let (domain, category, key, value, sensitivity) = structure_statement(&statement);
            let mut provenance = Provenance::new("user_statement").session(Some(session_id));
            let key = if sensitivity.encrypted_at_rest() {
                // Only the value is encrypted at rest; the key and provenance are stored as
                // they are, so neither may carry the statement: the key is an HMAC of it and
                // the raw text is not kept as a note.
                format!("{category}.private_{}", memory.crypto().opaque_id(user_id, &statement))
            } else {
                provenance = provenance.note(rest);
                key
            };
            let item = memory
                .remember(
                    user_id,
                    NewItem {
                        domain,
                        category,
                        key: &key,
                        value: value.clone(),
                        sensitivity,
                        source_agent: "mother",
                        provenance,
                    },
                )
                .await;
            let shown = match &value {
                serde_json::Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            return Some(MemoryReply::Remembered { key: item.key, value: shown });
        }
    }

    for prefix in ["forget that ", "forget about ", "forget "] {
        if l.starts_with(prefix) {
            let phrase = text.trim()[prefix.len()..].trim().trim_end_matches('.');
            if phrase.is_empty() {
                return Some(MemoryReply::Nothing("What should I forget?"));
            }
            let n = memory.forget_matching(user_id, phrase).await.len();
            return Some(MemoryReply::Forgot(n));
        }
    }

    if l.starts_with("why do you think") || l.starts_with("why do you assume") {
        let items = memory.read(user_id, Scope::MOTHER, None).await;
        let topic = l
            .trim_start_matches("why do you think")
            .trim_start_matches("why do you assume")
            .trim()
            .trim_start_matches("that")
            .trim()
            .trim_end_matches('?')
            .to_string();
        let explained: Vec<(String, String, Vec<String>)> = items
            .into_iter()
            .filter(|i| i.kind == Kind::Assumed)
            .filter(|i| topic.is_empty() || i.key.to_lowercase().contains(&topic) || i.value.to_string().to_lowercase().contains(&topic))
            .map(|i| {
                let why: Vec<String> = i
                    .provenance
                    .iter()
                    .map(|p| {
                        let mut s = p.kind.replace('_', " ");
                        if let Some(a) = &p.agent {
                            s.push_str(&format!(" by {a}"));
                        }
                        if let Some(n) = &p.note {
                            s.push_str(&format!(" — {n}"));
                        }
                        s
                    })
                    .collect();
                (i.key, i.value.to_string(), why)
            })
            .collect();
        return Some(MemoryReply::Explained(explained));
    }

    if l.starts_with("what do you know about me") || l.starts_with("what do you remember") || l == "what do you know" {
        let lines: Vec<String> = memory.read(user_id, Scope::MOTHER, None).await.iter().map(|i| i.note()).collect();
        return Some(MemoryReply::Listed(lines));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_statement_structuring() {
        let (d, c, k, v, _) = structure_statement("I never take meetings before 10");
        assert_eq!((d, c, k.as_str(), v.as_str().unwrap()), (Domain::Work, "preference", "preference.meetings.earliest_start", "10"));
        let (d, _, k, v, _) = structure_statement("I live in Nairobi");
        assert_eq!((d, k.as_str(), v.as_str().unwrap()), (Domain::Shared, "profile.home_location", "Nairobi"));
        let (d, _, k, _, s) = structure_statement("my sleep has been bad since the project started");
        assert_eq!(d, Domain::Work);
        assert!(k.starts_with("context.my_sleep_has_been_bad"));
        assert_eq!(s, Sensitivity::Health);
    }

    #[tokio::test]
    async fn memory_chat_remember_forget_explain() {
        let m = MemoryService::in_memory();
        let r = try_handle(&m, "u", "s", "Remember I never take meetings before 10").await.unwrap();
        assert!(matches!(r, MemoryReply::Remembered { .. }));
        let items = m.read("u", Scope::MOTHER, None).await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, Kind::Known);
        assert!(try_handle(&m, "u", "s", "What's happening with work?").await.is_none());
        let r = try_handle(&m, "u", "s", "why do you think that").await.unwrap();
        assert!(matches!(r, MemoryReply::Explained(ref v) if v.is_empty()));
        let r = try_handle(&m, "u", "s", "forget meetings").await.unwrap();
        assert!(matches!(r, MemoryReply::Forgot(1)));
        assert!(m.read("u", Scope::MOTHER, None).await.is_empty());
    }

    #[tokio::test]
    async fn memory_chat_sensitive_statement_keeps_no_plaintext_at_rest() {
        let m = MemoryService::in_memory();
        let r = try_handle(&m, "u", "s", "Remember my medication is lisinopril 10mg").await.unwrap();
        assert!(matches!(r, MemoryReply::Remembered { .. }));
        let items = m.read("u", Scope::MOTHER, None).await;
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.sensitivity, Sensitivity::Health);
        assert!(item.key.starts_with("context.private_"), "{}", item.key);
        assert!(!item.key.contains("medication") && !item.key.contains("lisinopril"));
        assert!(item.provenance.iter().all(|p| p.note.is_none()), "no raw statement in provenance");

        // At rest: the value is ciphertext and nothing else in the row carries the statement.
        let (value, provenance) = crate::memory::service::stored_columns(m.crypto(), item).unwrap();
        let row = format!("{} {value} {provenance}", item.key);
        assert!(!row.to_lowercase().contains("lisinopril") && !row.contains("medication"), "{row}");

        // The user still gets it back, cited as something they said.
        assert!(m.notes_for("u", Scope::MOTHER, 5).await[0].contains("lisinopril 10mg (you told me)"));
        // Saying it again updates the same item; "forget" still finds it by value.
        try_handle(&m, "u", "s", "remember my medication is lisinopril 10mg").await.unwrap();
        assert_eq!(m.read("u", Scope::MOTHER, None).await.len(), 1);
        assert!(matches!(try_handle(&m, "u", "s", "forget lisinopril").await, Some(MemoryReply::Forgot(1))));

        // A reminder about medication is sensitive too; ordinary statements keep readable keys.
        try_handle(&m, "u", "s", "remind me to refill my medication").await.unwrap();
        try_handle(&m, "u", "s", "remember the office wifi is slow").await.unwrap();
        let keys: Vec<String> = m.read("u", Scope::MOTHER, None).await.into_iter().map(|i| i.key).collect();
        assert!(keys.iter().any(|k| k.starts_with("task.private_")), "{keys:?}");
        assert!(keys.iter().any(|k| k == "context.the_office_wifi_is_slow"), "{keys:?}");
    }
}
