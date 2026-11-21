//! A Codex conversation found by the rollout it began, for a session whose
//! terminal title never names it.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::transcript;

/// The conversation that began in `cwd` at or after `since_ms` and that none
/// of `taken` holds, when exactly one did: with two, which is this session's
/// cannot be told, and the title has to say.
pub(super) fn find(root: &Path, cwd: &Path, since_ms: u64, taken: &[String]) -> Option<String> {
    let mut found: Option<String> = None;
    for path in recent(&root.join("sessions"), since_ms, 3) {
        let Some((id, began)) = began(&path, cwd) else {
            continue;
        };
        // The clock of the terminal and of the rollout's writer may differ a little.
        if began + 5_000 < since_ms || taken.contains(&id) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(id);
    }
    found
}

/// The rollouts under `dir` written to since `since_ms`.
fn recent(dir: &Path, since_ms: u64, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() && depth > 0 {
            out.extend(recent(&path, since_ms, depth - 1));
        } else if kind.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
            && entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|m| m.as_millis() as u64 + 5_000 >= since_ms)
        {
            out.push(path);
        }
    }
    out
}

/// The id and start of the conversation `path` records, when it ran in `cwd`.
fn began(path: &Path, cwd: &Path) -> Option<(String, u64)> {
    let mut first = String::new();
    BufReader::new(std::fs::File::open(path).ok()?)
        .read_line(&mut first)
        .ok()?;
    let meta: Value = serde_json::from_str(&first).ok()?;
    let meta = meta
        .get("payload")
        .filter(|_| meta["type"] == "session_meta")?;
    if meta["cwd"].as_str().map(Path::new) != Some(cwd) {
        return None;
    }
    if meta["source"]
        .as_str()
        .is_some_and(|s| s != "cli" && s != "vscode")
    {
        return None;
    }
    let id = meta["id"].as_str().filter(|id| transcript::thread_id(id))?;
    let at = meta["timestamp"].as_str().and_then(crate::time::epoch_ms)?;
    Some((id.to_string(), at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "019f7a4b-fee9-74d3-9977-5bc9bb134d11";
    const B: &str = "019f7a4c-0000-74d3-9977-5bc9bb134d22";
    // 2026-11-20T10:00:00Z
    const T0: u64 = 1_795_168_800_000;

    fn write(root: &Path, id: &str, cwd: &str, at: &str, source: &str) {
        let folder = root.join("sessions/2026/11/20");
        std::fs::create_dir_all(&folder).unwrap();
        let meta = json!({"type":"session_meta","payload":{"id":id,"cwd":cwd,"timestamp":at,"source":source}});
        std::fs::write(
            folder.join(format!("rollout-2026-11-20T10-00-00-{id}.jsonl")),
            format!("{meta}\n"),
        )
        .unwrap();
    }

    #[test]
    fn the_one_conversation_begun_in_the_directory_after_the_session_started_is_its() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), A, "/src", "2026-11-20T10:00:03.000Z", "cli");
        write(root.path(), B, "/other", "2026-11-20T10:00:04.000Z", "cli");
        let cwd = Path::new("/src");
        assert_eq!(find(root.path(), cwd, 0, &[]), Some(A.into()));
        assert_eq!(find(root.path(), cwd, T0, &[]), Some(A.into()));
    }

    #[test]
    fn an_earlier_conversation_a_claimed_one_or_a_tie_is_not_guessed_at() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), A, "/src", "2026-11-20T10:00:03.000Z", "cli");
        let cwd = Path::new("/src");
        assert_eq!(find(root.path(), cwd, T0 + 600_000, &[]), None, "older");
        assert_eq!(find(root.path(), cwd, T0, &[A.into()]), None, "claimed");
        write(root.path(), B, "/src", "2026-11-20T10:00:05.000Z", "cli");
        assert_eq!(find(root.path(), cwd, T0, &[]), None, "two begun");
        assert_eq!(find(root.path(), cwd, T0, &[A.into()]), Some(B.into()));
    }

    #[test]
    fn a_conversation_another_program_ran_is_not_a_session_s() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), A, "/src", "2026-11-20T10:00:03.000Z", "exec");
        assert_eq!(find(root.path(), Path::new("/src"), T0, &[]), None);
    }
}
