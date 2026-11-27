//! Merging a worktree's branch into the main checkout: the host is asked how
//! the merge stands, the person is told what it brings and what it would
//! touch, and only then is it made. A merge that would conflict is not made
//! in the main checkout; it is offered to be resolved in the branch instead.
//! Once it is made, the worktree is offered for deleting.

use std::path::{Path, PathBuf};

use tsumugi_model::{EnvironmentId, MainChanges, MergeStanding, Merged};

use super::delete::called;
use crate::chrome::status_line;
use crate::effect::Effect;
use crate::overlay::confirm::Ask;
use crate::state::ViewState;
use crate::tree::{self, nav::Item};

/// Start merging the branch of the tree's row `row`: the host is asked how
/// the merge stands. Nothing is merged yet, and the main checkout is told
/// not to be a worktree to merge.
pub(crate) fn merge(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    let Item::Worktree(environment, path) = row.own() else {
        return Vec::new();
    };
    let tree = tree::listed(state.hosts.work(environment));
    let Some(w) = tree.worktrees().find(|w| w.worktree.path == *path) else {
        return Vec::new();
    };
    if w.worktree.is_main() {
        return status_line::notice(
            &mut state.chrome.status,
            "The main checkout is not merged into itself".into(),
        );
    }
    vec![Effect::WorktreeMergeInspect {
        environment: environment.clone(),
        path: path.clone(),
    }]
}

/// The host said how merging the branch of the worktree at `path` stands:
/// the question is asked, or what stops the merge is told.
pub(crate) fn inspected(
    state: &mut ViewState,
    environment: EnvironmentId,
    path: PathBuf,
    result: Result<MergeStanding, String>,
) -> Vec<Effect> {
    let standing = match result {
        Ok(standing) => standing,
        Err(why) => {
            return status_line::notice(
                &mut state.chrome.status,
                format!("cannot merge {}: {why}", called(None, &path)),
            );
        }
    };
    let name = called(standing.branch.as_deref(), &path);
    if let Some(why) = standing.refusal() {
        return status_line::notice(
            &mut state.chrome.status,
            format!("cannot merge {name}: {why}"),
        );
    }
    state.overlays.confirm = Some(if standing.conflicts.is_empty() {
        Ask::MergeWorktree {
            environment,
            path,
            name,
            standing,
            commit_main: false,
        }
    } else {
        Ask::ResolveMerge {
            environment,
            path,
            name,
            base: standing.base.unwrap_or_default(),
            files: standing.conflicts,
        }
    });
    Vec::new()
}

/// The choice of the question open flipped, when it is a merge's: whether
/// the main checkout's changes are committed rather than stashed.
pub(crate) fn toggle(state: &mut ViewState) -> Vec<Effect> {
    if let Some(Ask::MergeWorktree {
        standing,
        commit_main,
        ..
    }) = state.overlays.confirm.as_mut()
        && standing.main_dirty()
    {
        *commit_main = !*commit_main;
    }
    Vec::new()
}

/// What the question answered yes asks: the host merges, treating the main
/// checkout's changes as the question said and closing the issue it named, or
/// resolves the conflicts in the branch.
pub(crate) fn confirmed(ask: Ask) -> Vec<Effect> {
    match ask {
        Ask::MergeWorktree {
            environment,
            path,
            standing,
            commit_main,
            ..
        } => {
            let changes = match (standing.main_dirty(), commit_main) {
                (false, _) => MainChanges::Refuse,
                (true, false) => MainChanges::Stash,
                (true, true) => MainChanges::Commit,
            };
            vec![Effect::WorktreeMerge {
                environment,
                path,
                changes,
                close_issue: standing.issue.is_some(),
            }]
        }
        Ask::ResolveMerge {
            environment, path, ..
        } => vec![Effect::WorktreeResolve { environment, path }],
        _ => Vec::new(),
    }
}

/// The host answered merging the branch of the worktree at `path`: the
/// status line says what became of it, and a merged worktree is offered for
/// deleting.
pub(crate) fn merged(
    state: &mut ViewState,
    environment: EnvironmentId,
    path: &Path,
    result: Result<Merged, String>,
) -> Vec<Effect> {
    let name = called(None, path);
    match result {
        Ok(Merged::Done {
            branch,
            base,
            commit,
            stash,
            closed,
        }) => {
            let mut told = format!("Merged {branch} into {base} at {commit}");
            if let Some(issue) = closed {
                told.push_str(&format!("; closed #{}", issue.number));
                if !issue.others.is_empty() {
                    let count = issue.others.len();
                    told.push_str(&format!(
                        "; {count} other attempt{} offered for deleting next",
                        if count == 1 { " is" } else { "s are" }
                    ));
                }
                state.overlays.discards.extend(
                    issue
                        .others
                        .into_iter()
                        .map(|other| (environment.clone(), other)),
                );
            }
            if let Some(stash) = stash {
                told.push_str(&format!(
                    "; the changes in {base} are kept in the stash \"{stash}\""
                ));
            }
            let mut effects = status_line::notice(&mut state.chrome.status, told);
            effects.push(Effect::WorktreeInspect {
                environment,
                path: path.to_path_buf(),
            });
            effects
        }
        Ok(Merged::Conflicts { files }) => status_line::notice(
            &mut state.chrome.status,
            format!(
                "cannot merge {name}: a conflict in {}; nothing was touched",
                files.join(", ")
            ),
        ),
        Err(why) => status_line::notice(
            &mut state.chrome.status,
            format!("cannot merge {name}: {why}"),
        ),
    }
}

/// The host answered resolving the conflicts of the branch of the worktree
/// at `path`.
pub(crate) fn resolved(
    state: &mut ViewState,
    path: &Path,
    result: Result<String, String>,
) -> Vec<Effect> {
    let told = match result {
        Ok(told) => told,
        Err(why) => format!("cannot resolve {}: {why}", called(None, path)),
    };
    status_line::notice(&mut state.chrome.status, told)
}
