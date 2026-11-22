//! The conversations Cursor CLI kept: a transcript for each, in a folder for
//! each project, and beside them the directory and the name it gave each.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::Value;
use tsumugi_model::{Agent, Past, Preview, Speaker, Turn, title_of};

/// How far into a transcript its first prompt is looked for.
const HEAD: u64 = 256 * 1024;

/// The name Cursor gives a conversation no one named.
const UNNAMED: &str = "New Agent";

/// The conversations Cursor kept under `root` that ran in `dir`, or in any
/// directory when none is given.
pub(super) fn read(root: &Path, dir: Option<&Path>) -> Vec<Past> {
    let chats = chats(&root.join("chats"));
    transcripts(&root.join("projects"))
        .into_iter()
        .filter_map(|(id, path)| {
            let meta = chats.get(&id)?;
            conversation(id, path, meta, dir)
        })
        .collect()
}

/// What Cursor wrote of a conversation beside it.
struct Meta {
    cwd: PathBuf,
    title: String,
}

/// The directory and name of each conversation, by its id.
fn chats(root: &Path) -> std::collections::HashMap<String, Meta> {
    let mut found = std::collections::HashMap::new();
    for chat in children(root)
        .into_iter()
        .flat_map(|folder| children(&folder))
    {
        let Some(id) = chat.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(meta) = read_meta(&chat.join("meta.json")) else {
            continue;
        };
        found.entry(id.to_string()).or_insert(meta);
    }
    found
}

/// A meta file is small; a larger one is not one.
const META: u64 = 64 * 1024;

fn read_meta(path: &Path) -> Option<Meta> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(META)
        .read_to_string(&mut text)
        .ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Some(Meta {
        cwd: PathBuf::from(v["cwd"].as_str().filter(|c| !c.is_empty())?),
        title: v["title"].as_str().unwrap_or_default().trim().to_string(),
    })
}

/// The transcripts under `projects`, by the id of the conversation each
/// holds.
fn transcripts(projects: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    for folder in children(projects) {
        for entry in children(&folder.join("agent-transcripts")) {
            let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let path = match entry.is_dir() {
                true => entry.join(format!("{name}.jsonl")),
                false => entry.clone(),
            };
            let id = path.file_stem().and_then(|n| n.to_str());
            if let Some(id) = id
                && path.extension().is_some_and(|e| e == "jsonl")
                && path.is_file()
            {
                found.push((id.to_string(), path));
            }
        }
    }
    found
}

fn children(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect()
}

fn conversation(id: String, path: PathBuf, meta: &Meta, dir: Option<&Path>) -> Option<Past> {
    if dir.is_some_and(|dir| dir != meta.cwd) {
        return None;
    }
    let file = File::open(&path).ok()?;
    let at_ms = file
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    let prompt = BufReader::new(file.take(HEAD))
        .split(b'\n')
        .map_while(Result::ok)
        .find_map(|line| match turn(&serde_json::from_slice(&line).ok()?)? {
            Turn {
                speaker: Speaker::User,
                text,
            } => Some(text),
            _ => None,
        })?;
    let named = Some(meta.title.as_str()).filter(|t| !t.is_empty() && *t != UNNAMED);
    Some(Past {
        agent: Agent::Cursor,
        id,
        cwd: meta.cwd.clone(),
        title: title_of(named.unwrap_or(&prompt)),
        at_ms,
        log: Some(path),
        ..Default::default()
    })
}

/// The turn `record` says, when it says one. Cursor wraps what the user
/// typed in a tag.
fn turn(record: &Value) -> Option<Turn> {
    let content = &record["message"]["content"];
    let (speaker, text) = match record["role"].as_str()? {
        "user" => {
            let text = crate::preview::text_of(content, "text")?;
            let text = text.trim();
            let text = text
                .strip_prefix("<user_query>")
                .and_then(|t| t.strip_suffix("</user_query>"))
                .unwrap_or(text);
            (Speaker::User, text.trim().to_string())
        }
        "assistant" => (Speaker::Agent, crate::preview::text_of(content, "text")?),
        _ => return None,
    };
    (!text.is_empty()).then_some(Turn { speaker, text })
}

/// What the conversation kept in the transcript at `path` says.
pub(super) fn preview(path: &Path) -> Option<Preview> {
    crate::preview::read(path, turn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn said(role: &str, text: &str) -> String {
        format!(
            "{}\n",
            json!({"role": role, "message": {"content": [{"type": "text", "text": text}]}})
        )
    }

    /// A conversation as Cursor keeps it: a transcript in its project's
    /// folder, and its meta in a folder of the chats.
    fn keep(root: &Path, id: &str, cwd: &str, title: &str, lines: &[String]) -> PathBuf {
        let transcript = root.join("projects/home-me-r/agent-transcripts").join(id);
        std::fs::create_dir_all(&transcript).unwrap();
        let path = transcript.join(format!("{id}.jsonl"));
        std::fs::write(&path, lines.concat()).unwrap();
        let chat = root.join("chats/abc123").join(id);
        std::fs::create_dir_all(&chat).unwrap();
        std::fs::write(
            chat.join("meta.json"),
            json!({"cwd": cwd, "title": title}).to_string(),
        )
        .unwrap();
        path
    }

    #[test]
    fn a_conversation_is_listed_with_the_directory_and_name_cursor_gave_it() {
        let root = tempfile::tempdir().unwrap();
        let path = keep(
            root.path(),
            "c1",
            "/r",
            "Fix login",
            &[said("user", "<user_query>\nfix the login\n</user_query>")],
        );
        let got = read(root.path(), Some(Path::new("/r")));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].agent, Agent::Cursor);
        assert_eq!(got[0].id, "c1");
        assert_eq!(got[0].cwd, PathBuf::from("/r"));
        assert_eq!(got[0].title, "Fix login");
        assert_eq!(got[0].log, Some(path));
        assert!(got[0].at_ms > 0);
        assert!(read(root.path(), Some(Path::new("/elsewhere"))).is_empty());
    }

    #[test]
    fn a_conversation_no_one_named_goes_by_its_first_prompt() {
        let root = tempfile::tempdir().unwrap();
        keep(
            root.path(),
            "c1",
            "/r",
            "New Agent",
            &[said("user", "<user_query>\nfix\nthe login\n</user_query>")],
        );
        assert_eq!(read(root.path(), None)[0].title, "fix the login");
    }

    #[test]
    fn a_conversation_without_a_prompt_or_a_directory_is_not_listed() {
        let root = tempfile::tempdir().unwrap();
        keep(root.path(), "empty", "/r", "x", &[said("assistant", "hi")]);
        keep(root.path(), "nowhere", "", "x", &[said("user", "hi")]);
        let lone = root.path().join("projects/p/agent-transcripts/lone");
        std::fs::create_dir_all(&lone).unwrap();
        std::fs::write(lone.join("lone.jsonl"), said("user", "hi")).unwrap();
        assert!(read(root.path(), None).is_empty());
        assert!(read(&root.path().join("missing"), None).is_empty());
    }

    #[test]
    fn a_flat_transcript_is_listed_too() {
        let root = tempfile::tempdir().unwrap();
        keep(root.path(), "c1", "/r", "x", &[]);
        let folder = root.path().join("projects/home-me-r/agent-transcripts");
        std::fs::remove_dir_all(folder.join("c1")).unwrap();
        std::fs::write(folder.join("c1.jsonl"), said("user", "hi")).unwrap();
        assert_eq!(read(root.path(), None).len(), 1);
    }

    #[test]
    fn a_preview_unwraps_what_the_user_typed() {
        let root = tempfile::tempdir().unwrap();
        let path = keep(
            root.path(),
            "c1",
            "/r",
            "x",
            &[
                said("user", "<user_query>\nhello\n</user_query>"),
                said("assistant", "hi"),
            ],
        );
        let got = preview(&path).unwrap();
        assert_eq!(got.first, "hello");
        assert_eq!(got.recent.len(), 2);
    }
}
