//! The issue sheet keeps the place it was opened on while focus changes.
use crate::content::{self, Area};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, Place, nav};

pub(crate) fn open(state: &mut ViewState) -> Vec<Effect> {
    show(state, false)
}

/// The sheet on its hand-over to a leader.
pub(crate) fn open_for_leader(state: &mut ViewState) -> Vec<Effect> {
    show(state, true)
}

fn show(state: &mut ViewState, leader: bool) -> Vec<Effect> {
    let place = chosen_place(state);
    state.overlays.menu = None;
    state.overlays.local_issues = Some(crate::overlay::LocalIssues {
        place,
        selected: None,
        leader,
    });
    Vec::new()
}

/// Where the keys are asking for something to be done: the worktree
/// selected in the tree while it has the keys, or else the one the focused
/// window shows, or else the environment's home.
pub(crate) fn chosen_place(state: &ViewState) -> Place {
    let environment = content::focused_environment(state);
    let work = state.hosts.work(&environment);
    let selected_row = crate::sidebar::selected(state);
    let selected = selected_row
        .as_ref()
        .filter(|_| state.content.focus == Area::Sidebar)
        .and_then(|item| nav::place_of(tree::listed(work), item));
    selected
        .or_else(|| {
            state
                .content
                .place
                .clone()
                .filter(|p| p.environment == environment)
        })
        .unwrap_or_else(|| {
            let home = work
                .and_then(|w| w.config.configured.as_ref())
                .map(|c| c.home.clone())
                .unwrap_or_default();
            Place::at(&environment, home)
        })
}
