//! The review comments kept for each worktree, in the instance's state
//! folder, and how each stands against its file's diff as it is now.
//! Changes are made one at a time to what is kept, so windows commenting at
//! once lose nothing of each other's.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tsumugi_model::repository::{Change as FileChange, DiffLine, Listing, Rev, Spot};
use tsumugi_model::review::{Change, Comment, Judged};

/// Taken while what is kept is read, changed and written again.
static KEEPING: Mutex<()> = Mutex::new(());

const FILE: &str = "review-comments.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Kept {
    /// By the worktree's path.
    worktrees: BTreeMap<String, Vec<Comment>>,
    /// The id the next comment of each worktree is given, so one removed
    /// never comes back under another's name.
    #[serde(default)]
    next: BTreeMap<String, u64>,
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

/// The comments kept for `worktree`, in the order they were made.
pub fn load(directory: &Path, worktree: &Path) -> Result<Vec<Comment>, String> {
    let mut kept = read(directory)?;
    Ok(kept.worktrees.remove(&key(worktree)).unwrap_or_default())
}

fn key(worktree: &Path) -> String {
    worktree.to_string_lossy().into_owned()
}

/// Make `change` to the comments kept for `worktree`; what is then kept.
/// What cannot be read is left as it is rather than written over.
pub fn change(directory: &Path, worktree: &Path, change: Change) -> Result<Vec<Comment>, String> {
    let _keeping = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut kept = read(directory)?;
    let name = key(worktree);
    let next = kept.next.entry(name.clone()).or_insert(1);
    let comments = kept.worktrees.entry(name.clone()).or_default();
    let find = |comments: &mut Vec<Comment>, id: u64| {
        comments
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| "No comment has this id".to_string())
    };
    match change {
        Change::Add(new) => {
            let a = &new.anchor;
            let lines = a.end.checked_sub(a.start).map(|n| n as usize + 1);
            if a.path.is_empty() || a.start == 0 || lines != Some(new.quoted.len()) {
                return Err("The comment is not on lines of a file".into());
            }
            let text = new.text.trim();
            if text.is_empty() {
                return Err("Write what the comment says".into());
            }
            comments.push(Comment {
                id: *next,
                rev: new.rev,
                anchor: new.anchor,
                quoted: new.quoted,
                text: text.to_owned(),
                resolved: false,
                sent: false,
                author: new.author,
            });
            *next += 1;
        }
        Change::Edit { id, text } => {
            let text = text.trim();
            if text.is_empty() {
                return Err("Write what the comment says".into());
            }
            let at = find(comments, id)?;
            comments[at].text = text.to_owned();
        }
        Change::Remove { id } => {
            let at = find(comments, id)?;
            comments.remove(at);
        }
        Change::Resolve { id, resolved } => {
            let at = find(comments, id)?;
            comments[at].resolved = resolved;
        }
        Change::Sent { ids } => {
            for comment in comments.iter_mut().filter(|c| ids.contains(&c.id)) {
                comment.sent = true;
            }
        }
    }
    let now = comments.clone();
    keep(directory, &kept)?;
    Ok(now)
}

/// `comments`, each judged against its file's diff as it is now.
pub async fn judge(worktree: &Path, comments: Vec<Comment>) -> Vec<Judged> {
    let mut files: HashMap<Rev, Vec<FileChange>> = HashMap::new();
    let mut diffs: HashMap<(Rev, String), Vec<DiffLine>> = HashMap::new();
    let mut judged = Vec::new();
    for comment in comments {
        let rev = comment.rev.clone();
        if !files.contains_key(&rev) {
            let read = crate::details::load(Spot::Files(rev.clone()), worktree).await;
            let listed = match read.listing {
                Ok(Listing::Files(listed)) => listed,
                _ => Vec::new(),
            };
            files.insert(rev.clone(), listed);
        }
        let at = (rev.clone(), comment.anchor.path.clone());
        if !diffs.contains_key(&at) {
            let lines = match files[&rev].iter().find(|c| c.path == at.1) {
                Some(change) => {
                    let spot = Spot::Lines(rev.clone(), change.clone());
                    match crate::details::load(spot, worktree).await.listing {
                        Ok(Listing::Diff(lines)) => lines,
                        _ => Vec::new(),
                    }
                }
                None => Vec::new(),
            };
            diffs.insert(at.clone(), lines);
        }
        judged.push(Judged {
            freshness: tsumugi_model::review::judge(&comment, &diffs[&at]),
            comment,
        });
    }
    judged
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::review::{Anchor, Freshness, NewComment, Side};

    fn add(text: &str, path: &str, start: u32, end: u32) -> Change {
        Change::Add(NewComment {
            rev: Rev::WorkingCopy,
            anchor: Anchor {
                path: path.into(),
                side: Side::New,
                start,
                end,
            },
            quoted: (start..=end).map(|n| format!("line {n}")).collect(),
            text: text.into(),
            author: None,
        })
    }

    #[test]
    fn comments_are_kept_per_worktree_across_loads_with_ids_that_are_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (Path::new("/work/a"), Path::new("/work/日本語"));
        assert_eq!(load(dir.path(), a).unwrap(), []);
        let first = change(dir.path(), a, add("one", "src/メモ.rs", 1, 2)).unwrap();
        assert_eq!(first[0].id, 1);
        assert_eq!(first[0].text, "one");
        assert!(!first[0].resolved && !first[0].sent);
        change(dir.path(), b, add("elsewhere", "x", 1, 1)).unwrap();
        change(dir.path(), a, add("two", "y", 3, 3)).unwrap();
        change(dir.path(), a, Change::Remove { id: 2 }).unwrap();
        let ids: Vec<u64> = change(dir.path(), a, add("three", "y", 3, 3))
            .unwrap()
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, [1, 3]);
        assert_eq!(load(dir.path(), b).unwrap().len(), 1);
    }

    #[test]
    fn a_comment_is_edited_resolved_and_marked_sent_by_its_id_alone() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/work");
        change(dir.path(), w, add("one", "a", 1, 1)).unwrap();
        change(dir.path(), w, add("two", "a", 2, 2)).unwrap();
        change(
            dir.path(),
            w,
            Change::Edit {
                id: 1,
                text: "  edited \n".into(),
            },
        )
        .unwrap();
        change(
            dir.path(),
            w,
            Change::Resolve {
                id: 2,
                resolved: true,
            },
        )
        .unwrap();
        let kept = change(dir.path(), w, Change::Sent { ids: vec![1, 2] }).unwrap();
        assert_eq!(kept[0].text, "edited");
        assert!(kept[0].sent && !kept[0].resolved);
        assert!(kept[1].sent && kept[1].resolved);
        let kept = change(
            dir.path(),
            w,
            Change::Resolve {
                id: 2,
                resolved: false,
            },
        )
        .unwrap();
        assert!(!kept[1].resolved);
    }

    #[test]
    fn a_change_that_makes_no_sense_is_refused_and_keeps_what_was_kept() {
        let dir = tempfile::tempdir().unwrap();
        let w = Path::new("/work");
        change(dir.path(), w, add("one", "a", 1, 1)).unwrap();
        for bad in [
            add("   ", "a", 1, 1),
            add("x", "", 1, 1),
            add("x", "a", 0, 1),
            add("x", "a", 3, 2),
            Change::Edit {
                id: 9,
                text: "x".into(),
            },
            Change::Edit {
                id: 1,
                text: " ".into(),
            },
            Change::Remove { id: 9 },
            Change::Resolve {
                id: 9,
                resolved: true,
            },
        ] {
            assert!(change(dir.path(), w, bad.clone()).is_err(), "{bad:?}");
        }
        let mut two_lines = add("x", "a", 1, 2);
        if let Change::Add(new) = &mut two_lines {
            new.quoted.pop();
        }
        assert!(change(dir.path(), w, two_lines).is_err());
        assert_eq!(load(dir.path(), w).unwrap().len(), 1);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_written_over() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        let w = Path::new("/work");
        assert!(load(dir.path(), w).is_err());
        assert!(change(dir.path(), w, add("one", "a", 1, 1)).is_err());
        assert_eq!(std::fs::read(dir.path().join(FILE)).unwrap(), b"not json");
    }

    fn git(dir: &Path, args: &[&str]) {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .status()
                .unwrap()
                .success()
        );
    }

    fn comment_on(path: &str, rev: Rev, side: Side, line: u32, quoted: &str) -> Comment {
        Comment {
            id: 1,
            rev,
            anchor: Anchor {
                path: path.into(),
                side,
                start: line,
                end: line,
            },
            quoted: vec![quoted.into()],
            text: "t".into(),
            resolved: false,
            sent: false,
            author: None,
        }
    }

    #[tokio::test]
    async fn a_comment_stands_while_its_lines_do_and_is_outdated_once_they_change_or_go() {
        let dir = tempfile::tempdir().unwrap();
        let (root, file) = (dir.path(), dir.path().join("メモ.txt"));
        git(root, &["init", "-q"]);
        std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "first"]);
        std::fs::write(&file, "one\nTWO\nthree\n").unwrap();
        let stands = |judged: &[Judged]| judged.iter().map(|j| j.freshness).collect::<Vec<_>>();
        let on_new = comment_on("メモ.txt", Rev::WorkingCopy, Side::New, 2, "TWO");
        let on_old = comment_on("メモ.txt", Rev::WorkingCopy, Side::Old, 2, "two");
        let all = vec![on_new.clone(), on_old.clone()];
        assert_eq!(
            stands(&judge(root, all.clone()).await),
            [Freshness::Current, Freshness::Current]
        );
        // A line added above shifts the changed one: the comment does not
        // follow it, it is outdated. The old side, read from the commit the
        // change is against, is as it was.
        std::fs::write(&file, "zero\none\nTWO\nthree\n").unwrap();
        assert_eq!(
            stands(&judge(root, all.clone()).await),
            [Freshness::Outdated, Freshness::Current]
        );
        // The change is committed: the file no longer differs.
        git(root, &["commit", "-q", "-am", "second"]);
        assert_eq!(
            stands(&judge(root, all).await),
            [Freshness::Outdated, Freshness::Outdated]
        );
    }

    #[tokio::test]
    async fn a_comment_on_a_commit_reads_that_commit_s_diff_whatever_the_worktree_holds() {
        let dir = tempfile::tempdir().unwrap();
        let (root, file) = (dir.path(), dir.path().join("a.txt"));
        git(root, &["init", "-q"]);
        std::fs::write(&file, "one\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "first"]);
        std::fs::write(&file, "one\ntwo\n").unwrap();
        git(root, &["commit", "-q", "-am", "second"]);
        let id = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        std::fs::write(&file, "completely different\n").unwrap();
        let there = comment_on("a.txt", Rev::Commit(id.clone()), Side::New, 2, "two");
        let wrong = comment_on("a.txt", Rev::Commit(id), Side::New, 2, "three");
        let judged = judge(root, vec![there, wrong]).await;
        assert_eq!(judged[0].freshness, Freshness::Current);
        assert_eq!(judged[1].freshness, Freshness::Outdated);
    }
}
