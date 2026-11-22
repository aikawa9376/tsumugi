//! Putting a worktree to sleep and waking it: its sessions' programs stop
//! and its terminals end, while its files, its branch and its sessions' rows
//! and tabs stay, so that waking it brings the conversations back.

use tsumugi_model::{Rest, Session, State};

use crate::chrome::status_line;
use crate::effect::Effect;
use crate::overlay::confirm::Ask;
use crate::state::ViewState;
use crate::tree::{self, nav::Item};

/// Whether stopping the session loses a turn in progress or an answer it
/// waits on.
fn mid_turn(session: &Session) -> bool {
    !matches!(session.state, State::Done | State::Idle)
}

/// What a worktree is called to the person.
fn called(worktree: &tsumugi_model::WorktreeNode) -> String {
    worktree.worktree.branch.clone().unwrap_or_else(|| {
        worktree.worktree.path.file_name().map_or_else(
            || worktree.worktree.path.display().to_string(),
            |f| f.to_string_lossy().into_owned(),
        )
    })
}

/// Put the worktree of the tree's row `row` to sleep: asked first where that
/// stops what is in the middle of something, a session mid-turn or a
/// terminal, which ends.
pub(crate) fn sleep(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    let Item::Worktree(environment, path) = row.own() else {
        return Vec::new();
    };
    let tree = tree::listed(state.hosts.work(environment));
    let Some(node) = tree.worktrees().find(|w| w.worktree.path == *path) else {
        return Vec::new();
    };
    let name = called(node);
    let resting = |s: &&Session| s.rest.is_none();
    let (wakeable, fixed): (Vec<&Session>, Vec<&Session>) = node
        .sessions
        .iter()
        .filter(resting)
        .partition(|s| s.wakeable());
    let terminals: Vec<u128> = node.terminals.iter().map(|t| t.id).collect();
    let left: Vec<String> = fixed.iter().map(|s| s.name.clone()).collect();
    if wakeable.is_empty() && terminals.is_empty() {
        let told = if left.is_empty() {
            format!("{name} has nothing to put to sleep")
        } else {
            format!(
                "{name} has nothing to put to sleep: {} cannot be woken again",
                left.join(", ")
            )
        };
        return status_line::notice(&mut state.chrome.status, told);
    }
    let environment = environment.clone();
    let ids: Vec<String> = wakeable.iter().map(|s| s.id.clone()).collect();
    let working = wakeable.iter().filter(|s| mid_turn(s)).count();
    if !left.is_empty() {
        status_line::notice(
            &mut state.chrome.status,
            format!(
                "{} cannot be woken again, so it keeps running",
                left.join(", ")
            ),
        );
    }
    if working == 0 && terminals.is_empty() {
        return vec![Effect::Sleep { environment, ids }];
    }
    state.overlays.confirm = Some(Ask::SleepWorktree {
        environment,
        name,
        ids,
        working,
        terminals,
    });
    Vec::new()
}

/// What the question answered yes asks: the terminals end, then the sessions
/// are put to sleep.
pub(crate) fn confirmed(ask: Ask) -> Vec<Effect> {
    let Ask::SleepWorktree {
        environment,
        ids,
        terminals,
        ..
    } = ask
    else {
        return Vec::new();
    };
    let mut effects: Vec<Effect> = terminals
        .into_iter()
        .map(|id| Effect::EndShell {
            environment: environment.clone(),
            id,
        })
        .collect();
    if !ids.is_empty() {
        effects.push(Effect::Sleep { environment, ids });
    }
    effects
}

/// Wake the worktree of the tree's row `row`: its sleeping sessions are
/// brought back with their conversations.
pub(crate) fn wake(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    let Item::Worktree(environment, path) = row.own() else {
        return Vec::new();
    };
    let tree = tree::listed(state.hosts.work(environment));
    let Some(node) = tree.worktrees().find(|w| w.worktree.path == *path) else {
        return Vec::new();
    };
    let ids: Vec<String> = node
        .sessions
        .iter()
        .filter(|s| s.rest == Some(Rest::Hibernated))
        .map(|s| s.id.clone())
        .collect();
    if ids.is_empty() {
        let told = format!("{} is not asleep", called(node));
        return status_line::notice(&mut state.chrome.status, told);
    }
    vec![Effect::BringBack {
        environment: environment.clone(),
        ids,
    }]
}

/// The work host said how many sessions slept, and which could not be woken
/// again or failed to stop and so run on, or why it could not.
pub(crate) fn slept(
    state: &mut ViewState,
    result: Result<(usize, Vec<String>), String>,
) -> Vec<Effect> {
    let told = match result {
        Ok((count, left)) => {
            let sessions = if count == 1 {
                "1 session".to_string()
            } else {
                format!("{count} sessions")
            };
            if left.is_empty() {
                format!("Put {sessions} to sleep")
            } else {
                format!("Put {sessions} to sleep; {} keeps running", left.join(", "))
            }
        }
        Err(why) => format!("cannot put to sleep: {why}"),
    };
    status_line::notice(&mut state.chrome.status, told)
}

#[cfg(test)]
mod tests;
