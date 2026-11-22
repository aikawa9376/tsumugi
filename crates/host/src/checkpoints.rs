//! What each worktree says of how its work stands: one line and a status,
//! set by the session working in it, by `tsumugi worktree set` or by the
//! user, and told to every window as the sessions are read.

use std::path::{Path, PathBuf};

use tsumugi_model::{Set, WorktreeStatus, comment_of};
use tsumugi_protocol::{Answer, TreeChange};

use crate::command::Output;
use crate::delegation::app::{root, with_store};
use crate::watch::Watched;

pub(crate) async fn publish(watched: &Watched) -> Result<(), String> {
    let checkpoints = with_store(&root(), |store| store.checkpoints())
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::Checkpoints { checkpoints }]);
    Ok(())
}

/// Change what `worktree` says, the comment cut to its line, and tell every
/// window.
pub(crate) async fn set(
    watched: &Watched,
    worktree: PathBuf,
    comment: Set<String>,
    status: Set<WorktreeStatus>,
) -> Result<Answer, String> {
    let worktree = tsumugi_sys::fs::canonicalize(&worktree)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| format!("{} is no directory", worktree.display()))?;
    let comment = match comment {
        Set::To(text) => Set::To(comment_of(&text)),
        other => other,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    with_store(&root(), move |store| {
        store.set_checkpoint(&worktree, comment, status, now)
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

const USAGE: &str = "usage: tsumugi worktree set [--dir <path>] [--comment <text>] [--status <todo|in-progress|in-review|completed>] [--clear-comment] [--clear-status]; tsumugi worktree show [--dir <path>]";

/// What `tsumugi worktree set` was asked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Asked {
    pub dir: Option<PathBuf>,
    pub comment: Set<String>,
    pub status: Set<WorktreeStatus>,
    /// Only read what the worktree says.
    pub show: bool,
}

/// `args` as `set` or `show` and what follows; none for anything else.
pub(crate) fn parse(args: &[String]) -> Option<Asked> {
    let (verb, mut rest) = args.split_first()?;
    let show = match verb.as_str() {
        "set" => false,
        "show" => true,
        _ => return None,
    };
    let mut asked = Asked {
        dir: None,
        comment: Set::Keep,
        status: Set::Keep,
        show,
    };
    while let Some((word, after)) = rest.split_first() {
        rest = after;
        let mut value = || {
            let (value, after) = rest.split_first()?;
            rest = after;
            Some(value.clone())
        };
        match word.as_str() {
            "--dir" if asked.dir.is_none() => asked.dir = Some(PathBuf::from(value()?)),
            "--comment" | "--workspace-comment" if !show && asked.comment == Set::Keep => {
                asked.comment = Set::To(value()?)
            }
            "--status" | "--workspace-status" if !show && asked.status == Set::Keep => {
                asked.status = Set::To(WorktreeStatus::parse(&value()?)?)
            }
            "--clear-comment" if !show && asked.comment == Set::Keep => asked.comment = Set::Clear,
            "--clear-status" if !show && asked.status == Set::Keep => asked.status = Set::Clear,
            _ => return None,
        }
    }
    (show || asked.comment != Set::Keep || asked.status != Set::Keep).then_some(asked)
}

/// The command as the work host runs it for a command run in `cwd`.
pub(crate) async fn run(watched: &Watched, args: &[String], cwd: &Path) -> Output {
    let refused = |text: String, code| Output {
        wrote: Err(text),
        code,
        effects_possible: false,
    };
    let Some(asked) = parse(args) else {
        return refused(USAGE.into(), 2);
    };
    let dir = asked.dir.as_ref().map_or_else(
        || cwd.to_path_buf(),
        |dir| {
            if dir.is_absolute() {
                dir.clone()
            } else {
                cwd.join(dir)
            }
        },
    );
    if asked.show {
        let canonical = tsumugi_sys::fs::canonicalize(&dir).unwrap_or(dir);
        return match with_store(&root(), |store| store.checkpoints()).await {
            Ok(all) => {
                let line = all
                    .iter()
                    .find(|c| c.worktree == canonical)
                    .map(|c| c.line())
                    .unwrap_or_default();
                Output {
                    wrote: Ok(line),
                    code: 0,
                    effects_possible: false,
                }
            }
            Err(error) => refused(error.to_string(), 1),
        };
    }
    match set(watched, dir, asked.comment, asked.status).await {
        Ok(_) => Output {
            wrote: Ok(String::new()),
            code: 0,
            effects_possible: true,
        },
        Err(error) => Output {
            wrote: Err(error),
            code: 1,
            effects_possible: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(words: &[&str]) -> Option<Asked> {
        parse(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn a_comment_and_a_status_are_set_together_or_apart_by_orca_s_names_or_ours() {
        let both = asked(&["set", "--comment", "waiting on CI", "--status", "in-review"]).unwrap();
        assert_eq!(both.comment, Set::To("waiting on CI".into()));
        assert_eq!(both.status, Set::To(WorktreeStatus::InReview));
        let orca = asked(&[
            "set",
            "--workspace-comment",
            "x",
            "--workspace-status",
            "done",
        ])
        .unwrap();
        assert_eq!(orca.status, Set::To(WorktreeStatus::Completed));
        let status = asked(&["set", "--status", "todo", "--dir", "/w"]).unwrap();
        assert_eq!((status.comment, status.dir), (Set::Keep, Some("/w".into())));
    }

    #[test]
    fn a_part_is_cleared_by_name_and_set_or_cleared_only_once() {
        let cleared = asked(&["set", "--clear-comment", "--clear-status"]).unwrap();
        assert_eq!((cleared.comment, cleared.status), (Set::Clear, Set::Clear));
        for refused in [
            &["set"][..],
            &["set", "--status", "blocked"],
            &["set", "--comment"],
            &["set", "--comment", "a", "--clear-comment"],
            &["set", "--status", "todo", "--status", "done"],
            &["show", "--comment", "a"],
            &["set", "--dir", "/a", "--dir", "/b", "--comment", "x"],
            &["merge"],
        ] {
            assert!(asked(refused).is_none(), "{refused:?}");
        }
        assert!(asked(&["show"]).unwrap().show);
        assert!(asked(&["show", "--dir", "/w"]).is_some());
    }
}
