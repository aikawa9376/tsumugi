//! Setting the status a worktree shows of how its work stands, from the
//! tree's menu.

use tsumugi_model::{Set, WorktreeStatus};

use crate::actions::Action;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::nav::Item;

/// The change to a status that `action` makes; none for any other action.
pub(crate) fn status_of(action: &Action) -> Option<Set<WorktreeStatus>> {
    Some(match action {
        Action::WorktreeStatusTodo => Set::To(WorktreeStatus::Todo),
        Action::WorktreeStatusInProgress => Set::To(WorktreeStatus::InProgress),
        Action::WorktreeStatusInReview => Set::To(WorktreeStatus::InReview),
        Action::WorktreeStatusCompleted => Set::To(WorktreeStatus::Completed),
        Action::WorktreeStatusClear => Set::Clear,
        _ => return None,
    })
}

/// Ask the work host to change the status of the worktree or workspace at
/// the tree's row `row`; any other row has none.
pub(crate) fn set(_state: &mut ViewState, row: &Item, status: Set<WorktreeStatus>) -> Vec<Effect> {
    let (Item::Worktree(environment, path) | Item::Workspace(environment, path)) = row.own() else {
        return Vec::new();
    };
    vec![Effect::SetCheckpoint {
        environment: environment.clone(),
        worktree: path.clone(),
        status,
    }]
}
