//! User-facing text and recorded thinking, from normalized transcript records.

use serde_json::Value;
use tsumugi_model::conversation::{Message, Role, Tool, kept};

#[derive(Debug, Clone, Default)]
pub(super) struct Conversation {
    pub messages: Vec<Message>,
}

/// What one record said, in the order it said it.
enum Part<'a> {
    Words { thinking: bool, text: &'a str },
    Call(&'a Value),
    Returned(&'a Value),
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
        let at_ms = record["timestamp"].as_str().and_then(crate::time::epoch_ms);
        let thinking = role == Role::Assistant && is_thinking(record);
        let content = &record["message"]["content"];
        let parts: Vec<Part> = match content {
            Value::String(s) => vec![Part::Words {
                thinking,
                text: s.as_str(),
            }],
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| match p["type"].as_str() {
                    Some("text") => p["text"]
                        .as_str()
                        .map(|text| Part::Words { thinking, text }),
                    Some("thinking") if role == Role::Assistant => {
                        p["thinking"].as_str().map(|text| Part::Words {
                            thinking: true,
                            text,
                        })
                    }
                    Some("tool_use") if role == Role::Assistant => Some(Part::Call(p)),
                    Some("tool_result") if role == Role::User => Some(Part::Returned(p)),
                    _ => None,
                })
                .collect(),
            _ => return,
        };
        // Words next to each other of one kind are one message.
        let mut spans: Vec<(bool, String)> = Vec::new();
        let flush = |this: &mut Self, spans: &mut Vec<(bool, String)>| {
            for (thinking, text) in spans.drain(..) {
                let text = super::without_reminders(&text);
                if text.trim().is_empty() || (role == Role::User && injected(text.trim())) {
                    continue;
                }
                this.messages.push(Message {
                    role,
                    text,
                    at_ms,
                    thinking,
                    tool: None,
                });
            }
        };
        for part in parts {
            match part {
                Part::Words { thinking, text } => {
                    if let Some((was, joined)) = spans.last_mut()
                        && *was == thinking
                    {
                        joined.push('\n');
                        joined.push_str(text);
                    } else {
                        spans.push((thinking, text.into()));
                    }
                }
                Part::Call(call) => {
                    flush(self, &mut spans);
                    self.call(
                        call["id"].as_str().unwrap_or_default(),
                        call["name"].as_str().unwrap_or_default(),
                        &call["input"],
                        at_ms,
                    );
                }
                Part::Returned(result) => {
                    flush(self, &mut spans);
                    self.returned(
                        result["tool_use_id"].as_str().unwrap_or_default(),
                        &result["content"],
                        result["is_error"] == true,
                    );
                }
            }
        }
        flush(self, &mut spans);
    }

    /// The assistant called tool `name` with `input`.
    pub fn call(&mut self, id: &str, name: &str, input: &Value, at_ms: Option<u64>) {
        if name.is_empty() {
            return;
        }
        self.messages.push(Message {
            role: Role::Assistant,
            text: String::new(),
            at_ms,
            thinking: false,
            tool: Some(Tool {
                id: id.into(),
                name: name.into(),
                summary: summary(name, input),
                input: detail(input),
                output: None,
                failed: false,
            }),
        });
    }

    /// The call `id` returned `content`; a result of a call not seen is not
    /// part of the conversation.
    pub fn returned(&mut self, id: &str, content: &Value, failed: bool) {
        let Some(tool) = self
            .messages
            .iter_mut()
            .rev()
            .filter_map(|m| m.tool.as_mut())
            .find(|t| t.id == id && !id.is_empty())
        else {
            return;
        };
        tool.output = Some(kept(&flatten(content)));
        tool.failed = failed;
    }
}

/// The words of a tool's result, whatever shape it was recorded in.
fn flatten(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str().or_else(|| p.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The call in one line: its most telling argument.
fn summary(name: &str, input: &Value) -> String {
    let text = match input {
        Value::String(s) => s.as_str(),
        Value::Object(map) => [
            "command",
            "cmd",
            "file_path",
            "path",
            "pattern",
            "query",
            "url",
            "description",
            "prompt",
        ]
        .iter()
        .find_map(|key| map.get(*key))
        .and_then(|v| match v {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        })
        .or_else(|| map.values().find_map(Value::as_str))
        .unwrap_or_default(),
        _ => "",
    };
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let line: String = line.trim().chars().take(120).collect();
    if line.is_empty() { name.into() } else { line }
}

/// The input as it is worth reading: a string as it is, an object as JSON.
fn detail(input: &Value) -> String {
    kept(&match input {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    })
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
