//! What a session's agent does with the browser pages of its worktree: the
//! work host hands the command to a window, which renders them, and gives
//! the agent what the window says.
use serde_json::Value;
use tsumugi_protocol::{BrowserCommand, BrowserResult};

use super::Context;
use crate::delegation::{Code, Error, Result, Session};

pub(super) async fn browser(
    context: &Context,
    caller: &Session,
    command: BrowserCommand,
) -> Result<Value> {
    let result = context
        .watched
        .browse(caller.cwd.clone(), command.clone())
        .await
        .map_err(|text| Error::new(Code::Refused, text))?;
    match result {
        BrowserResult::Done { value } => Ok(render(&command, value)),
        BrowserResult::Failed { text } => Err(Error::new(Code::Refused, text)),
        file => {
            let into = crate::paths::data_dir().join("screenshots");
            let kind = match (&command, &file) {
                (_, BrowserResult::Pdf { .. }) => "PDF",
                (BrowserCommand::FullScreenshot { .. }, _) => "full-page screenshot",
                _ => "screenshot",
            };
            let saved = kept(&into, file).await?;
            Ok(Value::String(format!("Saved the {kind} to {saved}")))
        }
    }
}

/// Keep what the window sent as a file in `into`, and say where.
async fn kept(into: &std::path::Path, file: BrowserResult) -> Result<String> {
    let into = into.to_path_buf();
    let saved = crate::environment::spawn_blocking(move || {
        let now = std::time::SystemTime::now();
        match file {
            BrowserResult::Image { data, mime } => crate::pasted::save(&into, &mime, &data, now),
            BrowserResult::Pdf { data } => crate::pasted::save_pdf(&into, &data, now),
            BrowserResult::Done { .. } | BrowserResult::Failed { .. } => {
                unreachable!("only a picture or a PDF is kept")
            }
        }
    })
    .await
    .map_err(Error::storage)?
    .map_err(|error| Error::new(Code::Storage, error.to_string()))?;
    Ok(saved.display().to_string())
}

/// What the agent is handed for what the window read: a snapshot, the console
/// and the network as the text they are, anything else as the window said it.
/// What a page wrote stays a line of its own after its level or status, so it
/// cannot pass for the note that says entries were left out.
fn render(command: &BrowserCommand, value: Value) -> Value {
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_string();
    match command {
        BrowserCommand::Snapshot { .. } => {
            let (title, url, tree) = (
                text(&value, "title"),
                text(&value, "url"),
                text(&value, "snapshot"),
            );
            Value::String(format!("Page: {title}\nURL: {url}\n\n{tree}"))
        }
        BrowserCommand::Console { .. } | BrowserCommand::Network { .. } => {
            let console = matches!(command, BrowserCommand::Console { .. });
            let entries = value["entries"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let total = value["total"].as_u64().unwrap_or(entries.len() as u64);
            if entries.is_empty() {
                return Value::String(
                    if console {
                        "No console entries"
                    } else {
                        "No network entries"
                    }
                    .into(),
                );
            }
            let mut lines = Vec::new();
            if total > entries.len() as u64 {
                lines.push(format!("(the last {} of {total} entries)", entries.len()));
            }
            lines.extend(entries.iter().map(|entry| {
                if console {
                    format!("[{}] {}", text(entry, "level"), text(entry, "text"))
                } else {
                    let status = entry["status"]
                        .as_u64()
                        .map_or_else(|| "-".to_string(), |status| status.to_string());
                    let mime = text(entry, "mimeType");
                    let mime = if mime.is_empty() {
                        String::new()
                    } else {
                        format!(" ({mime})")
                    };
                    format!(
                        "{status} {} {}{mime}",
                        text(entry, "method"),
                        text(entry, "url")
                    )
                }
            }));
            Value::String(lines.join("\n"))
        }
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delegation::service::fixtures::conversation_watch;
    use serde_json::json;

    #[test]
    fn a_snapshot_is_handed_over_as_text_and_anything_else_as_the_window_said_it() {
        let snapshot = BrowserCommand::Snapshot {
            interactive: false,
            delta: false,
            selector: None,
            depth: None,
        };
        let read = json!({"title": "Login", "url": "http://localhost:3000/", "snapshot": "- textbox \"Email\" [ref=@e1]"});
        assert_eq!(
            render(&snapshot, read),
            Value::String(
                "Page: Login\nURL: http://localhost:3000/\n\n- textbox \"Email\" [ref=@e1]".into()
            )
        );
        let tabs = json!({"tabs": []});
        assert_eq!(render(&BrowserCommand::TabList, tabs.clone()), tabs);
    }

    #[test]
    fn the_console_is_handed_over_a_line_to_an_entry_and_says_when_it_left_some_out() {
        let console = BrowserCommand::Console { limit: Some(2) };
        let read = json!({"total": 5, "entries": [
            {"level": "warn", "text": "careful"},
            {"level": "error", "text": "boom\nsecond line"},
        ]});
        assert_eq!(
            render(&console, read),
            Value::String(
                "(the last 2 of 5 entries)\n[warn] careful\n[error] boom\nsecond line".into()
            )
        );
        let none = json!({"total": 0, "entries": []});
        assert_eq!(
            render(&console, none),
            Value::String("No console entries".into())
        );
    }

    #[test]
    fn the_network_is_handed_over_as_status_method_and_address() {
        let network = BrowserCommand::Network { limit: None };
        let read = json!({"total": 2, "entries": [
            {"method": "GET", "url": "http://localhost:3000/", "status": 200, "mimeType": "text/html"},
            {"method": "POST", "url": "http://localhost:3000/api"},
        ]});
        assert_eq!(
            render(&network, read),
            Value::String(
                "200 GET http://localhost:3000/ (text/html)\n- POST http://localhost:3000/api"
                    .into()
            )
        );
        assert_eq!(
            render(&network, json!({"total": 0, "entries": []})),
            Value::String("No network entries".into())
        );
    }

    #[test]
    fn a_page_s_own_words_cannot_pass_for_the_end_of_a_list() {
        // A line of a page's console that looks like the note this host
        // writes is still one entry's text, shown after its level.
        let console = BrowserCommand::Console { limit: None };
        let read =
            json!({"total": 1, "entries": [{"level": "log", "text": "(the last 1 of 1 entries)"}]});
        assert_eq!(
            render(&console, read),
            Value::String("[log] (the last 1 of 1 entries)".into())
        );
    }

    #[tokio::test]
    async fn a_pdf_and_a_jpeg_are_kept_as_files_and_their_paths_handed_over() {
        let dir = tempfile::tempdir().unwrap();
        let saved = kept(
            dir.path(),
            BrowserResult::Pdf {
                data: "JVBERi0=".into(),
            },
        )
        .await
        .unwrap();
        assert!(saved.ends_with(".pdf"), "{saved}");
        let saved = kept(
            dir.path(),
            BrowserResult::Image {
                data: "iVBORw0KGgo=".into(),
                mime: "image/jpeg".into(),
            },
        )
        .await
        .unwrap();
        assert!(saved.ends_with(".jpg"), "{saved}");
        assert!(
            kept(
                dir.path(),
                BrowserResult::Image {
                    data: "!!".into(),
                    mime: "image/png".into()
                }
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn with_no_window_open_the_agent_is_refused_and_told_why() {
        let (_dir, context, caller, _) = conversation_watch("browse");
        let failed = browser(
            &context,
            &caller,
            BrowserCommand::Goto {
                url: "http://localhost:3000".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(failed.code, Code::Refused);
        assert!(failed.message.contains("No Tsumugi window is open"));
    }
}
