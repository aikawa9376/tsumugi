//! Deleting a worktree: the host is asked how it stands, the person is told
//! what deleting it loses and chooses, and only then is it deleted.

use std::path::{Path, PathBuf};

use tsumugi_model::{BranchFate, EnvironmentId, WorktreeRemoved, WorktreeStanding};
use tsumugi_protocol::Request;

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::overlay::confirm::Ask;
use crate::state::ViewState;
use crate::tree::{self, nav::Item};

/// What a worktree is called to the person: its branch, else its directory.
fn called(branch: Option<&str>, path: &Path) -> String {
    branch.map(str::to_string).unwrap_or_else(|| {
        path.file_name().map_or_else(
            || path.display().to_string(),
            |f| f.to_string_lossy().into_owned(),
        )
    })
}

/// Start deleting the worktree of the tree's row `row`: the host is asked
/// how it stands. Nothing is deleted yet, and what is not a worktree to
/// delete, the main checkout, is said so.
pub(crate) fn delete(state: &mut ViewState, row: &Item) -> Vec<Effect> {
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
            "The main checkout is not a worktree to delete".into(),
        );
    }
    vec![Effect::WorktreeInspect {
        environment: environment.clone(),
        path: path.clone(),
    }]
}

/// The host said how the worktree at `path` stands: the question is asked,
/// with what deleting it loses.
pub(crate) fn inspected(
    state: &mut ViewState,
    environment: EnvironmentId,
    path: PathBuf,
    result: Result<WorktreeStanding, String>,
) -> Vec<Effect> {
    let standing = match result {
        Ok(standing) => standing,
        Err(why) => {
            return status_line::notice(
                &mut state.chrome.status,
                format!("cannot delete {}: {why}", called(None, &path)),
            );
        }
    };
    if standing.main {
        return status_line::notice(
            &mut state.chrome.status,
            "The main checkout is not a worktree to delete".into(),
        );
    }
    if standing.locked {
        return status_line::notice(
            &mut state.chrome.status,
            format!(
                "{} is locked: unlock it with git worktree unlock first",
                called(standing.branch.as_deref(), &path)
            ),
        );
    }
    let tree = tree::listed(state.hosts.work(&environment));
    let (sessions, terminals) = tree
        .worktrees()
        .find(|w| w.worktree.path == path)
        .map(|w| {
            (
                w.sessions.clone(),
                w.terminals.iter().map(|t| t.id).collect(),
            )
        })
        .unwrap_or_default();
    // A branch whose commits nothing else has is kept unless the person
    // says otherwise; one that is safe goes with its worktree.
    let delete_branch =
        standing.branch.is_some() && standing.unmerged == 0 && standing.unpushed == 0;
    state.overlays.confirm = Some(Ask::DeleteWorktree {
        environment,
        name: called(standing.branch.as_deref(), &path),
        path,
        standing,
        sessions,
        terminals,
        delete_branch,
    });
    Vec::new()
}

/// The branch choice of the question open flipped, when it has one.
pub(crate) fn toggle(state: &mut ViewState) -> Vec<Effect> {
    if let Some(Ask::DeleteWorktree {
        standing,
        delete_branch,
        ..
    }) = state.overlays.confirm.as_mut()
        && standing.branch.is_some()
    {
        *delete_branch = !*delete_branch;
    }
    Vec::new()
}

/// What the question answered yes asks: what runs in the worktree ends, then
/// the host deletes it, with its changes only when the question said it has
/// some.
pub(crate) fn confirmed(ask: Ask) -> Vec<Effect> {
    let Ask::DeleteWorktree {
        environment,
        path,
        standing,
        sessions,
        terminals,
        delete_branch,
        ..
    } = ask
    else {
        return Vec::new();
    };
    let branch = match (
        delete_branch && standing.branch.is_some(),
        standing.unmerged,
    ) {
        (false, _) => BranchFate::Keep,
        (true, 0) => BranchFate::Delete,
        (true, _) => BranchFate::DeleteUnmerged,
    };
    let stops = sessions.into_iter().map(|session| Effect::Host {
        environment: environment.clone(),
        request: Request::Stop {
            session: Box::new(session),
        },
    });
    let ends = terminals.into_iter().map(|id| Effect::EndShell {
        environment: environment.clone(),
        id,
    });
    let deletes = Effect::WorktreeDelete {
        environment: environment.clone(),
        path,
        force: standing.dirty(),
        branch,
    };
    stops.chain(ends).chain([deletes]).collect()
}

/// The host answered deleting the worktree at `path`: the status line says
/// what became of it, and of its branch when git left it.
pub(crate) fn removed(
    state: &mut ViewState,
    path: &Path,
    result: Result<WorktreeRemoved, String>,
) -> Vec<Effect> {
    let name = called(None, path);
    let told = match result {
        Ok(WorktreeRemoved { kept: None }) => format!("Deleted {name}"),
        Ok(WorktreeRemoved { kept: Some(kept) }) => {
            format!(
                "Deleted {name}; kept the branch {}: {}",
                kept.name, kept.why
            )
        }
        Err(why) => format!("cannot delete {name}: {why}"),
    };
    status_line::notice(&mut state.chrome.status, told)
}
