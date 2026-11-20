//! Escape keeps its ordinary meaning before a second one leaves the area.

use tsumugi_model::Key;

use super::Probe;
use crate::content::Area;
use crate::effect::Effect;
use crate::keymap::Step;
use crate::state::ViewState;

pub(super) fn plain(key: &Key) -> bool {
    key.key == "Escape" && !key.ctrl && !key.alt && !key.shift && !key.meta
}

pub(super) fn press(state: &mut ViewState, key: &Key, probe: &Probe) -> Option<Vec<Effect>> {
    let previous = state.console.escaped_from.take();
    if state.content.focus == Area::Sidebar {
        return None;
    }
    let here = (
        crate::content::spot_of(state.content.focus, &state.content.windows),
        state.content.active().cloned(),
    );
    if previous.as_ref() == Some(&here) {
        let contexts = super::route::help_contexts(state.content.focus, state.content.active());
        match state
            .chrome
            .keymap
            .resolve(&contexts, &[Key::new("Escape")], key)
        {
            Step::Run(action) => {
                return Some(crate::perform::perform(
                    state,
                    action,
                    Some(key.clone()),
                    probe,
                ));
            }
            Step::Wait => {
                state.console.pending = vec![Key::new("Escape"), key.clone()];
                return Some(Vec::new());
            }
            Step::Nothing => {}
        }
    }
    if plain(key) {
        state.console.escaped_from = Some(here);
    }
    None
}
