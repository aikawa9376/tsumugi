//! What the details panel does with the diffs it opens in the user's Neovim.
//! Asking an agent to resolve the files in conflict is [`crate::ai`]'s.

use crate::effect::Effect;
use crate::state::ViewState;

/// Open the diff the cursor is on in the user's Neovim, where it has its
/// Diffview or else a built-in diff.
pub(crate) fn open_in_neovim(state: &mut ViewState) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = crate::content::shown_environment(state);
    let work = state.hosts.work(&environment);
    let Some(spot) = super::selected_spot(&state.details, work) else {
        return vec![Effect::Notice(
            "Select a changed file to open its diff".into(),
        )];
    };
    vec![Effect::StartDiffEditor {
        place: crate::tree::Place::at(&environment, &worktree),
        spot,
    }]
}
