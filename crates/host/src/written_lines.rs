//! The lines of each worktree's files that sessions' agents wrote, in the
//! instance's state folder: each file's lines as last read around an
//! agent's write, with who wrote each, carried to the file as it reads now
//! whenever they are asked for.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use similar::{Algorithm, DiffTag};
use tsumugi_model::review::{Written, WrittenRun};

/// Taken while what is kept is read, changed and written again.
static KEEPING: Mutex<()> = Mutex::new(());

const FILE: &str = "written-lines.json";

/// A file larger than this is not followed: diffing it would hold up the
/// agent's turn.
pub const MAX_BYTES: usize = 8 << 20;

/// How long a diff of one file may take before what it found is used.
const DIFF_TIME: Duration = Duration::from_secs(1);

#[derive(Debug, Default, Serialize, Deserialize)]
struct Kept {
    /// By the worktree's path.
    worktrees: BTreeMap<String, Tree>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Tree {
    /// Who wrote, as [`File::by`] counts them.
    authors: Vec<String>,
    /// By the path from the top of the worktree.
    files: BTreeMap<String, File>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    /// What each line read, as [`hash`] gives it.
    lines: Vec<u64>,
    /// Who wrote each line, by its place in [`Tree::authors`]; none for a
    /// person, or a line that was there before.
    by: Vec<Option<u32>>,
    /// The write the file was read before, until it is read after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    call: Option<String>,
}

fn read(directory: &Path) -> Result<Kept, String> {
    let path = directory.join(FILE);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("cannot read {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Kept::default()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn keep(directory: &Path, kept: &Kept) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(kept).map_err(|error| error.to_string())?;
    tsumugi_sys::fs::replace(directory.join(FILE), bytes, 0o600).map_err(|error| error.to_string())
}

fn key(worktree: &Path) -> String {
    worktree.to_string_lossy().into_owned()
}

/// What a line reads, short enough to keep for every line of a file.
fn hash(line: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in line {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn lines(text: &[u8]) -> Vec<u64> {
    let text = text.strip_suffix(b"\n").unwrap_or(text);
    if text.is_empty() {
        return Vec::new();
    }
    text.split(|b| *b == b'\n').map(hash).collect()
}

/// Who wrote each of `now`'s lines, carried from `was`'s by a diff of the
/// two: a line still there keeps its author; a line changed or added is
/// `new`'s. When `new` is none, a line added that reads as one removed
/// keeps that line's author, as it was moved rather than written.
fn carry(was: &File, now: &[u64], new: Option<u32>) -> Vec<Option<u32>> {
    let deadline = Instant::now() + DIFF_TIME;
    let ops =
        similar::capture_diff_slices_deadline(Algorithm::Myers, &was.lines, now, Some(deadline));
    let mut by = vec![new; now.len()];
    let mut removed: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut added = Vec::new();
    for op in ops {
        let (tag, old, fresh) = op.as_tag_tuple();
        match tag {
            DiffTag::Equal => {
                for (o, n) in old.zip(fresh) {
                    by[n] = was.by.get(o).copied().flatten();
                }
            }
            DiffTag::Delete | DiffTag::Insert | DiffTag::Replace => {
                for o in old {
                    if let Some(Some(author)) = was.by.get(o) {
                        removed.entry(was.lines[o]).or_default().push(*author);
                    }
                }
                added.extend(fresh);
            }
        }
    }
    if new.is_none() {
        for n in added {
            if let Some(author) = removed.get_mut(&now[n]).and_then(Vec::pop) {
                by[n] = Some(author);
            }
        }
    }
    by
}

/// Read `path` of `worktree` just before the agent's write `call`.
pub fn before(
    directory: &Path,
    worktree: &Path,
    path: &str,
    text: &[u8],
    call: &str,
) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Ok(());
    }
    let _keeping = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut kept = read(directory)?;
    let tree = kept.worktrees.entry(key(worktree)).or_default();
    let now = lines(text);
    let by = match tree.files.get(path) {
        Some(was) => carry(was, &now, None),
        None => vec![None; now.len()],
    };
    tree.files.insert(
        path.to_string(),
        File {
            lines: now,
            by,
            call: Some(call.to_string()),
        },
    );
    keep(directory, &kept)
}

/// Read `path` of `worktree` just after the agent's write `call`, made by
/// the session `by`: the lines it changed or added are that session's. A
/// write the file was not read before marks nothing.
pub fn after(
    directory: &Path,
    worktree: &Path,
    path: &str,
    text: &[u8],
    call: &str,
    by: &str,
) -> Result<(), String> {
    let _keeping = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut kept = read(directory)?;
    let Some(tree) = kept.worktrees.get_mut(&key(worktree)) else {
        return Ok(());
    };
    let Some(was) = tree
        .files
        .get(path)
        .filter(|f| f.call.as_deref() == Some(call))
    else {
        return Ok(());
    };
    if text.len() > MAX_BYTES {
        tree.files.remove(path);
        return keep(directory, &kept);
    }
    let author = match tree.authors.iter().position(|a| a == by) {
        Some(at) => at,
        None => {
            tree.authors.push(by.to_string());
            tree.authors.len() - 1
        }
    };
    let now = lines(text);
    let marks = carry(was, &now, Some(author as u32));
    if marks.iter().all(Option::is_none) {
        tree.files.remove(path);
    } else {
        tree.files.insert(
            path.to_string(),
            File {
                lines: now,
                by: marks,
                call: None,
            },
        );
    }
    keep(directory, &kept)
}

/// The lines of `worktree`'s files its sessions' agents wrote, as each file
/// reads now: `read` gives a file's text by its path from the top of the
/// worktree, or none where it is gone.
pub fn load(
    directory: &Path,
    worktree: &Path,
    read_file: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<Vec<Written>, String> {
    let mut kept = read(directory)?;
    let Some(tree) = kept.worktrees.remove(&key(worktree)) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (path, file) in &tree.files {
        let Some(text) = read_file(path).filter(|t| t.len() <= MAX_BYTES) else {
            continue;
        };
        let by = carry(file, &lines(&text), None);
        let runs = runs(&by, &tree.authors);
        if !runs.is_empty() {
            out.push(Written {
                path: path.clone(),
                runs,
            });
        }
    }
    Ok(out)
}

fn runs(by: &[Option<u32>], authors: &[String]) -> Vec<WrittenRun> {
    let mut runs: Vec<WrittenRun> = Vec::new();
    for (i, author) in by.iter().enumerate() {
        let Some(name) = author.and_then(|a| authors.get(a as usize)) else {
            continue;
        };
        let line = i as u32 + 1;
        match runs.last_mut() {
            Some(run) if run.end + 1 == line && run.by == *name => run.end = line,
            _ => runs.push(WrittenRun {
                start: line,
                end: line,
                by: name.clone(),
            }),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Who wrote each line of `path` as `text` reads, `-` for a person.
    fn marks(dir: &Path, worktree: &Path, path: &str, text: &str) -> Vec<String> {
        let written = load(dir, worktree, |p| {
            (p == path).then(|| text.as_bytes().to_vec())
        })
        .unwrap();
        let count = lines(text.as_bytes()).len() as u32;
        (1..=count)
            .map(|n| {
                tsumugi_model::review::written_by(&written, path, n)
                    .unwrap_or("-")
                    .to_string()
            })
            .collect()
    }

    fn write(dir: &Path, w: &Path, path: &str, from: &str, to: &str, call: &str, by: &str) {
        before(dir, w, path, from.as_bytes(), call).unwrap();
        after(dir, w, path, to.as_bytes(), call, by).unwrap();
    }

    #[test]
    fn a_write_marks_the_lines_it_changed_or_added_and_no_other() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/w");
        let to = "a\nB\nc\nd\n";
        write(dir.path(), w, "f", "a\nb\nc\n", to, "t1", "s1");
        assert_eq!(marks(dir.path(), w, "f", to), ["-", "s1", "-", "s1"]);
    }

    #[test]
    fn a_file_written_anew_is_the_agent_s_wherever_it_differs() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/w");
        write(dir.path(), w, "new", "", "x\ny\n", "t1", "s1");
        assert_eq!(marks(dir.path(), w, "new", "x\ny\n"), ["s1", "s1"]);
        write(dir.path(), w, "new", "x\ny\n", "x\ny\nz", "t2", "s2");
        assert_eq!(marks(dir.path(), w, "new", "x\ny\nz"), ["s1", "s1", "s2"]);
    }

    #[test]
    fn a_person_s_edit_turns_the_line_back_and_undoing_it_brings_the_mark_back() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/w");
        write(dir.path(), w, "f", "", "a\nb\nc\n", "t1", "s1");
        assert_eq!(marks(dir.path(), w, "f", "a\nB\nc\n"), ["s1", "-", "s1"]);
        assert_eq!(marks(dir.path(), w, "f", "a\nb\nc\n"), ["s1", "s1", "s1"]);
    }

    #[test]
    fn a_line_moved_keeps_who_wrote_it_and_lines_added_around_it_are_the_person_s() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/w");
        write(dir.path(), w, "f", "p\n", "p\na\nb\n", "t1", "s1");
        assert_eq!(
            marks(dir.path(), w, "f", "b\nnew\np\na\n"),
            ["s1", "-", "-", "s1"]
        );
    }

    #[test]
    fn what_a_person_changed_between_two_writes_is_not_taken_by_the_second() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/w");
        write(dir.path(), w, "f", "", "a\nb\n", "t1", "s1");
        // The person changes b, then another session's agent adds c.
        write(dir.path(), w, "f", "a\nB\n", "a\nB\nc\n", "t2", "s2");
        assert_eq!(marks(dir.path(), w, "f", "a\nB\nc\n"), ["s1", "-", "s2"]);
    }

    #[test]
    fn a_write_the_file_was_not_read_before_marks_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/w");
        after(dir.path(), w, "f", b"a\n", "t1", "s1").unwrap();
        before(dir.path(), w, "f", b"a\n", "t2").unwrap();
        after(dir.path(), w, "f", b"a\nb\n", "t3", "s1").unwrap();
        assert_eq!(marks(dir.path(), w, "f", "a\nb\n"), ["-", "-"]);
    }

    #[test]
    fn the_marks_are_kept_per_worktree_and_a_file_gone_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let (w, other) = (Path::new("/w"), Path::new("/other"));
        write(dir.path(), w, "f", "", "a\n", "t1", "s1");
        assert_eq!(marks(dir.path(), other, "f", "a\n"), ["-"]);
        assert!(load(dir.path(), w, |_| None).unwrap().is_empty());
    }

    #[test]
    fn runs_join_the_lines_one_session_wrote_one_after_another() {
        let authors = ["s1".to_string(), "s2".to_string()];
        let by = [Some(0), Some(0), None, Some(0), Some(1)];
        let run = |start, end, by: &str| WrittenRun {
            start,
            end,
            by: by.into(),
        };
        assert_eq!(
            runs(&by, &authors),
            [run(1, 2, "s1"), run(4, 4, "s1"), run(5, 5, "s2")]
        );
    }
}
