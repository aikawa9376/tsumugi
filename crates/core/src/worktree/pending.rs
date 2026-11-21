//! The worktrees being made: each has a row in the tree and goes through
//! the steps of [`PIPELINE`] while the dialog that asked for it is gone and
//! the window stays free.

use std::path::PathBuf;

use tsumugi_model::{EnvironmentId, NewWorktree};

use super::Composer;
use crate::chrome::status_line;
use crate::content;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::nav::Item;

/// A step of making a worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The work host makes the worktree and its branch.
    Create,
    /// A new session's draft opens in it.
    Start,
}

impl Step {
    /// What the row says while the step runs.
    pub fn doing(self) -> &'static str {
        match self {
            Step::Create => "Creating the worktree",
            Step::Start => "Starting a session",
        }
    }
}

/// The steps every new worktree goes through, in order. A step is added by
/// naming it here and saying in [`run`] what it asks for and in [`made`]
/// what comes of its answer; the rows draw whatever is listed.
pub const PIPELINE: &[Step] = &[Step::Create, Step::Start];

/// How far a step of a worktree being made has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Done,
    Running,
    Waiting,
    Failed,
}

/// A worktree being made, which has a row in the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: u64,
    pub environment: EnvironmentId,
    /// The workspace whose row it is listed under.
    pub root: PathBuf,
    pub repository: PathBuf,
    pub new: NewWorktree,
    /// The step of [`PIPELINE`] that runs, or failed.
    pub at: usize,
    /// Where the worktree is, once the host made it.
    pub made: Option<PathBuf>,
    /// Why a step failed, which stays on the row until it is dismissed.
    pub failed: Option<String>,
}

impl Pending {
    /// Each step with how far it has come.
    pub fn steps(&self) -> Vec<(Step, Progress)> {
        PIPELINE
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let progress = match i.cmp(&self.at) {
                    std::cmp::Ordering::Less => Progress::Done,
                    std::cmp::Ordering::Equal if self.failed.is_some() => Progress::Failed,
                    std::cmp::Ordering::Equal => Progress::Running,
                    std::cmp::Ordering::Greater => Progress::Waiting,
                };
                (*step, progress)
            })
            .collect()
    }

    /// The row it has in the tree.
    pub fn row(&self) -> Item {
        Item::Pending(self.environment.clone(), self.root.clone(), self.id)
    }
}

/// `new`, which the dialog `composer` asked for, starts being made: its row
/// is listed under its workspace, unfolded to show it, and its first step
/// runs.
pub(super) fn begin(state: &mut ViewState, composer: Composer, new: NewWorktree) -> Vec<Effect> {
    let Composer {
        environment,
        repository,
        ..
    } = composer;
    let tree = crate::tree::listed(state.hosts.work(&environment));
    let root = tree
        .workspace_of(&repository)
        .map_or_else(|| repository.clone(), |ws| ws.root.clone());
    let id = state.tree.making_next;
    state.tree.making_next += 1;
    let pending = Pending {
        id,
        environment: environment.clone(),
        root: root.clone(),
        repository,
        new,
        at: 0,
        made: None,
        failed: None,
    };
    state
        .tree
        .folds
        .set(&Item::Workspace(environment, root), true);
    state.tree.making.push(pending);
    run(state, id)
}

/// What the step of `id` that runs asks for.
fn run(state: &mut ViewState, id: u64) -> Vec<Effect> {
    let Some(pending) = state.tree.making.iter().find(|p| p.id == id) else {
        return Vec::new();
    };
    match PIPELINE[pending.at] {
        Step::Create => vec![Effect::WorktreeCreate {
            environment: pending.environment.clone(),
            repository: pending.repository.clone(),
            id,
            new: pending.new.clone(),
        }],
        Step::Start => {
            let Some(pending) = remove(state, id) else {
                return Vec::new();
            };
            let Some(path) = pending.made else {
                return Vec::new();
            };
            content::draft(&mut state.content, pending.environment, path, true)
        }
    }
}

fn remove(state: &mut ViewState, id: u64) -> Option<Pending> {
    let at = state.tree.making.iter().position(|p| p.id == id)?;
    Some(state.tree.making.remove(at))
}

/// The host answered the step of `id` that ran: the next step runs, or the
/// row keeps why it failed. A worktree the host made before a later step
/// failed stays made, and the row says where.
pub(crate) fn made(state: &mut ViewState, id: u64, result: Result<PathBuf, String>) -> Vec<Effect> {
    let Some(pending) = state.tree.making.iter_mut().find(|p| p.id == id) else {
        return Vec::new();
    };
    match result {
        Ok(path) => {
            pending.made = Some(path);
            pending.at += 1;
            run(state, id)
        }
        Err(why) => {
            let told = format!("cannot make {}: {why}", pending.new.name);
            pending.failed = Some(why);
            status_line::notice(&mut state.chrome.status, told)
        }
    }
}

/// The row of a worktree that failed is put away.
pub(crate) fn dismiss(state: &mut ViewState, id: u64) -> Vec<Effect> {
    if state
        .tree
        .making
        .iter()
        .any(|p| p.id == id && p.failed.is_some())
    {
        remove(state, id);
    }
    Vec::new()
}
