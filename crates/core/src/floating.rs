//! The floating terminal: a panel of shells over the window that belongs to
//! no worktree. The work host holds the shells, as it holds every terminal,
//! so which tabs the panel has is read off its listing; what this holds is
//! whether the panel is open and has the keys, and which tab each
//! environment last showed.

use std::collections::{BTreeMap, BTreeSet};

use tsumugi_model::{EnvironmentId, Key, TerminalSession};

use crate::actions::Action;
use crate::content::{self, Tab};
use crate::cursor;
use crate::effect::Effect;
use crate::keymap::{Context, Step};
use crate::state::ViewState;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Floating {
    /// The panel is drawn over the window.
    pub open: bool,
    /// The panel has the keys: they go to its shell but for those bound.
    pub focused: bool,
    /// The tab each environment's panel showed last, by terminal.
    selected: BTreeMap<EnvironmentId, u128>,
    /// The environments a shell was asked of that has not come yet.
    starting: BTreeSet<EnvironmentId>,
}

/// The environment the panel is of: the one the window shows.
pub fn environment(state: &ViewState) -> EnvironmentId {
    content::shown_environment_of(&state.content, &state.hosts)
}

/// The floating shells `environment` lists, in the order it lists them.
pub fn shells<'a>(state: &'a ViewState, environment: &EnvironmentId) -> Vec<&'a TerminalSession> {
    crate::tree::terminals(state.hosts.work(environment))
        .iter()
        .filter(|t| t.floating)
        .collect()
}

/// The panel's tabs: the floating shells of the environment shown.
pub fn tabs(state: &ViewState) -> Vec<Tab> {
    let environment = environment(state);
    shells(state, &environment)
        .into_iter()
        .map(|t| Tab::Terminal(environment.clone(), t.id))
        .collect()
}

/// The tab the panel shows: the one chosen last in this environment while it
/// is still listed, else the first.
pub fn shown(state: &ViewState) -> Option<Tab> {
    let environment = environment(state);
    let shells = shells(state, &environment);
    let chosen = state.floating.selected.get(&environment);
    shells
        .iter()
        .find(|t| Some(&t.id) == chosen)
        .or(shells.first())
        .map(|t| Tab::Terminal(environment, t.id))
}

/// Whether `action` is the panel's own, which leaves the keys with it.
pub(crate) fn is_own(action: &Action) -> bool {
    matches!(
        action,
        Action::FloatingToggle
            | Action::FloatingNew
            | Action::FloatingCloseTab
            | Action::FloatingNext
            | Action::FloatingPrev
    )
}

/// An action ran from the keys: any but the panel's own takes the keys to
/// where it acts.
pub(crate) fn leave_for(state: &mut ViewState, action: &Action) {
    if !is_own(action) {
        state.floating.focused = false;
    }
}

pub(crate) fn act(state: &mut ViewState, action: Action) -> Vec<Effect> {
    match action {
        Action::FloatingToggle => toggle(state),
        Action::FloatingNew => {
            if !state.floating.open {
                return toggle_open_new(state);
            }
            state.floating.focused = true;
            start(state)
        }
        Action::FloatingCloseTab => close_tab(state),
        Action::FloatingNext => step_tab(state, 1),
        Action::FloatingPrev => step_tab(state, -1),
        _ => Vec::new(),
    }
}

/// Open the panel, with the keys, and ask for a shell unless it has one.
fn toggle(state: &mut ViewState) -> Vec<Effect> {
    let floating = &mut state.floating;
    match (floating.open, floating.focused) {
        (true, true) => {
            floating.open = false;
            floating.focused = false;
            Vec::new()
        }
        (true, false) | (false, _) => {
            floating.open = true;
            floating.focused = true;
            if shown(state).is_none() {
                start(state)
            } else {
                Vec::new()
            }
        }
    }
}

/// Open the panel and ask for another shell in it.
fn toggle_open_new(state: &mut ViewState) -> Vec<Effect> {
    state.floating.open = true;
    state.floating.focused = true;
    start(state)
}

/// Ask for a shell in the environment shown, unless one is being started.
fn start(state: &mut ViewState) -> Vec<Effect> {
    let environment = environment(state);
    if !state.floating.starting.insert(environment.clone()) {
        return Vec::new();
    }
    vec![Effect::StartFloating { environment }]
}

fn step_tab(state: &mut ViewState, delta: isize) -> Vec<Effect> {
    let tabs = tabs(state);
    let Some(shown) = shown(state) else {
        return Vec::new();
    };
    let at = tabs.iter().position(|t| *t == shown).unwrap_or(0);
    if let Some(Tab::Terminal(environment, id)) = tabs.get(cursor::wrap(at, delta, tabs.len())) {
        state.floating.selected.insert(environment.clone(), *id);
    }
    Vec::new()
}

fn close_tab(state: &mut ViewState) -> Vec<Effect> {
    let tabs = tabs(state);
    let Some(shown) = shown(state) else {
        return Vec::new();
    };
    let Tab::Terminal(environment, id) = shown.clone() else {
        return Vec::new();
    };
    let at = tabs.iter().position(|t| *t == shown).unwrap_or(0);
    let beside = if at > 0 { at - 1 } else { at + 1 };
    match tabs.get(beside) {
        Some(Tab::Terminal(_, next)) => {
            state.floating.selected.insert(environment.clone(), *next);
        }
        _ => {
            state.floating.selected.remove(&environment);
            state.floating.open = false;
            state.floating.focused = false;
        }
    }
    vec![Effect::EndShell { environment, id }]
}

/// A shell the host started for the panel: shown, without the keys moving.
pub(crate) fn started(state: &mut ViewState, environment: EnvironmentId, id: u128) {
    state.floating.starting.remove(&environment);
    state.floating.selected.insert(environment, id);
}

/// A shell the host could not start for the panel.
pub(crate) fn failed(state: &mut ViewState, environment: &EnvironmentId) {
    state.floating.starting.remove(environment);
}

/// A tab clicked: shown, and the panel has the keys.
pub(crate) fn select(state: &mut ViewState, environment: EnvironmentId, id: u128) {
    state.floating.selected.insert(environment, id);
    focus(state);
}

/// The panel clicked: it has the keys.
pub(crate) fn focus(state: &mut ViewState) {
    if state.floating.open {
        state.floating.focused = true;
    }
}

/// The panel closed, its shells left running.
pub(crate) fn close(state: &mut ViewState) {
    state.floating.open = false;
    state.floating.focused = false;
}

/// Something else clicked: the panel stays open but gives up the keys.
pub(crate) fn blur(state: &mut ViewState) {
    state.floating.focused = false;
}

/// A key pressed while the panel has the keys: its own bindings and the
/// window's global ones run, and every other key is the shell's.
pub(crate) fn press(state: &mut ViewState, key: Key) -> Vec<Effect> {
    state.console.escaped_from = None;
    state.console.repeat = None;
    let seq = std::mem::take(&mut state.console.pending);
    let contexts = [Context::Floating, Context::Global];
    match state.chrome.keymap.resolve(&contexts, &seq, &key) {
        Step::Wait => {
            state.console.pending = seq.into_iter().chain(Some(key)).collect();
            Vec::new()
        }
        Step::Run(action) => vec![Effect::PerformKey(action, key)],
        // Keys of a sequence begun and broken off were the shell's too.
        Step::Nothing => seq
            .into_iter()
            .chain(Some(key))
            .map(Effect::FloatingKey)
            .collect(),
    }
}
