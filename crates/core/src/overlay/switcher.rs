//! The tab switcher: the content's tabs listed over the window while the
//! modifier keys that opened it are held, the one moved to shown when they
//! are let go. Nothing is shown, hidden or restarted while it is open.

use crate::cursor;
use crate::effect::Effect;
use crate::settings::TabSwitchOrder;
use crate::state::ViewState;

/// The tabs listed, by their keys, and which of them is moved to.
#[derive(Debug, Clone, PartialEq)]
pub struct Switcher {
    pub tabs: Vec<u64>,
    pub at: usize,
}

/// Open the switcher and move `delta` places from the tab shown, or move on
/// when it is open; nothing opens for fewer than two tabs.
pub(crate) fn step(state: &mut ViewState, delta: isize) -> Vec<Effect> {
    if let Some(open) = state.overlays.switcher.as_mut() {
        open.at = cursor::wrap(open.at, delta, open.tabs.len());
        return Vec::new();
    }
    let windows = &state.content.windows;
    if windows.tabs().len() < 2 {
        return Vec::new();
    }
    let tabs: Vec<u64> = match state.config.settings.tab_switch_order {
        TabSwitchOrder::Mru => windows.recent_tabs(),
        TabSwitchOrder::Sequential => windows.tabs().iter().collect::<Vec<_>>(),
    }
    .into_iter()
    .map(|t| t.key())
    .collect();
    let shown = windows.current().map(|t| t.key());
    let from = tabs.iter().position(|k| Some(*k) == shown).unwrap_or(0);
    let at = cursor::wrap(from, delta, tabs.len());
    state.overlays.switcher = Some(Switcher { tabs, at });
    Vec::new()
}

/// Show the tab moved to, and shut the switcher.
pub(crate) fn commit(state: &mut ViewState) -> Vec<Effect> {
    let Some(open) = state.overlays.switcher.take() else {
        return Vec::new();
    };
    if let Some(key) = open.tabs.get(open.at) {
        state.content.windows.go_to_tab_keyed(*key);
    }
    let view = &mut state.content;
    view.focus = crate::content::settled(view.focus, crate::content::shows_prompt(view.active()));
    Vec::new()
}

/// Shut the switcher and leave the tab shown as it is.
pub(crate) fn cancel(state: &mut ViewState) -> Vec<Effect> {
    state.overlays.switcher = None;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::content::Tab;
    use crate::msg::Msg;
    use crate::overlay::Holder;
    use crate::scenario::Scenario;
    use crate::testing::{here, show};
    use tsumugi_model::Key;

    fn three() -> Scenario {
        Scenario::new()
            .send(show(Tab::browser(here(), "/a")))
            .send(show(Tab::Terminal(here(), 2)))
            .send(show(Tab::Terminal(here(), 3)))
    }

    fn shown(run: &Scenario) -> Option<&Tab> {
        run.state().content.windows.focused()
    }

    fn at(run: &Scenario) -> Option<usize> {
        run.state().overlays.switcher.as_ref().map(|s| s.at)
    }

    fn recent() -> Msg {
        Msg::Window(Action::TabRecent)
    }

    fn held(key: Key) -> Msg {
        Msg::HeldKey(key)
    }

    #[test]
    fn the_first_press_moves_to_the_tab_shown_before_without_showing_it() {
        let run = three().send(recent());
        assert_eq!(at(&run), Some(1));
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 3)));
        assert_eq!(crate::overlay::holder(run.state()), Some(Holder::Switcher));
    }

    #[test]
    fn pressing_again_goes_on_down_the_list_and_round() {
        let run = three().send(recent()).send(held(Key::new("Tab").ctrl()));
        assert_eq!(at(&run), Some(2));
        let run = run.send(held(Key::new("Tab").ctrl()));
        assert_eq!(at(&run), Some(0));
    }

    #[test]
    fn the_previous_key_opens_it_on_the_tab_shown_longest_ago_and_steps_back() {
        let run = three().send(Msg::Window(Action::TabRecentPrev));
        assert_eq!(at(&run), Some(2));
        let mut back = Key::new("Tab").ctrl();
        back.shift = true;
        assert_eq!(at(&run.send(held(back))), Some(1));
    }

    #[test]
    fn letting_go_of_the_modifiers_shows_the_tab_moved_to_and_it_leads_the_recent_tabs() {
        let run = three().send(recent()).send(Msg::SwitcherRelease);
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 2)));
        assert_eq!(run.state().overlays.switcher, None);
        let run = run.send(recent()).send(Msg::SwitcherRelease);
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 3)));
    }

    #[test]
    fn enter_chooses_and_escape_gives_up() {
        let run = three().send(recent()).send(held(Key::new("Enter")));
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 2)));
        assert_eq!(run.state().overlays.switcher, None);
        let run = run.send(recent()).send(held(Key::new("Escape")));
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 2)));
        assert_eq!(run.state().overlays.switcher, None);
    }

    #[test]
    fn a_key_the_switcher_has_no_use_for_does_nothing_behind_it() {
        let run = three().send(recent()).send(held(Key::new("x")));
        assert_eq!(at(&run), Some(1));
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 3)));
    }

    #[test]
    fn one_tab_has_nothing_to_switch_to() {
        let run = Scenario::new()
            .send(show(Tab::Terminal(here(), 2)))
            .send(recent());
        assert_eq!(run.state().overlays.switcher, None);
        let run = run.send(Msg::SwitcherRelease);
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 2)));
    }

    #[test]
    fn a_release_with_no_switcher_open_does_nothing() {
        let run = three().send(Msg::SwitcherRelease);
        assert_eq!(shown(&run), Some(&Tab::Terminal(here(), 3)));
    }

    #[test]
    fn the_sequential_order_lists_the_tabs_as_they_stand() {
        let run = three()
            .with_state(|s| s.config.settings.tab_switch_order = TabSwitchOrder::Sequential)
            .send(recent());
        // The tab shown is the last of three: the next goes round to the first.
        assert_eq!(at(&run), Some(0));
        let run = run.send(Msg::SwitcherRelease);
        assert!(matches!(shown(&run), Some(Tab::Browser { .. })));
    }

    #[test]
    fn switching_leaves_every_window_and_tab_as_it_was() {
        let before = three();
        let tabs = before.state().content.windows.tabs().len();
        let run = before.send(recent()).send(Msg::SwitcherRelease);
        assert_eq!(run.state().content.windows.tabs().len(), tabs);
        assert!(run.last_effects().iter().all(|e| !matches!(
            e,
            Effect::Host { .. } | Effect::StartTerminal(_)
        )));
    }

    #[test]
    fn a_tab_that_closes_while_the_switcher_is_open_is_not_gone_to() {
        let mut run = three().send(recent());
        let gone = run.state().overlays.switcher.as_ref().unwrap().tabs[1];
        run = run.with_state(|s| {
            s.content.windows.go_to_tab_keyed(gone);
            s.content.windows.close_tab();
        });
        let before = shown(&run).cloned();
        let run = run.send(Msg::SwitcherRelease);
        assert_eq!(shown(&run).cloned(), before);
    }
}
