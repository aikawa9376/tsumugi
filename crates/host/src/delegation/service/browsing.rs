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
        BrowserResult::Image { data } => {
            let into = crate::paths::data_dir().join("screenshots");
            let saved = crate::environment::spawn_blocking(move || {
                crate::pasted::save(&into, "image/png", &data, std::time::SystemTime::now())
            })
            .await
            .map_err(Error::storage)?
            .map_err(|error| Error::new(Code::Storage, error.to_string()))?;
            Ok(Value::String(format!(
                "Saved the screenshot to {}",
                saved.display()
            )))
        }
        BrowserResult::Failed { text } => Err(Error::new(Code::Refused, text)),
    }
}

/// What the agent is handed for what the window read: a snapshot as the text
/// it is, anything else as the window said it.
fn render(command: &BrowserCommand, value: Value) -> Value {
    let BrowserCommand::Snapshot { .. } = command else {
        return value;
    };
    let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
    let (title, url, tree) = (text("title"), text("url"), text("snapshot"));
    Value::String(format!("Page: {title}\nURL: {url}\n\n{tree}"))
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
