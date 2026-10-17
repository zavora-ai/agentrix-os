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
use crate::events::sse::{to_event, FieldEvent};
use crate::orchestrator::{coordinator, fallback, persist};
use crate::state::{SessionArtifacts, SessionStore};

fn deck_cards() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "glyph":"📊","title":"Auto-Excel","agent":"auto-excel","surface":"excel",
            "stream":["Pulling Q3 numbers…","Building the revenue chart…"]
        }),
        serde_json::json!({
            "glyph":"📝","title":"Auto-Docs","agent":"auto-docs","surface":"docs",
            "stream":["Drafting the narrative…","Tightening the story…"]
        }),
        serde_json::json!({
            "glyph":"🖼️","title":"Auto-Slides","agent":"auto-slides","surface":"slides",
            "stream":["Waiting for numbers & story…"],"waitsFor":2
        }),
    ]
}

fn card_index(author: &str) -> Option<usize> {
    match author {
        "excel_agent" => Some(0),
        "docs_agent" => Some(1),
        "slides_agent" => Some(2),
        _ => None,
    }
}

fn status_line(tool: &str) -> &'static str {
    match tool {
        "create_workbook" | "open_workbook" => "Pulling Q3 numbers…",
        "write_cells" | "write_column" | "write_row" | "add_chart" => "Building the revenue chart…",
        "save_workbook" => "Revenue model ready…",
        "create_document" => "Drafting the narrative…",
        "insert_paragraph" => "Tightening the story…",
        "save_document" => "Story ready…",
        "create_presentation" => "Composing slides…",
        "add_slide" => "Adding slide…",
        "save_presentation" => "Deck ready…",
        _ => "Working…",
    }
}

/// Card label for a saved artifact: what kind of file it is and its name. The slide count is the
/// number of `add_slide` calls seen in this run; nothing about the content is estimated (rule 1).
fn artifact_label(path: &Path, slides_added: u32) -> (String, String) {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("artifact");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "xlsx" => ("Workbook saved".into(), name.into()),
        "docx" => ("Document saved".into(), name.into()),
        "pptx" if slides_added > 0 => (format!("{slides_added} slides"), format!("Deck saved · {name}")),
        "pptx" => ("Deck saved".into(), name.into()),
        _ => ("Saved".into(), name.into()),
    }
}

fn parse_saved_path(response: &serde_json::Value, _artifact_dir: &Path) -> Option<PathBuf> {
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

    // Fallback: newest matching file in artifact dir
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

fn collect_session_artifacts(dir: &Path) -> SessionArtifacts {
    SessionArtifacts {
        xlsx: find_newest_artifact(dir, "xlsx").map(|p| p.display().to_string()),
        docx: find_newest_artifact(dir, "docx").map(|p| p.display().to_string()),
        pptx: find_newest_artifact(dir, "pptx").map(|p| p.display().to_string()),
        combined_pptx: None,
    }
}

pub fn stream_deck(
    runner: Arc<Runner>,
    user_id: String,
    session_id: String,
    intent: String,
    artifact_root: PathBuf,
    sessions: Option<SessionStore>,
    suzy_runner: Option<Arc<Runner>>,
) -> ReceiverStream<Result<axum::response::sse::Event, Infallible>> {
    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let session_dir = artifacts::session_dir(&artifact_root, &user_id, &session_id);
        if tokio::fs::create_dir_all(&session_dir).await.is_err() {
            return;
        }

        let cards = deck_cards();
        if let Some(ref store) = sessions {
            store
                .set_scenario(&session_id, "deck", Some(&intent))
                .await;
        }

        let _ = tx
            .send(Ok(to_event(&FieldEvent::Scenario {
                key: "deck".into(),
                text: intent.clone(),
                total_cards: cards.len(),
            })))
            .await;

        for (index, card) in cards.iter().enumerate() {
            if let Some(ref store) = sessions {
                persist::card_spawn(store, &session_id, index, card.clone()).await;
            }
            let _ = tx
                .send(Ok(to_event(&FieldEvent::CardSpawn {
                    index,
                    card: card.clone(),
                    domain: crate::domain::Domain::for_card("deck", card),
                })))
                .await;
        }

        let mut resolved: Vec<bool> = vec![false; 3];
        let mut slide_count = 0u32;

        let sibling_listing = list_sibling_artifacts(&session_dir).await;
        let prompt = format!(
            "{intent}\n\n\
             [Save files to: {}/]\n\
             [Sibling artifacts]\n{sibling_listing}\n\
             Build the pitch deck artifacts for this session.",
            session_dir.display()
        );

        let content = Content::new("user").with_text(&prompt);
        let uid = match UserId::try_from(user_id.as_str()) {
            Ok(u) => u,
            Err(e) => {
                let _ = tx.send(Ok(to_event(&FieldEvent::Error {
                    message: e.to_string(),
                })))
                .await;
                return;
            }
        };
        let sid = match SessionId::try_from(session_id.as_str()) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Ok(to_event(&FieldEvent::Error {
                    message: e.to_string(),
                })))
                .await;
                return;
            }
        };

        crate::agents::ensure_runner_session(&runner, &user_id, &session_id).await;
        let stream = match runner.run(uid, sid, content).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Ok(to_event(&FieldEvent::Error {
                    message: e.to_string(),
                })))
                .await;
                return;
            }
        };

        futures::pin_mut!(stream);

        while let Some(result) = stream.next().await {
            let event = match result {
                Ok(ev) => ev,
                Err(e) => {
                    let _ = tx.send(Ok(to_event(&FieldEvent::Error {
                        message: e.to_string(),
                    })))
                    .await;
                    break;
                }
            };

            let author = event.author.clone();
            let Some(index) = card_index(&author) else {
                continue;
            };

            if let Some(content) = event.content() {
                for part in &content.parts {
                    match part {
                        adk_core::Part::FunctionCall { name, .. } => {
                            let _ = tx
                                .send(Ok(to_event(&FieldEvent::CardStatus {
                                    index,
                                    status: "working".into(),
                                    line: Some(status_line(name).into()),
                                })))
                                .await;
                            if name == "add_slide" && index == 2 {
                                slide_count += 1;
                                let _ = tx
                                    .send(Ok(to_event(&FieldEvent::CardSurface {
                                        index,
                                        surface: "slides".into(),
                                        slide: slide_count,
                                        total: 10,
                                    })))
                                    .await;
                            }
                        }
                        adk_core::Part::FunctionResponse { function_response, .. } => {
                            let tool = function_response.name.as_str();
                            if matches!(tool, "save_workbook" | "save_document" | "save_presentation")
                            {
                                let path = parse_saved_path(&function_response.response, &session_dir)
                                    .or_else(|| {
                                        let ext = match tool {
                                            "save_workbook" => "xlsx",
                                            "save_document" => "docx",
                                            "save_presentation" => "pptx",
                                            _ => return None,
                                        };
                                        find_newest_artifact(&session_dir, ext)
                                    });

                                if let Some(path) = path {
                                    let (big, sub) = artifact_label(&path, slide_count);
                                    let url = artifacts::public_url(
                                        &user_id,
                                        &session_id,
                                        &path,
                                        &artifact_root,
                                    );
                                    let mut resolve = serde_json::json!({
                                        "big": big,
                                        "sub": sub,
                                        "actions": if index == 2 {
                                            vec!["Save deck", "Present"]
                                        } else {
                                            vec!["Save", "Open"]
                                        },
                                    });
                                    if let Some(url) = url {
                                        resolve["artifact_url"] = serde_json::Value::String(url);
                                    }
                                    if let Some(ref store) = sessions {
                                        let card = cards.get(index).cloned().unwrap_or_default();
                                        persist::card_resolve(
                                            store,
                                            &session_id,
                                            index,
                                            card,
                                            resolve.clone(),
                                            false,
                                        )
                                        .await;
                                    }
                                    let _ = tx
                                        .send(Ok(to_event(&FieldEvent::CardResolve {
                                            index,
                                            resolve,
                                        })))
                                        .await;
                                    resolved[index] = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Resolve any cards that didn't emit save events (scan directory)
        for (index, ext) in [(0, "xlsx"), (1, "docx"), (2, "pptx")] {
            if resolved[index] {
                continue;
            }
            let Some(path) = find_newest_artifact(&session_dir, ext) else {
                let resolve = fallback::unavailable(format!("No .{ext} file was saved this run"));
                if let Some(ref store) = sessions {
                    let card = cards.get(index).cloned().unwrap_or_default();
                    persist::card_resolve(store, &session_id, index, card, resolve.clone(), false).await;
                }
                let _ = tx
                    .send(Ok(to_event(&FieldEvent::CardResolve { index, resolve })))
                    .await;
                continue;
            };
            let (big, sub) = artifact_label(&path, slide_count);
            let url =
                artifacts::public_url(&user_id, &session_id, &path, &artifact_root);
            let mut resolve = serde_json::json!({
                "big": big,
                "sub": sub,
                "actions": if index == 2 { vec!["Save deck", "Present"] } else { vec!["Save", "Open"] },
            });
            if let Some(url) = url {
                resolve["artifact_url"] = serde_json::Value::String(url);
            }
            if let Some(ref store) = sessions {
                let card = cards.get(index).cloned().unwrap_or_default();
                persist::card_resolve(
                    store,
                    &session_id,
                    index,
                    card,
                    resolve.clone(),
                    false,
                )
                .await;
            }
            let _ = tx
                .send(Ok(to_event(&FieldEvent::CardResolve { index, resolve })))
                .await;
        }

        if let Some(ref store) = sessions {
            store
                .update_artifacts(&session_id, collect_session_artifacts(&session_dir))
                .await;
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
        if let Some(ref store) = sessions {
            coordinator::emit_suzy_and_suggest(
                &tx,
                suzy_runner.as_ref(),
                store,
                &session_id,
                &user_id,
                "deck",
            )
            .await;
        }
        let _ = tx.send(Ok(to_event(&FieldEvent::Done))).await;
    });

    ReceiverStream::new(rx)
}

async fn list_sibling_artifacts(dir: &Path) -> String {
    let mut lines = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                lines.push(format!("- {}", path.display()));
            }
        }
    }
    if lines.is_empty() {
        "(none yet — excel and docs agents run first)".into()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_labels_state_only_what_was_observed() {
        assert_eq!(artifact_label(Path::new("/a/model.xlsx"), 0), ("Workbook saved".into(), "model.xlsx".into()));
        assert_eq!(artifact_label(Path::new("/a/story.docx"), 0).0, "Document saved");
        assert_eq!(artifact_label(Path::new("/a/deck.pptx"), 0).0, "Deck saved");
        assert_eq!(artifact_label(Path::new("/a/deck.pptx"), 7).0, "7 slides");
    }
}
