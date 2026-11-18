//! The issue sheet keeps the place it was opened on while focus changes.
use crate::content::{self, Area};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, Place, nav};

pub(crate) fn open(state: &mut ViewState) -> Vec<Effect> {
    let environment = content::focused_environment(state);
    let work = state.hosts.work(&environment);
    let selected_row = crate::sidebar::selected(state);
    let selected = selected_row
        .as_ref()
        .filter(|_| state.content.focus == Area::Sidebar)
        .and_then(|item| nav::place_of(tree::listed(work), item));
    let place = selected
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
        });
    state.overlays.menu = None;
    state.overlays.local_issues = Some(place);
    Vec::new()
}
