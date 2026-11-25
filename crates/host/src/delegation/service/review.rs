//! A session's agent reading its worktree's review and leaving notes on the
//! lines of its change, kept with the user's comments.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tsumugi_model::repository::{Listing, Rev, Spot};
use tsumugi_model::review::{Anchor, Change, NewComment, Side};

use super::serve::answer;
use crate::delegation::{Code, Error, Result, Session, protocol};

fn invalid(text: impl Into<String>) -> Error {
    Error::new(Code::Invalid, text.into())
}

/// The top of the worktree `dir` names, or the caller works in.
async fn worktree_of(caller: &Session, dir: Option<&PathBuf>) -> Result<PathBuf> {
    let dir = match dir {
        Some(dir) if dir.is_absolute() => dir.clone(),
        Some(dir) => caller.cwd.join(dir),
        None => caller.cwd.clone(),
    };
    tsumugi_vcs::inspect(&dir)
        .await
        .map(|w| w.path)
        .ok_or_else(|| invalid(format!("{} is in no Git worktree", dir.display())))
}

/// How a note's author is named: the session's code, then its name.
fn author(caller: &Session) -> String {
    format!("{} {}", caller.code, caller.name)
}

/// The change `named`: the worktree against the branch's base where the
/// branch can be compared, what is not committed, or a commit.
async fn change_of(worktree: &Path, named: Option<&str>) -> Result<Rev> {
    match named.map(str::trim).filter(|n| !n.is_empty()) {
        None | Some("branch") => Ok(tsumugi_vcs::branch::standing(worktree)
            .await
            .map_or(Rev::WorkingCopy, |s| Rev::Base(s.merge_base))),
        Some("uncommitted") => Ok(Rev::WorkingCopy),
        Some(commit) => tsumugi_vcs::git::commit_id(worktree, commit)
            .await
            .map(Rev::Commit)
            .map_err(|e| invalid(e.to_string())),
    }
}

/// The side a note is on, as written.
fn side_of(word: Option<&str>) -> Result<Side> {
    match word.map(str::trim) {
        None | Some("new") => Ok(Side::New),
        Some("old") => Ok(Side::Old),
        Some(other) => Err(invalid(format!("{other} is no side; use new or old"))),
    }
}

/// Every comment and note of the caller's worktree, judged against its diff.
pub(super) async fn read(caller: &Session, asked: protocol::ReadReview) -> Result<Value> {
    let worktree = worktree_of(caller, asked.dir.as_ref()).await?;
    let directory = crate::paths::instance_state();
    let kept = worktree.clone();
    let comments =
        tokio::task::spawn_blocking(move || crate::review_comments::load(&directory, &kept))
            .await
            .map_err(Error::storage)?
            .map_err(|e| Error::new(Code::Storage, e))?;
    let judged = crate::review_comments::judge(&worktree, comments).await;
    answer(judged)
}

/// Leave the note asked for, or change or delete one the caller left.
pub(super) async fn annotate(caller: &Session, asked: protocol::Annotate) -> Result<Value> {
    let worktree = worktree_of(caller, asked.dir.as_ref()).await?;
    let directory = crate::paths::instance_state();
    let by = author(caller);
    let change = match asked.id {
        Some(id) => {
            let (dir, kept) = (directory.clone(), worktree.clone());
            let comments =
                tokio::task::spawn_blocking(move || crate::review_comments::load(&dir, &kept))
                    .await
                    .map_err(Error::storage)?
                    .map_err(|e| Error::new(Code::Storage, e))?;
            let mine = comments
                .iter()
                .any(|c| c.id == id && c.author.as_deref() == Some(by.as_str()));
            if !mine {
                return Err(Error::new(
                    Code::Refused,
                    "only a note you left can be changed or deleted",
                ));
            }
            match (asked.delete, asked.text) {
                (true, _) => Change::Remove { id },
                (false, Some(text)) => Change::Edit { id, text },
                (false, None) => return Err(invalid("give the note's new text, or delete")),
            }
        }
        None => {
            let path = asked
                .path
                .filter(|p| !p.is_empty())
                .ok_or_else(|| invalid("name the file's path"))?;
            let start = asked
                .start
                .filter(|s| *s > 0)
                .ok_or_else(|| invalid("name the first line, from 1"))?;
            let end = asked.end.unwrap_or(start);
            if end < start {
                return Err(invalid("the last line comes before the first"));
            }
            let text = asked
                .text
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| invalid("write what the note says"))?;
            let anchor = Anchor {
                path,
                side: side_of(asked.side.as_deref())?,
                start,
                end,
            };
            let rev = change_of(&worktree, asked.change.as_deref()).await?;
            let quoted = quoted(&worktree, &rev, &anchor).await?;
            Change::Add(NewComment {
                rev,
                anchor,
                quoted,
                text,
                author: Some(by),
            })
        }
    };
    let kept = worktree.clone();
    let now = tokio::task::spawn_blocking(move || {
        crate::review_comments::change(&directory, &kept, change)
    })
    .await
    .map_err(Error::storage)?
    .map_err(|e| Error::new(Code::Invalid, e))?;
    answer(now)
}

/// The lines of `anchor` as the change `rev` has them, refused where they
/// are not in its line diff.
async fn quoted(worktree: &Path, rev: &Rev, anchor: &Anchor) -> Result<Vec<String>> {
    let not_there = || {
        invalid(format!(
            "{} has no lines {}-{} on the {} side in that change's diff",
            anchor.path,
            anchor.start,
            anchor.end,
            if anchor.side == Side::Old {
                "old"
            } else {
                "new"
            }
        ))
    };
    let files = match crate::details::load(Spot::Files(rev.clone()), worktree)
        .await
        .listing
    {
        Ok(Listing::Files(files)) => files,
        Ok(_) => Vec::new(),
        Err(e) => return Err(invalid(e)),
    };
    let file = files
        .into_iter()
        .find(|f| f.path == anchor.path)
        .ok_or_else(|| invalid(format!("{} is not changed in that change", anchor.path)))?;
    let lines = match crate::details::load(Spot::Lines(rev.clone(), file), worktree)
        .await
        .listing
    {
        Ok(Listing::Diff(lines)) => lines,
        _ => return Err(not_there()),
    };
    tsumugi_model::review::text_at(&lines, anchor).ok_or_else(not_there)
}

/// The file the caller's agent is about to write, or has just written, read
/// so the lines the write changes are marked the caller's. It answers at
/// once, whatever it finds: a file in no worktree is not followed.
pub(super) async fn writing(caller: &Session, asked: protocol::Writing) -> Result<Value> {
    let file = if asked.path.is_absolute() {
        asked.path
    } else {
        caller.cwd.join(asked.path)
    };
    let Some((worktree, path)) = place_of(&file).await else {
        return Ok(Value::Null);
    };
    let by = author(caller);
    let directory = crate::paths::instance_state();
    let kept = tokio::task::spawn_blocking(move || {
        let text = match std::fs::read(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.to_string()),
        };
        if asked.done {
            crate::written_lines::after(&directory, &worktree, &path, &text, &asked.call, &by)
        } else {
            crate::written_lines::before(&directory, &worktree, &path, &text, &asked.call)
        }
    })
    .await
    .map_err(Error::storage)?;
    if let Err(why) = kept {
        tracing::debug!("the lines written could not be kept: {why}");
    }
    Ok(Value::Null)
}

/// The worktree `file` is in, and its path from the worktree's top, even
/// before the file and the folders it is to be written into exist.
async fn place_of(file: &Path) -> Option<(PathBuf, String)> {
    let mut there = file.parent()?;
    while !there.is_dir() {
        there = there.parent()?;
    }
    let worktree = tsumugi_vcs::inspect(there).await?.path;
    let top = std::fs::canonicalize(&worktree).ok()?;
    let rest = file.strip_prefix(there).ok()?;
    let file = std::fs::canonicalize(there).ok()?.join(rest);
    let path = file.strip_prefix(&top).ok()?;
    let path = path
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?
        .join("/");
    (!path.is_empty() && !path.starts_with(".git/")).then_some((worktree, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_on_the_new_side_unless_it_names_the_old() {
        assert_eq!(side_of(None).unwrap(), Side::New);
        assert_eq!(side_of(Some("new")).unwrap(), Side::New);
        assert_eq!(side_of(Some("old")).unwrap(), Side::Old);
        assert!(side_of(Some("left")).is_err());
    }

    #[tokio::test]
    async fn a_file_is_placed_in_its_worktree_before_it_or_its_folders_exist() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let git = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(git.success());
        let (worktree, path) = place_of(&repo.join("src/new/a.rs")).await.unwrap();
        assert_eq!(path, "src/new/a.rs");
        assert_eq!(
            std::fs::canonicalize(worktree).unwrap(),
            std::fs::canonicalize(&repo).unwrap()
        );
        assert_eq!(place_of(&dir.path().join("loose.txt")).await, None);
    }
}
