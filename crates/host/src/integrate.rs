//! Merging a finished branch into the main checkout for the window, the
//! command line and the agents that lead: what the merge would do, the merge
//! with the local issue it finishes closed, and the session that resolves
//! what would conflict.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tsumugi_model::{LinkedIssue, MainChanges, MergeStanding, Merged};

use crate::delegation::app::{root, with_store};

/// The issue the session of the worktree at `path` works on, when one does
/// and it is not closed.
async fn linked(root: &Path, path: &Path) -> Result<Option<(u64, LinkedIssue)>, String> {
    let dir = tsumugi_sys::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let found = with_store(root, {
        let dir = dir.clone();
        move |store| store.local_issue_worked_in(&dir)
    })
    .await
    .map_err(|error| error.to_string())?;
    Ok(found.map(|(number, issue)| {
        let others = issue
            .attempts
            .into_iter()
            .filter_map(|attempt| attempt.worktree)
            .filter(|worktree| !worktree.starts_with(&dir))
            .collect();
        (
            number,
            LinkedIssue {
                number,
                title: issue.title,
                others,
            },
        )
    }))
}

/// How merging the branch of the worktree at `path` into the main checkout
/// stands, with the issue the merge would close, read from the state under
/// `state`. Nothing is changed.
pub(crate) async fn standing_in(state: &Path, path: &Path) -> Result<MergeStanding, String> {
    let mut standing = tsumugi_vcs::integrate::standing(path).await?;
    standing.issue = linked(state, path).await?.map(|(_, issue)| issue);
    Ok(standing)
}

pub(crate) async fn standing(path: &Path) -> Result<MergeStanding, String> {
    standing_in(&root(), path).await
}

/// Merge the branch of the worktree at `path` into the main checkout as
/// `changes` says to treat the changes there, and, with `close_issue`, close
/// the issue the worktree's session works on once the merge is made. A merge
/// that would conflict, or that is refused, closes nothing.
pub(crate) async fn merge_in(
    state: &Path,
    path: &Path,
    changes: MainChanges,
    message: Option<&str>,
    close_issue: bool,
) -> Result<Merged, String> {
    // Read before the merge: nothing answers for the issue afterwards.
    let issue = linked(state, path).await?;
    let merged = tsumugi_vcs::integrate::merge(path, changes, message).await?;
    let Merged::Done {
        branch,
        base,
        commit,
        stash,
        ..
    } = merged
    else {
        return Ok(merged);
    };
    let mut closed = None;
    if close_issue && let Some((number, issue)) = issue {
        let selector = number.to_string();
        with_store(state, move |store| {
            let (_, found) = store.local_issue_named(&selector)?;
            store.close_merged_local_issue(&found.id)
        })
        .await
        .map_err(|error| {
            format!(
                "{branch} was merged into {base} at {commit}, but its issue is not closed: {error}"
            )
        })?;
        closed = Some(issue);
    }
    Ok(Merged::Done {
        branch,
        base,
        commit,
        stash,
        closed,
    })
}

pub(crate) async fn merge(
    path: &Path,
    changes: MainChanges,
    message: Option<&str>,
    close_issue: bool,
) -> Result<Merged, String> {
    merge_in(&root(), path, changes, message, close_issue).await
}

/// What a worktree deleted after its merge came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Removal {
    pub removed: bool,
    /// Why the worktree stayed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Why its branch stayed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_kept: Option<String>,
}

/// Delete the worktree at `path` and its merged branch, as a cleanup does:
/// never one with changes, and never one a session or terminal works in.
pub(crate) async fn remove(daemon: &tsumugi_term::Daemon, path: &Path) -> Removal {
    match crate::cleanup::busy_directories(daemon).await {
        Ok(busy) => remove_unused(&busy, path).await,
        Err(why) => Removal {
            removed: false,
            why: Some(why),
            branch_kept: None,
        },
    }
}

/// [`remove`], with `busy` the directories sessions and terminals work in.
async fn remove_unused(busy: &[PathBuf], path: &Path) -> Removal {
    let stays = |why: String| Removal {
        removed: false,
        why: Some(why),
        branch_kept: None,
    };
    let dir = tsumugi_sys::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let standing = match tsumugi_vcs::worktree::standing(&dir).await {
        Ok(standing) => standing,
        Err(error) => return stays(error),
    };
    let kept = crate::cleanup::reasons(
        &standing,
        busy.iter().any(|working| working.starts_with(&dir)),
    );
    // What a merge brings in is not lost by deleting it, so only what a
    // cleanup would keep for other reasons keeps it.
    let why: Vec<String> = kept
        .iter()
        .filter(|why| !matches!(why, crate::cleanup::Why::Unpushed(_)))
        .map(crate::cleanup::Why::text)
        .collect();
    if !why.is_empty() {
        return stays(why.join(", "));
    }
    match tsumugi_vcs::worktree::delete(&dir, false, tsumugi_model::BranchFate::Delete).await {
        Ok(removed) => Removal {
            removed: true,
            why: None,
            branch_kept: removed
                .kept
                .map(|kept| format!("{}: {}", kept.name, kept.why)),
        },
        Err(error) => stays(error),
    }
}

/// What a session that resolves a merge is first told.
pub(crate) fn conflict_prompt(branch: &str, base: &str, files: &[String]) -> String {
    let mut told = format!(
        "{base} has been merged into your branch {branch} in this worktree, and the merge stopped on conflicts. \
         Resolve them so that both sides' intent is kept, in these files:\n"
    );
    for file in files {
        told.push_str(&format!("- {file}\n"));
    }
    told.push_str(
        "\nWhere a file is generated, generate it again instead of merging it by hand. \
         When every conflict is resolved, stage the files, finish the merge with `git commit --no-edit`, run the checks this repository has, and say what you did. \
         Do not touch the main checkout, and do not push.",
    );
    told
}

/// Bring the main checkout's branch into the branch of the worktree at
/// `path`, and start a session there that resolves what conflicts. The main
/// checkout is not touched. Says what became of it.
pub(crate) async fn resolve(
    watched: Option<&crate::watch::Watched>,
    path: &Path,
) -> Result<String, String> {
    let standing = tsumugi_vcs::integrate::standing(path).await?;
    let (Some(branch), Some(base)) = (standing.branch.clone(), standing.base.clone()) else {
        return Err("the worktree and the main checkout both need a branch checked out".into());
    };
    let files = tsumugi_vcs::integrate::bring_in_base(path).await?;
    if files.is_empty() {
        return Ok(format!("{base} merged into {branch} without a conflict"));
    }
    let told = conflict_prompt(&branch, &base, &files);
    let settings = crate::settings::current();
    let context = crate::programs::starting(&settings, None, None)?;
    let daemon = crate::daemon::daemon();
    let listed = daemon.sessions().await.map_err(|e| e.to_string())?;
    let dir: PathBuf = path.to_path_buf();
    let request = crate::execution::StartRequest {
        agent: context.agent(),
        profile: Some(context.profile),
        name: crate::starting::name_for(Some("Resolve"), &told, &listed),
        name_given: true,
        prompt: told,
        cwd: dir.clone(),
        env: crate::daemon::agent_env(),
        started_ms: tsumugi_model::time::now_ms(),
        show: true,
        ..Default::default()
    };
    let program = crate::work_host::tsumugi_program();
    let state = root();
    let host = crate::starting::Host {
        daemon: &daemon,
        root: &state,
        program: &program,
    };
    let place = crate::starting::Place {
        dir,
        worktree: false,
        base: None,
    };
    crate::starting::start(host, &settings, None, place, request)
        .await
        .map_err(|failure| {
            format!(
                "{base} was merged into {branch} and stopped on {} conflicts, left to resolve there; no session started: {}",
                files.len(),
                failure.message
            )
        })?;
    if let Some(watched) = watched {
        watched.relist();
    }
    Ok(format!(
        "{base} was merged into {branch}, which stopped on {}; a session resolves them there and the main checkout is untouched",
        files.join(", ")
    ))
}

#[cfg(test)]
mod tests;
