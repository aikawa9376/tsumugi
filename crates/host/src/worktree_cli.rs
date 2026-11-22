//! `tsumugi worktree merge`: merge a finished branch into the main checkout,
//! or read how that merge stands. The same asks the `merge_worktree` tool
//! makes for a session.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tsumugi_model::{MainChanges, Merged};

use crate::command::Output;

const USAGE: &str = "usage: tsumugi worktree merge [--dir <path> | --branch <name>] [--check] [--stash | --commit] [--message <text>] [--close-issue] [--remove] [--resolve]";

/// What `tsumugi worktree merge` was asked.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Asked {
    /// A directory of the worktree whose branch is merged; the one the
    /// command runs in when none is named.
    pub dir: Option<PathBuf>,
    /// The branch to merge, found among the repository's worktrees.
    pub branch: Option<String>,
    /// Only read how the merge stands.
    pub check: bool,
    pub changes: MainChanges,
    pub message: Option<String>,
    pub close_issue: bool,
    pub remove: bool,
    /// Bring the main checkout's branch into the branch and start a session
    /// to resolve what conflicts, instead of merging into the main checkout.
    pub resolve: bool,
}

pub(crate) fn parse(args: &[String]) -> Option<Asked> {
    let (verb, mut rest) = args.split_first()?;
    if verb != "merge" {
        return None;
    }
    let mut asked = Asked::default();
    let mut chose_changes = false;
    while let Some((word, after)) = rest.split_first() {
        rest = after;
        let mut value = || {
            let (value, after) = rest.split_first()?;
            rest = after;
            Some(value.clone())
        };
        match word.as_str() {
            "--dir" if asked.dir.is_none() && asked.branch.is_none() => {
                asked.dir = Some(PathBuf::from(value()?));
            }
            "--branch" if asked.branch.is_none() && asked.dir.is_none() => {
                asked.branch = Some(value()?);
            }
            "--check" if !asked.check => asked.check = true,
            "--stash" | "--commit" if !chose_changes => {
                chose_changes = true;
                asked.changes = if word == "--stash" {
                    MainChanges::Stash
                } else {
                    MainChanges::Commit
                };
            }
            "--message" if asked.message.is_none() => asked.message = Some(value()?),
            "--close-issue" if !asked.close_issue => asked.close_issue = true,
            "--remove" if !asked.remove => asked.remove = true,
            "--resolve" if !asked.resolve => asked.resolve = true,
            _ => return None,
        }
    }
    asked.consistent().then_some(asked)
}

impl Asked {
    /// Whether what is asked can be done together: reading changes nothing,
    /// and resolving merges the other way round, into the branch.
    pub(crate) fn consistent(&self) -> bool {
        let changes = self.changes != MainChanges::Refuse;
        let merging = changes || self.message.is_some() || self.close_issue || self.remove;
        !(self.check && (merging || self.resolve)) && !(self.resolve && merging)
    }
}

/// The command as the work host runs it for a command run in `cwd`.
pub(crate) async fn run(watched: &crate::watch::Watched, args: &[String], cwd: &Path) -> Output {
    let Some(asked) = parse(args) else {
        return Output {
            wrote: Err(USAGE.into()),
            code: 2,
            effects_possible: false,
        };
    };
    let effects_possible = !asked.check;
    let daemon = crate::daemon::daemon();
    let done = carry_out(
        &crate::delegation::app::root(),
        &daemon,
        Some(watched),
        &asked,
        cwd,
    )
    .await;
    if effects_possible && matches!(&done, Ok((told, _)) if told["merged"]["closed"].is_object()) {
        let _ = crate::local_issues::publish(watched).await;
    }
    if effects_possible {
        watched.relist();
        watched.read_again();
    }
    match done {
        Ok((told, code)) => Output {
            wrote: Ok(serde_json::to_string_pretty(&told).unwrap_or_default()),
            code,
            effects_possible,
        },
        Err(error) => Output {
            wrote: Err(error),
            code: 1,
            effects_possible,
        },
    }
}

/// The worktree `asked` names: by its directory, by its branch among the
/// repository's worktrees, or else the directory the command runs in.
async fn worktree_of(asked: &Asked, cwd: &Path) -> Result<PathBuf, String> {
    let absolute = |dir: &Path| {
        if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            cwd.join(dir)
        }
    };
    if let Some(branch) = &asked.branch {
        let all = tsumugi_vcs::worktree::all(cwd).await?;
        return all
            .into_iter()
            .find(|entry| entry.branch.as_deref() == Some(branch))
            .map(|entry| entry.path)
            .ok_or_else(|| format!("no worktree has the branch {branch} checked out"));
    }
    let dir = asked
        .dir
        .as_deref()
        .map_or_else(|| cwd.to_path_buf(), absolute);
    tsumugi_sys::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| format!("{} is no directory", dir.display()))
}

/// Do what `asked` says, and what it came to, with the exit code to end
/// with: 0 when it was done or only read, 3 when the merge would conflict
/// and nothing was touched.
pub(crate) async fn carry_out(
    state: &Path,
    daemon: &tsumugi_term::Daemon,
    watched: Option<&crate::watch::Watched>,
    asked: &Asked,
    cwd: &Path,
) -> Result<(Value, i32), String> {
    let path = worktree_of(asked, cwd).await?;
    if asked.check {
        let standing = crate::integrate::standing_in(state, &path).await?;
        return Ok((
            json!({"worktree": path, "refusal": standing.refusal(), "standing": standing}),
            0,
        ));
    }
    if asked.resolve {
        let told = crate::integrate::resolve(watched, &path).await?;
        return Ok((json!({"worktree": path, "resolve": told}), 0));
    }
    let merged = crate::integrate::merge_in(
        state,
        &path,
        asked.changes,
        asked.message.as_deref(),
        asked.close_issue,
    )
    .await?;
    match &merged {
        Merged::Conflicts { files } => Ok((
            json!({
                "worktree": path,
                "merged": merged,
                "note": format!(
                    "the merge would conflict in {} and nothing was touched; use --resolve to resolve it in the branch's worktree",
                    files.join(", ")
                ),
            }),
            3,
        )),
        Merged::Done { .. } => {
            let removal = if asked.remove {
                Some(crate::integrate::remove(daemon, &path).await)
            } else {
                None
            };
            Ok((
                json!({"worktree": path, "merged": merged, "removal": removal}),
                0,
            ))
        }
    }
}

/// `tsumugi worktree`, run in the work host of the instance.
pub fn cli(args: &[String]) -> i32 {
    if parse(args).is_none() {
        eprintln!("{USAGE}");
        return 2;
    }
    let mut ask = vec!["worktree".to_string()];
    ask.extend(args.iter().cloned());
    crate::work_host::run_command(ask, None)
}

#[cfg(test)]
mod tests;
