//! A session the user marked unread, and what the actions that mark it or
//! silence its place act on.

use std::path::PathBuf;

use tsumugi_model::EnvironmentId;

use crate::content::Tab;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::nav::Item;

/// The session an action acts on: the one of the selected row while the
/// keys are in a list, or else the one a tab shows.
pub(crate) fn session_of(
    cur: Option<&Item>,
    on_list: bool,
    shown: Option<&Tab>,
) -> Option<(EnvironmentId, String)> {
    match cur.map(Item::own) {
        Some(Item::Session(environment, id) | Item::Waiting(environment, id)) if on_list => {
            Some((environment.clone(), id.clone()))
        }
        _ => match shown {
            Some(Tab::Session(environment, id)) => Some((environment.clone(), id.clone())),
            _ => None,
        },
    }
}

/// The directory whose notifications an action silences: a workspace's root
/// or a worktree's path of the selected row, or where the session of the
/// row or the tab runs.
pub(crate) fn place_of(
    state: &ViewState,
    cur: Option<&Item>,
    on_list: bool,
    shown: Option<&Tab>,
) -> Option<(EnvironmentId, PathBuf)> {
    match cur.map(Item::own) {
        Some(Item::Workspace(environment, path) | Item::Worktree(environment, path)) if on_list => {
            return Some((environment.clone(), path.clone()));
        }
        _ => {}
    }
    let (environment, id) = session_of(cur, on_list, shown)?;
    let work = state.hosts.work(&environment)?;
    let (_, worktree, _) = work.tree.tree().find(&id)?;
    Some((environment, worktree.worktree.path.clone()))
}

/// Mark `id` unread: a finished session shows its check again, and any
/// session stays unread until it is shown anew.
pub(crate) fn mark(state: &mut ViewState, id: &str) -> Vec<Effect> {
    state.tree.completions = state.tree.completions.marked_unread(id);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use tsumugi_model::State;

    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{here, listing, session, session_tab, show_in};

    fn mark(id: &str) -> Msg {
        Msg::SessionMarkUnread {
            environment: here(),
            id: id.into(),
        }
    }

    fn unread(run: &Scenario, id: &str) -> bool {
        let state = run.state();
        let tree = state.hosts.own().work.as_ref().unwrap().tree.tree();
        let (_, _, s) = tree.find(id).unwrap();
        state.tree.completions.unread(s)
    }

    #[test]
    fn a_session_marked_unread_stays_so_until_it_is_shown_anew() {
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r"), session("b").cwd("/r")]))
            .send(show_in(session_tab("a"), "/r"));
        assert!(!unread(&run, "a"));
        let run = run.send(mark("a"));
        // Still the tab shown: what the user marked stands.
        assert!(unread(&run, "a"));
        let run = run.send(show_in(session_tab("b"), "/r"));
        assert!(unread(&run, "a"));
        let run = run.send(show_in(session_tab("a"), "/r"));
        assert!(!unread(&run, "a"));
    }

    #[test]
    fn a_session_finished_out_of_view_is_unread_until_shown_with_the_window_in_front() {
        let done = || session("a").cwd("/r").state(State::Done);
        let away = Scenario::new()
            .send(Msg::WindowFocused(false))
            .event(listing([done()]))
            .send(show_in(session_tab("a"), "/r"));
        assert!(unread(&away, "a"));
        assert_eq!(away.state().tree.completions.unread_count(), 1);
        let back = away.send(Msg::WindowFocused(true));
        assert!(!unread(&back, "a"));
        assert_eq!(back.state().tree.completions.unread_count(), 0);
        // Finished in a tab that is not shown, it stays unread.
        let hidden = Scenario::new().event(listing([done(), session("b").cwd("/r")]));
        let hidden = hidden.send(show_in(session_tab("b"), "/r"));
        assert!(unread(&hidden, "a"));
    }

    #[test]
    fn marking_a_finished_session_seen_before_unread_brings_back_its_check() {
        let run = Scenario::new()
            .event(listing([session("a").cwd("/r").state(State::Done)]))
            .send(show_in(session_tab("a"), "/r"))
            .send(mark("a"));
        assert!(unread(&run, "a"));
    }
}
