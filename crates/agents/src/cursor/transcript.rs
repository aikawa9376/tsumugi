//! Read a Cursor CLI transcript into the shared link collector, as the file
//! grows. Cursor records the user's words, its replies and the calls it made,
//! with no time, no call ids and no results.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::ConversationReader;
use crate::links::{Collector, Links};
use crate::tail::{Follow, Tail};

pub(super) struct Transcript {
    root: PathBuf,
    id: String,
    path: Option<PathBuf>,
    tail: Tail,
    found: Collector,
    /// How many calls were taken in, which names the next.
    calls: usize,
    /// The calls nothing has come after yet.
    open: Vec<String>,
}

impl Transcript {
    /// The reader of the conversation `id`, kept under `root`, of a session
    /// in `cwd`, `~` standing for `home`.
    pub(super) fn open(root: PathBuf, id: String, cwd: &Path, home: &Path) -> Self {
        Self {
            root,
            id,
            path: None,
            tail: Tail::default(),
            found: Collector::new(cwd, home),
            calls: 0,
            open: Vec::new(),
        }
    }

    fn refresh(&mut self) {
        if !self.path.as_ref().is_some_and(|p| p.is_file()) {
            self.path = super::past::transcript(&self.root, &self.id);
        }
        self.tail.read_at(self.path.as_deref()).feed(self);
    }

    /// The calls before what came next ended, for Cursor tells no result:
    /// one is not left running for good.
    fn close(&mut self) {
        for id in std::mem::take(&mut self.open) {
            self.found.returned(&id, &json!(""), false);
        }
    }
}

/// What the user typed, out of the tags Cursor puts it and its context in.
fn typed(text: &str) -> &str {
    match text.split_once("<user_query>") {
        Some((_, rest)) => rest
            .split_once("</user_query>")
            .map_or(rest, |(query, _)| query)
            .trim(),
        None => text.trim(),
    }
}

impl Follow for Transcript {
    fn restart(&mut self) {
        self.found.clear();
        self.calls = 0;
        self.open.clear();
    }

    fn take(&mut self, record: &Value) {
        self.close();
        let role = match record["role"].as_str() {
            Some(role @ ("user" | "assistant")) => role,
            _ => return,
        };
        let parts: Vec<Value> = match &record["message"]["content"] {
            Value::String(text) => vec![json!({"type":"text", "text":text})],
            Value::Array(parts) => parts
                .iter()
                .filter_map(|p| match p["type"].as_str() {
                    Some("text") => {
                        let text = p["text"].as_str()?;
                        let text = if role == "user" { typed(text) } else { text };
                        Some(json!({"type":"text", "text":text}))
                    }
                    Some("tool_use") if role == "assistant" => {
                        self.calls += 1;
                        let id = format!("cursor-{}", self.calls);
                        self.open.push(id.clone());
                        Some(json!({"type":"tool_use", "id":id,
                            "name":p["name"], "input":p["input"]}))
                    }
                    _ => None,
                })
                .collect(),
            _ => return,
        };
        self.found
            .said(role, &Value::Null, json!(parts), &|p| p.is_file());
    }
}

impl ConversationReader for Transcript {
    fn messages(&self) -> Vec<tsumugi_model::conversation::Message> {
        self.found.messages()
    }
    fn read(&mut self) -> Links {
        self.refresh();
        self.found.links()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::append;
    use tsumugi_model::conversation::Role;

    fn line(v: Value) -> String {
        format!("{v}\n")
    }

    /// A transcript of `lines` kept as Cursor keeps conversation `id`.
    fn kept(root: &Path, id: &str, lines: &[String]) -> PathBuf {
        let folder = root.join("projects/home-me-r/agent-transcripts").join(id);
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(format!("{id}.jsonl"));
        std::fs::write(&path, lines.concat()).unwrap();
        path
    }

    #[test]
    fn the_conversation_shows_what_the_user_typed_the_replies_and_the_calls() {
        let root = tempfile::tempdir().unwrap();
        let path = kept(
            root.path(),
            "c1",
            &[
                line(json!({"role":"user","message":{"content":[{"type":"text",
                    "text":"<timestamp>Tuesday, Jun 2, 2026, 11:20 AM</timestamp>\n<user_query>\nfix the login\n</user_query>"}]}})),
                line(json!({"role":"assistant","message":{"content":[
                    {"type":"text","text":"Looking."},
                    {"type":"tool_use","name":"Shell","input":{"command":"cargo test"}},
                    {"type":"tool_use","name":"Read","input":{"path":"/r/a.rs"}}]}})),
            ],
        );
        let mut reader =
            Transcript::open(root.path().into(), "c1".into(), root.path(), root.path());
        reader.read();
        let messages = reader.messages();
        assert_eq!(
            messages
                .iter()
                .map(|m| (
                    m.role,
                    m.text.as_str(),
                    m.tool.as_ref().map(|t| t.summary.as_str())
                ))
                .collect::<Vec<_>>(),
            [
                (Role::User, "fix the login", None),
                (Role::Assistant, "Looking.", None),
                (Role::Assistant, "", Some("cargo test")),
                (Role::Assistant, "", Some("/r/a.rs")),
            ]
        );
        assert!(
            messages[2].tool.as_ref().unwrap().output.is_none(),
            "the last calls run"
        );
        let ids: Vec<_> = messages
            .iter()
            .filter_map(|m| m.tool.as_ref())
            .map(|t| &t.id)
            .collect();
        assert_ne!(ids[0], ids[1]);

        append(
            &path,
            &line(json!({"type":"turn_ended","status":"success"})),
        );
        reader.read();
        assert!(
            reader
                .messages()
                .iter()
                .filter_map(|m| m.tool.as_ref())
                .all(|t| t.output.is_some()),
            "what comes after a call ends it"
        );
        reader.read();
        assert_eq!(reader.messages().len(), 4);
    }

    #[test]
    fn a_reader_waits_for_its_transcript() {
        let root = tempfile::tempdir().unwrap();
        let mut reader =
            Transcript::open(root.path().into(), "c1".into(), root.path(), root.path());
        reader.read();
        assert!(reader.messages().is_empty());
        kept(
            root.path(),
            "c1",
            &[line(
                json!({"role":"user","message":{"content":[{"type":"text","text":"hi"}]}}),
            )],
        );
        reader.read();
        assert_eq!(reader.messages()[0].text, "hi");
    }
}
