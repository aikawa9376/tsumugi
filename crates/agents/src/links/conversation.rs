//! User-facing text and recorded thinking, from normalized transcript records.

use serde_json::Value;
use tsumugi_model::conversation::{Message, Role};

#[derive(Debug, Clone, Default)]
pub(super) struct Conversation {
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn take(&mut self, record: &Value) {
        let role = match record["type"].as_str() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => return,
        };
        if record["isMeta"] == true || record["origin"]["kind"] == "peer" {
            return;
        }
        let thinking = role == Role::Assistant && is_thinking(record);
        let content = &record["message"]["content"];
        let parts: Vec<_> = match content {
            Value::String(s) => vec![(thinking, s.as_str())],
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| match p["type"].as_str() {
                    Some("text") => p["text"].as_str().map(|s| (thinking, s)),
                    Some("thinking") if role == Role::Assistant => {
                        p["thinking"].as_str().map(|s| (true, s))
                    }
                    _ => None,
                })
                .collect(),
            _ => return,
        };
        let mut spans: Vec<(bool, String)> = Vec::new();
        for (thinking, text) in parts {
            if let Some((was, joined)) = spans.last_mut()
                && *was == thinking
            {
                joined.push('\n');
                joined.push_str(text);
            } else {
                spans.push((thinking, text.into()));
            }
        }
        for (thinking, text) in spans {
            let text = super::without_reminders(&text);
            if text.trim().is_empty() || (role == Role::User && injected(text.trim())) {
                continue;
            }
            self.messages.push(Message {
                role,
                text,
                at_ms: record["timestamp"].as_str().and_then(crate::time::epoch_ms),
                thinking,
            });
        }
    }
}

pub(super) fn is_thinking(record: &Value) -> bool {
    record["message"]["phase"] == "analysis" || record["message"]["channel"] == "analysis"
}

fn injected(text: &str) -> bool {
    [
        "# AGENTS.md instructions for ",
        "<environment_context>",
        "<permissions instructions>",
        "<turn_aborted>",
        "<command-",
        "<local-command",
        "<teammate-message",
        "[message] from ",
        "[Request interrupted by user",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}
