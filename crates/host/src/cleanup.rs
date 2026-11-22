//! Clearing away the worktrees that are done with: those with nothing to lose
//! and nothing running in them. A worktree that has changes, commits no one
//! else has, or a session in it is never a candidate, so cleaning up needs no
//! question; what could be lost is left for the user to delete knowingly.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tsumugi_model::{BranchFate, CleanupEntry, CleanupPull, WorktreeStanding};

/// Why a worktree is left where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The repository's main checkout.
    Main,
    /// Git keeps it from being removed.
    Locked,
    /// No branch is checked out, so what it holds may be reached by nothing else.
    Detached,
    /// Files changed, staged or not tracked.
    Changed(usize),
    /// Commits of its branch no remote has.
    Unpushed(usize),
    /// Commits of its branch the main checkout does not hold.
    Unmerged(usize),
    /// A session or a terminal works in it.
    InUse,
}

impl Why {
    pub fn text(&self) -> String {
        match self {
            Self::Main => "the main checkout".into(),
            Self::Locked => "locked".into(),
            Self::Detached => "no branch is checked out".into(),
            Self::Changed(n) => format!("{n} changed files"),
            Self::Unpushed(n) => format!("{n} commits no remote has"),
            Self::Unmerged(n) => format!("{n} commits not in the main checkout"),
            Self::InUse => "a session or terminal works in it".into(),
        }
    }
}

/// A worktree, and whether it can go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub branch: Option<String>,
    /// What keeps it; empty for a worktree that can be removed.
    pub kept: Vec<Why>,
    /// What it takes of the disk, in bytes.
    pub bytes: u64,
}

impl Candidate {
    pub fn removable(&self) -> bool {
        self.kept.is_empty()
    }
}

/// What keeps a worktree standing as `standing`, with a session or terminal
/// in it or not.
pub fn reasons(standing: &WorktreeStanding, in_use: bool) -> Vec<Why> {
    let mut kept = Vec::new();
    if standing.main {
        kept.push(Why::Main);
    }
    if standing.locked {
        kept.push(Why::Locked);
    }
    if standing.branch.is_none() && !standing.main {
        kept.push(Why::Detached);
    }
    if standing.changed > 0 {
        kept.push(Why::Changed(standing.changed));
    }
    if standing.unpushed > 0 {
        kept.push(Why::Unpushed(standing.unpushed));
    }
    if standing.unmerged > 0 {
        kept.push(Why::Unmerged(standing.unmerged));
    }
    if in_use {
        kept.push(Why::InUse);
    }
    kept
}

/// The worktrees of the repository holding `at`, each with what keeps it.
/// `busy` are the directories sessions and terminals work in, and `skip`
/// the worktrees that are not the user's to list, such as the gate's own.
pub async fn scan(at: &Path, busy: &[PathBuf], skip: &Path) -> Result<Vec<Candidate>, String> {
    let mut found = Vec::new();
    for entry in tsumugi_vcs::worktree::all(at).await? {
        if entry.path.starts_with(skip) {
            continue;
        }
        let standing = tsumugi_vcs::worktree::standing(&entry.path).await?;
        let in_use = busy.iter().any(|dir| dir.starts_with(&entry.path));
        let path = entry.path.clone();
        let bytes = crate::environment::spawn_blocking(move || crate::governor::size_of(&path))
            .await
            .unwrap_or(0);
        found.push(Candidate {
            kept: reasons(&standing, in_use),
            path: entry.path,
            branch: entry.branch,
            bytes,
        });
    }
    Ok(found)
}

/// Commits GitHub holds as a pull request's head are pushed, whatever the
/// remotes still hold of the branch, as after the remote's branch was deleted
/// once the pull request was merged. A commit made since is not.
pub fn settle(
    standing: &mut WorktreeStanding,
    head: Option<&str>,
    pull: Option<&tsumugi_vcs::github::PullRef>,
) {
    if let (Some(head), Some(pull)) = (head, pull)
        && pull.head == head
    {
        standing.unpushed = 0;
    }
}

/// The worktrees of the repository holding `at` that a cleanup lists, the
/// main checkout left out, each with what is weighed before deleting it.
/// `pulls` are the repository's pull requests, as far as GitHub could be
/// asked, and `skip` the worktrees that are not the user's to list.
pub async fn entries(
    at: &Path,
    skip: &Path,
    pulls: &[tsumugi_vcs::github::PullRef],
) -> Result<Vec<CleanupEntry>, String> {
    let mut found = Vec::new();
    for entry in tsumugi_vcs::worktree::all(at).await? {
        if entry.path.starts_with(skip) {
            continue;
        }
        let mut standing = tsumugi_vcs::worktree::standing(&entry.path).await?;
        if standing.main {
            continue;
        }
        let facts = tsumugi_vcs::worktree::facts(&entry.path).await?;
        let pull = entry
            .branch
            .as_deref()
            .and_then(|branch| tsumugi_vcs::github::pull_of(pulls, branch));
        settle(&mut standing, facts.head.as_deref(), pull);
        let path = entry.path.clone();
        let bytes = crate::environment::spawn_blocking(move || crate::governor::size_of(&path))
            .await
            .unwrap_or(0);
        found.push(CleanupEntry {
            path: entry.path,
            standing,
            bytes,
            merged: facts.merged,
            last_commit: facts.last_commit,
            pull: pull.map(|p| CleanupPull {
                number: p.number,
                state: p.state,
            }),
        });
    }
    Ok(found)
}

/// How removing one worktree went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Removed {
    pub path: PathBuf,
    /// Why the worktree stayed, when it did.
    pub error: Option<String>,
    /// Why its branch stayed, when it was to go and did not.
    pub branch_kept: Option<String>,
}

/// Removes the candidates that can go, never with force: git itself refuses
/// a worktree that changed since it was scanned. Their branches go too only
/// when `branches` says so, and then only when merged.
pub async fn sweep(candidates: &[Candidate], branches: bool) -> Vec<Removed> {
    let fate = if branches {
        BranchFate::Delete
    } else {
        BranchFate::Keep
    };
    let mut done = Vec::new();
    for candidate in candidates.iter().filter(|c| c.removable()) {
        let result = tsumugi_vcs::worktree::delete(&candidate.path, false, fate).await;
        done.push(match result {
            Ok(removed) => Removed {
                path: candidate.path.clone(),
                error: None,
                branch_kept: removed
                    .kept
                    .map(|kept| format!("{}: {}", kept.name, kept.why)),
            },
            Err(error) => Removed {
                path: candidate.path.clone(),
                error: Some(error),
                branch_kept: None,
            },
        });
    }
    done
}

/// What a cleanup is told: what could go, what stays and why, and, when it
/// swept, what went.
pub fn told(candidates: &[Candidate], swept: Option<&[Removed]>) -> serde_json::Value {
    let can: Vec<_> = candidates.iter().filter(|c| c.removable()).collect();
    let kept: Vec<_> = candidates
        .iter()
        .filter(|c| !c.removable() && !c.kept.contains(&Why::Main))
        .map(|c| {
            serde_json::json!({
                "path": c.path,
                "branch": c.branch,
                "why": c.kept.iter().map(Why::text).collect::<Vec<_>>(),
                "bytes": c.bytes,
            })
        })
        .collect();
    serde_json::json!({
        "removable": can,
        "kept": kept,
        "removed": swept,
    })
}

/// The directories that sessions and terminals of this work host work in.
pub(crate) async fn busy_directories(
    daemon: &tsumugi_term::Daemon,
) -> Result<Vec<PathBuf>, String> {
    let listed = daemon.sessions().await.map_err(|error| error.to_string())?;
    Ok(listed
        .iter()
        .filter_map(crate::sessions::read)
        .map(|kept| match kept {
            crate::sessions::Kept::Agent(session) => session.cwd,
            crate::sessions::Kept::Terminal(terminal) => terminal.cwd,
        })
        .collect())
}

/// Lists the worktrees of the repository holding `dir` that are done with,
/// and what keeps the others, and removes the ones that are done with when
/// `remove`.
pub async fn ask(
    daemon: &tsumugi_term::Daemon,
    settings: crate::settings::Settings,
    dir: &Path,
    remove: bool,
) -> crate::delegation::Result<serde_json::Value> {
    use crate::delegation::{Code, Error};
    let busy = busy_directories(daemon)
        .await
        .map_err(|error| Error::new(Code::Storage, error))?;
    let found = scan(dir, &busy, &crate::gate::folder())
        .await
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let swept = if remove {
        Some(sweep(&found, settings.governor.cleanup_branches).await)
    } else {
        None
    };
    let mut told = told(&found, swept.as_deref());
    let sizes: Vec<_> = found.iter().map(|c| (c.path.clone(), c.bytes)).collect();
    if let Some(warning) = crate::governor::Limits::from(&settings.governor).warning(&sizes) {
        told["warning"] = warning.into();
    }
    Ok(told)
}

#[cfg(test)]
mod tests;
