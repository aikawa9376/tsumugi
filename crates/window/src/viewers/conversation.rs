//! A conversation read in order, with only thinking folded away.

use tsumugi_model::conversation::{Message, Role};

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
