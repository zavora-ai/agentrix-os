use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use adk_core::{Content, UserId, SessionId};
use adk_runner::Runner;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::artifacts;
use crate::events::sse::{to_event, ConductStep, FieldEvent};
use crate::orchestrator::{coordinator, persist};
use crate::state::{SessionArtifacts, SessionStore};

fn deck_conduct_steps() -> Vec<ConductStep> {
    vec![
        ConductStep {
            op: "fuse".into(),
            source: Some("Auto-Excel".into()),
            target: Some("Auto-Slides".into()),
            delay_ms: None,
        },
        ConductStep {
            op: "wait".into(),
            source: None,
            target: None,
            delay_ms: Some(500),
        },
        ConductStep {
            op: "fuse".into(),
            source: Some("Auto-Docs".into()),
            target: Some("Auto-Slides".into()),
            delay_ms: None,
        },
        ConductStep {
            op: "wait".into(),
            source: None,
            target: None,
            delay_ms: Some(400),
        },
    ]
}

fn parse_saved_path(response: &serde_json::Value) -> Option<PathBuf> {
    let output = response
        .get("output")
        .and_then(|o| o.as_str())
        .or_else(|| response.get("path").and_then(|p| p.as_str()))
        .or_else(|| response.get("file_path").and_then(|p| p.as_str()));

    if let Some(s) = output {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
            if let Some(p) = parsed
                .get("path")
                .or_else(|| parsed.get("file_path"))
                .or_else(|| parsed.get("output_path"))
                .and_then(|v| v.as_str())
            {
                return Some(PathBuf::from(p));
            }
        }
        let path = PathBuf::from(s);
        if path.is_absolute() {
            return Some(path);
        }
    }
    None
}

fn parse_slide_count(response: &serde_json::Value) -> Option<u32> {
    let output = response
        .get("output")
        .and_then(|o| o.as_str())
        .unwrap_or("");
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(n) = parsed
            .get("slide_count")
            .or_else(|| parsed.get("slides"))
            .and_then(|v| v.as_u64())
        {
            return Some(n as u32);
        }
    }
    for token in output.split_whitespace() {
        if let Ok(n) = token.parse::<u32>() {
            if (1..=50).contains(&n) {
                return Some(n);
            }
        }
    }
    None
}

fn find_newest_artifact(dir: &Path, ext: &str) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(ext) {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if newest.as_ref().is_none_or(|(t, _)| modified > *t) {
                            newest = Some((modified, path));
                        }
                    }
                }
            }
        }
    }
    newest.map(|(_, p)| p)
}

/// Newest `.ext` file in `dir` written at or after `since` (this run's output, not an old file).
fn newest_artifact_since(dir: &Path, ext: &str, since: std::time::SystemTime) -> Option<PathBuf> {
    let path = find_newest_artifact(dir, ext)?;
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    (modified >= since).then_some(path)
}

/// `deck_finish` sub line: the slide count only when `describe_presentation` reported one.
fn finish_sub(slide_count: Option<u32>) -> String {
    match slide_count {
        Some(n) => format!("{n} slides · numbers + story combined"),
        None => "Numbers + story combined".into(),
    }
}

fn collect_artifacts(dir: &Path) -> SessionArtifacts {
    SessionArtifacts {
        xlsx: find_newest_artifact(dir, "xlsx").map(|p| p.display().to_string()),
        docx: find_newest_artifact(dir, "docx").map(|p| p.display().to_string()),
        pptx: find_newest_artifact(dir, "pptx").map(|p| p.display().to_string()),
        combined_pptx: None,
    }
}

pub fn stream_combine(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    artifact_root: PathBuf,
    sessions: SessionStore,
    scenario: Option<String>,
) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    let (tx, rx) = mpsc::channel(64);

    tokio::spawn(async move {
        let session_dir = artifacts::session_dir(&artifact_root, &user_id, &session_id);

        let _ = tx
            .send(Ok(to_event(&FieldEvent::Conduct {
                steps: deck_conduct_steps(),
            })))
            .await;

        let record = sessions.get(&session_id).await;
        let artifacts = record
            .map(|r| r.artifacts)
            .unwrap_or_else(|| collect_artifacts(&session_dir));

        let xlsx = artifacts
            .xlsx
            .clone()
            .or_else(|| find_newest_artifact(&session_dir, "xlsx").map(|p| p.display().to_string()));
        let docx = artifacts
            .docx
            .clone()
            .or_else(|| find_newest_artifact(&session_dir, "docx").map(|p| p.display().to_string()));
        let pptx = artifacts
            .pptx
            .clone()
            .or_else(|| find_newest_artifact(&session_dir, "pptx").map(|p| p.display().to_string()));

        if xlsx.is_none() || docx.is_none() || pptx.is_none() {
            let _ = tx
                .send(Ok(to_event(&FieldEvent::Error {
                    message: "Missing deck artifacts — run a deck intent first".into(),
                })))
                .await;
            let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
            return;
        }

        let prompt = format!(
            "{intent}\n\n\
             [Save files to: {}/]\n\
             [Artifacts]\n\
             pptx: {}\n\
             [Sibling sources]\n\
             xlsx: {}\n\
             docx: {}\n\
             Merge excel numbers and docs narrative into the slides deck.",
            session_dir.display(),
            pptx.as_deref().unwrap_or(""),
            xlsx.as_deref().unwrap_or(""),
            docx.as_deref().unwrap_or(""),
        );

        let content = Content::new("user").with_text(&prompt);
        let uid = match UserId::try_from(user_id.as_str()) {
            Ok(u) => u,
            Err(e) => {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                return;
            }
        };
        let sid = match SessionId::try_from(session_id.as_str()) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                return;
            }
        };

        let started = std::time::SystemTime::now();
        crate::agents::ensure_runner_session(&runner, &user_id, &session_id).await;
        let stream = match runner.run(uid, sid, content).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                return;
            }
        };

        futures::pin_mut!(stream);
        let mut slide_count: Option<u32> = None;
        let mut combined_path: Option<PathBuf> = None;

        while let Some(result) = stream.next().await {
            let event = match result {
                Ok(ev) => ev,
                Err(e) => {
                    let _ = tx
                        .send(Ok(to_event(&FieldEvent::Error {
                            message: e.to_string(),
                        })))
                        .await;
                    break;
                }
            };

            if event.author != "combine_agent" {
                continue;
            }

            if let Some(content) = event.content() {
                for part in &content.parts {
                    if let adk_core::Part::FunctionResponse { function_response, .. } = part {
                        let tool = function_response.name.as_str();
                        if tool == "describe_presentation" {
                            slide_count =
                                parse_slide_count(&function_response.response).or(slide_count);
                        }
                        if tool == "save_presentation" {
                            combined_path = parse_saved_path(&function_response.response)
                                .or_else(|| newest_artifact_since(&session_dir, "pptx", started));
                        }
                    }
                }
            }
        }

        if combined_path.is_none() {
            combined_path = newest_artifact_since(&session_dir, "pptx", started);
        }
        // No deck saved by this run: say so. Announcing "Deck ready" would point at the
        // uncombined slides (rule 1).
        let Some(combined) = combined_path else {
            tracing::warn!("combine produced no deck — leaving the separate files as they are");
            let _ = tx
                .send(Ok(to_event(&FieldEvent::Error {
                    message: "I couldn't combine the deck this time — your numbers, story and slides are still separate files.".into(),
                })))
                .await;
            let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
            return;
        };

        let sub = finish_sub(slide_count);
        let url = artifacts::public_url(&user_id, &session_id, &combined, &artifact_root);

        let deck_finish = FieldEvent::DeckFinish {
            big: "Deck ready".into(),
            sub: sub.clone(),
            artifact_url: url.clone(),
            slide_count,
        };
        let _ = tx.send(Ok(to_event(&deck_finish))).await;

        let slides_card = serde_json::json!({
            "glyph":"🖼️","title":"Auto-Slides","agent":"auto-slides","surface":"slides"
        });
        let mut pinned_resolve = serde_json::json!({
            "big": "Deck ready",
            "sub": sub,
            "actions": ["Save deck", "Present"],
        });
        if let Some(ref u) = url {
            pinned_resolve["artifact_url"] = serde_json::Value::String(u.clone());
        }
        persist::card_resolve(
            &sessions,
            &session_id,
            2,
            slides_card,
            pinned_resolve,
            true,
        )
        .await;

        let mut updated = collect_artifacts(&session_dir);
        updated.combined_pptx = Some(combined.display().to_string());
        sessions.update_artifacts(&session_id, updated).await;

        tokio::time::sleep(Duration::from_millis(300)).await;
        let current = scenario.as_deref().unwrap_or("deck");
        coordinator::emit_tour_advance(&tx, current).await;
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    ReceiverStream::new(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_finish_never_invents_a_slide_count() {
        assert_eq!(finish_sub(None), "Numbers + story combined");
        assert_eq!(finish_sub(Some(12)), "12 slides · numbers + story combined");
    }

    #[test]
    fn an_older_deck_is_not_this_runs_output() {
        let dir = std::env::temp_dir().join(format!("combine-since-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("slides.pptx"), b"old").unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        assert!(newest_artifact_since(&dir, "pptx", later).is_none());
        assert!(newest_artifact_since(&dir, "pptx", std::time::UNIX_EPOCH).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
