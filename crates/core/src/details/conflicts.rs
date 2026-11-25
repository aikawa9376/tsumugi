//! What the details panel does with the diffs it opens in the user's Neovim
//! or in a diff tab. Asking an agent to resolve the files in conflict is
//! [`crate::ai`]'s.

use tsumugi_model::repository::{Rev, Spot};
use tsumugi_protocol::WorkSnapshot;

use super::Details;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::update::step;

/// The diff the cursor is on: the file or the commit selected, else the row
/// opened, else what is not committed.
fn diff_at_cursor(details: &Details, work: Option<&WorkSnapshot>) -> Spot {
    if let Some(spot) = super::selected_spot(details, work) {
        return spot;
    }
    if let Some(spot) = details.drill.spot() {
        return spot;
    }
    match super::selected_commit(details, work) {
        Some(commit) => Spot::Files(Rev::Commit(commit.id)),
        None => Spot::Files(Rev::WorkingCopy),
    }
}

/// Open the diff the cursor is on in the user's Neovim, where it has its
/// Diffview or else a built-in diff, or in a diff tab.
pub(crate) fn open_diff(state: &mut ViewState, neovim: bool) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = crate::content::shown_environment(state);
    let spot = diff_at_cursor(&state.details, state.hosts.work(&environment));
    step(
        state,
        Msg::OpenDiff {
            environment,
            worktree,
            spot,
            neovim,
            line: None,
        },
    )
}
