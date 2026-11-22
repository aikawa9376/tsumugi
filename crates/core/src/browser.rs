//! What a browser page is asked beyond its address field: to search its
//! text, and to come back after it was closed.

use crate::content::{Area, Tab};
use crate::effect::Effect;
use crate::state::ViewState;

#[cfg(test)]
mod tests;

/// Whether a browser page is what the keys are in.
pub(crate) fn has_the_keys(state: &ViewState) -> bool {
    state.content.focus == Area::Console
        && matches!(state.content.active(), Some(Tab::Browser { .. }))
}

/// Ask the browser page shown to open its search bar.
pub(crate) fn find(state: &mut ViewState) -> Vec<Effect> {
    match (
        state.content.active().cloned(),
        state.content.windows.focused_window(),
    ) {
        (Some(Tab::Browser { .. }), Some(window)) => vec![Effect::FindInPage(window)],
        _ => vec![Effect::Notice("Show a browser page to search it".into())],
    }
}

/// Ask for the page closed last in the worktree the keys are in, as a new
/// page is opened in it.
pub(crate) fn reopen(state: &mut ViewState) -> Vec<Effect> {
    let environment = crate::content::shown_environment(state);
    let worktree = crate::content::browser_worktree(state, &environment, false);
    vec![Effect::ReopenPage {
        environment,
        worktree,
    }]
}
