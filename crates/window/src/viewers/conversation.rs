//! A conversation read in order, with only thinking folded away.

use tsumugi_model::conversation::{Message, Role, Tool};

use super::{escape, render_post};

pub fn conversation_update_script(id: &str, message: &str) -> String {
    let script = include_str!("../../assets/js/conversation-update.js");
    format!("{{const id = {id:?}; const message = {message}; {script}}}")
}

pub fn conversation_fragment(messages: &[Message]) -> String {
    if messages.is_empty() {
        return "<p class=\"conversation-empty\">No conversation yet.</p>".into();
    }
    let mut out = String::new();
    let mut at = 0;
    while at < messages.len() {
        if let Some(tool) = &messages[at].tool {
            out.push_str(&call(tool, at));
            at += 1;
            continue;
        }
        if !messages[at].thinking {
            out.push_str(&message(&messages[at], at));
            at += 1;
            continue;
        }
        let end = messages[at..]
            .iter()
            .position(|m| !m.thinking)
            .map_or(messages.len(), |end| at + end);
        out.push_str(&format!("<details class=\"conversation-progress\" data-progress=\"{at}\"><summary>Thinking · {}</summary>", end - at));
        for (i, m) in messages.iter().enumerate().take(end).skip(at) {
            out.push_str(&message(m, i));
        }
        out.push_str("</details>");
        at = end;
    }
    out
}

/// A tool call as a folded line that opens to its input and output.
fn call(tool: &Tool, index: usize) -> String {
    let state = match (&tool.output, tool.failed) {
        (_, true) => "failed",
        (Some(_), false) => "done",
        (None, false) => "running",
    };
    let part = |label: &str, text: &str| {
        if text.trim().is_empty() {
            String::new()
        } else {
            format!(
                "<div class=\"conversation-tool-part\"><b>{label}</b><pre>{}</pre></div>",
                escape(text)
            )
        }
    };
    format!(
        "<details class=\"conversation-tool {state}\" data-progress=\"{index}\">\
        <summary><span class=\"conversation-tool-name\">{}</span>\
        <span class=\"conversation-tool-summary\">{}</span>\
        <span class=\"conversation-tool-state\">{state}</span></summary>{}{}</details>",
        escape(&tool.name),
        escape(&tool.summary),
        part("Input", &tool.input),
        part("Output", tool.output.as_deref().unwrap_or_default()),
    )
}

fn message(m: &Message, index: usize) -> String {
    let (role, label) = match m.role {
        Role::User => ("user", "You"),
        Role::Assistant => ("assistant", "Assistant"),
    };
    format!(
        "<section class=\"conversation-message {role}\" data-message=\"{index}\">\
        <header><b>{label}</b><button type=\"button\" data-conversation-copy=\"{}\">Copy</button></header>\
        <div class=\"conversation-text markdown-body\">{}</div></section>",
        escape(&m.text),
        render_post(&m.text)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(role: Role, text: &str, thinking: bool) -> Message {
        Message {
            role,
            text: text.into(),
            at_ms: None,
            thinking,
            tool: None,
        }
    }

    #[test]
    fn conversation_keeps_every_user_facing_reply_visible_including_progress() {
        let html = conversation_fragment(&[
            said(Role::User, "Fix it", false),
            said(Role::Assistant, "Checking", false),
            said(Role::Assistant, "Done **now**", false),
            said(Role::User, "Next", false),
            said(Role::Assistant, "Still working", false),
        ]);
        assert!(!html.contains("<details"));
        assert!(html.find("Checking").unwrap() < html.find("Done <strong>now</strong>").unwrap());
        assert!(html.contains("Still working"));
        assert!(html.find("Fix it").unwrap() < html.find("Next").unwrap());
    }

    #[test]
    fn a_tool_call_is_one_folded_line_that_escapes_what_it_ran_and_returned() {
        let html = conversation_fragment(&[
            said(Role::Assistant, "Running", false),
            Message {
                tool: Some(Tool {
                    id: "a".into(),
                    name: "Bash".into(),
                    summary: "echo <b>".into(),
                    input: "{\"command\":\"echo <b>\"}".into(),
                    output: Some("<script>x</script>".into()),
                    failed: true,
                }),
                ..said(Role::Assistant, "", false)
            },
            Message {
                tool: Some(Tool {
                    id: "b".into(),
                    name: "Read".into(),
                    summary: "file".into(),
                    input: String::new(),
                    output: None,
                    failed: false,
                }),
                ..said(Role::Assistant, "", false)
            },
        ]);
        assert!(html.contains("<details class=\"conversation-tool failed\" data-progress=\"1\">"));
        assert!(html.contains("<details class=\"conversation-tool running\" data-progress=\"2\">"));
        assert!(html.contains("echo &lt;b&gt;"));
        assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains(" open"));
        assert_eq!(html.matches("conversation-message").count(), 1);
    }

    #[test]
    fn conversation_folds_only_thinking_in_order_and_starts_it_closed() {
        let html = conversation_fragment(&[
            said(Role::Assistant, "First thought", true),
            said(Role::Assistant, "Second thought", true),
            said(Role::Assistant, "Checking", false),
            said(Role::Assistant, "Third thought", true),
            said(Role::Assistant, "Done", false),
        ]);
        assert!(html.contains("<summary>Thinking · 2</summary>"));
        assert!(html.contains("data-progress=\"3\""));
        assert_eq!(html.matches("<details").count(), 2);
        assert!(!html.contains(" open"));
        assert!(html.find("Second thought").unwrap() < html.find("</details>").unwrap());
        assert!(html.find("</details>").unwrap() < html.find("Checking").unwrap());
        assert!(html.rfind("</details>").unwrap() < html.find("Done").unwrap());
    }

    #[test]
    fn conversation_copy_preserves_markdown_and_cannot_escape_the_attribute() {
        let html =
            conversation_fragment(&[said(Role::User, "\" onmouseover=\"alert(1)\n`code`", false)]);
        assert!(
            html.contains("data-conversation-copy=\"&quot; onmouseover=&quot;alert(1)\n`code`\"")
        );
        assert!(!html.contains(" onmouseover=\"alert"));
        assert!(conversation_fragment(&[]).contains("No conversation yet"));
    }
}
