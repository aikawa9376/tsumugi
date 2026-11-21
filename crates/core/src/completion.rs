//! Completions acknowledged in this window, separate from reported state.

use std::collections::HashSet;

use tsumugi_model::{Session, State, TerminalSession, Tree};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completions {
    seen: HashSet<String>,
    seen_terminals: HashSet<u128>,
    /// Sessions the user marked unread, which stay so until shown anew.
    unread: HashSet<String>,
    /// What the last look found shown, to tell a view that begins from one
    /// that goes on.
    last: Option<Looked>,
    /// How many sessions and terminals are unread as of the last look.
    count: usize,
}

/// What the focused window shows while the user is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewed<'a> {
    Session(&'a str),
    Terminal(u128),
}

/// [`Viewed`], kept.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Looked {
    Session(String),
    Terminal(u128),
}

impl Looked {
    fn of(shown: Viewed) -> Self {
        match shown {
            Viewed::Session(id) => Self::Session(id.to_owned()),
            Viewed::Terminal(id) => Self::Terminal(id),
        }
    }
}

impl Completions {
    /// These marks with `id` marked unread: a finished session shows its
    /// check again, and any session stays unread until it is shown anew.
    #[must_use]
    pub fn marked_unread(&self, id: &str) -> Self {
        let mut next = self.clone();
        next.seen.remove(id);
        next.unread.insert(id.to_owned());
        next
    }

    /// Whether `session` is unread: finished and not yet seen, or marked
    /// unread by the user.
    pub fn unread(&self, session: &Session) -> bool {
        self.unread.contains(&session.id)
            || (session.state == State::Done && !self.seen.contains(&session.id))
    }

    /// How many sessions and terminals were unread at the last look.
    pub fn unread_count(&self) -> usize {
        self.count
    }

    pub fn updated(&self, tree: &Tree, shown: Option<Viewed>) -> Self {
        // A view that goes on does not read what the user marked unread
        // while looking at it.
        let anew = self.last != shown.map(Looked::of);
        let reading =
            |id: &str| shown == Some(Viewed::Session(id)) && (anew || !self.unread.contains(id));
        let unread: HashSet<String> = tree
            .sessions()
            .filter(|(_, s)| self.unread.contains(&s.id) && !reading(&s.id))
            .map(|(_, s)| s.id.clone())
            .collect();
        let seen: HashSet<String> = tree
            .sessions()
            .filter(|(_, s)| s.state == State::Done)
            .filter(|(_, s)| {
                (self.seen.contains(&s.id) && !unread.contains(&s.id)) || reading(&s.id)
            })
            .map(|(_, s)| s.id.clone())
            .collect();
        let seen_terminals: HashSet<u128> = tree
            .worktrees()
            .flat_map(|w| w.terminals.iter())
            .filter(|t| t.state == Some(State::Done))
            .filter(|t| {
                self.seen_terminals.contains(&t.id) || shown == Some(Viewed::Terminal(t.id))
            })
            .map(|t| t.id)
            .collect();
        let count = tree
            .sessions()
            .filter(|(_, s)| {
                unread.contains(&s.id) || (s.state == State::Done && !seen.contains(&s.id))
            })
            .count()
            + tree
                .worktrees()
                .flat_map(|w| w.terminals.iter())
                .filter(|t| t.state == Some(State::Done) && !seen_terminals.contains(&t.id))
                .count();
        Self {
            seen,
            seen_terminals,
            unread,
            last: shown.map(Looked::of),
            count,
        }
    }

    pub fn state<'a>(&self, session: &'a Session) -> &'a State {
        if session.state == State::Done && self.seen.contains(&session.id) {
            &State::Idle
        } else {
            &session.state
        }
    }

    /// How the program in `terminal` stands, as this window marks it; `None`
    /// while it has reported nothing.
    pub fn terminal_state<'a>(&self, terminal: &'a TerminalSession) -> Option<&'a State> {
        match &terminal.state {
            Some(State::Done) if self.seen_terminals.contains(&terminal.id) => Some(&State::Idle),
            state => state.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::testing;
    use tsumugi_model::{Agent, State, Wait};

    fn session(agent: Agent, state: State) -> tsumugi_model::Session {
        testing::session("review")
            .cwd("/project")
            .agent(agent)
            .state(state)
            .build()
    }

    fn tree(session: &tsumugi_model::Session) -> tsumugi_model::Tree {
        testing::tree().session(session.clone()).build()
    }

    #[test]
    fn an_unviewed_completion_keeps_its_check_for_either_agent() {
        for agent in Agent::ALL {
            let s = session(agent, State::Done);
            let marks = Completions::default().updated(&tree(&s), None);
            assert_eq!(marks.state(&s), &State::Done);
        }
    }

    #[test]
    fn viewing_a_completion_clears_its_check_across_refreshes_and_tab_switches() {
        for agent in Agent::ALL {
            let s = session(agent, State::Done);
            let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
            assert_eq!(marks.state(&s), &State::Idle);
            let marks = marks.updated(&tree(&s), Some(Viewed::Session("another-session")));
            assert_eq!(marks.state(&s), &State::Idle);
            assert_eq!(s.state, State::Done);
        }
    }

    #[test]
    fn a_new_turn_completed_out_of_view_gets_a_new_check() {
        let mut s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
        s.state = State::Working;
        let marks = marks.updated(&tree(&s), None);
        assert_eq!(marks.state(&s), &State::Working);
        s.state = State::Done;
        let marks = marks.updated(&tree(&s), None);
        assert_eq!(marks.state(&s), &State::Done);
    }

    #[test]
    fn a_completion_already_on_screen_is_seen() {
        let mut s = session(Agent::Codex, State::Working);
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
        s.state = State::Done;
        let marks = marks.updated(&tree(&s), Some(Viewed::Session(&s.id)));
        assert_eq!(marks.state(&s), &State::Idle);
    }

    #[test]
    fn viewing_does_not_clear_work_waiting_or_failure() {
        for state in [
            State::Idle,
            State::Working,
            State::Waiting(Wait::Permission),
            State::Failed,
        ] {
            let s = session(Agent::Claude, state.clone());
            let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
            assert_eq!(marks.state(&s), &state);
        }
    }

    fn terminal(state: Option<State>) -> (tsumugi_model::TerminalSession, tsumugi_model::Tree) {
        let t = tsumugi_model::TerminalSession {
            state,
            ..testing::shell(7, "/project")
        };
        let anchor = session(Agent::Claude, State::Idle);
        let tree = testing::tree().session(anchor).terminal(t.clone()).build();
        assert!(tree.terminal(7).is_some());
        (t, tree)
    }

    #[test]
    fn a_terminal_s_finished_program_keeps_its_check_until_the_terminal_is_viewed() {
        let (t, tree) = terminal(Some(State::Done));
        let marks = Completions::default().updated(&tree, None);
        assert_eq!(marks.terminal_state(&t), Some(&State::Done));
        let marks = marks.updated(&tree, Some(Viewed::Terminal(7)));
        assert_eq!(marks.terminal_state(&t), Some(&State::Idle));
        let marks = marks.updated(&tree, None);
        assert_eq!(marks.terminal_state(&t), Some(&State::Idle));
    }

    #[test]
    fn a_terminal_whose_program_reports_nothing_has_no_state() {
        let (t, tree) = terminal(None);
        let marks = Completions::default().updated(&tree, Some(Viewed::Terminal(7)));
        assert_eq!(marks.terminal_state(&t), None);
    }

    #[test]
    fn viewing_a_terminal_does_not_clear_its_work_or_failure() {
        for state in [State::Working, State::Waiting(Wait::Input), State::Failed] {
            let (t, tree) = terminal(Some(state.clone()));
            let marks = Completions::default().updated(&tree, Some(Viewed::Terminal(7)));
            assert_eq!(marks.terminal_state(&t), Some(&state));
        }
    }

    #[test]
    fn a_removed_session_does_not_leave_a_seen_completion_behind() {
        let s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
        let marks = marks.updated(&tsumugi_model::Tree::default(), None);
        assert_eq!(marks, Completions::default());
    }

    #[test]
    fn a_completion_not_yet_seen_is_unread_and_viewing_reads_it() {
        let s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), None);
        assert!(marks.unread(&s));
        let marks = marks.updated(&tree(&s), Some(Viewed::Session(&s.id)));
        assert!(!marks.unread(&s));
        // Work that did not finish is not news.
        let idle = session(Agent::Claude, State::Idle);
        assert!(
            !Completions::default()
                .updated(&tree(&idle), None)
                .unread(&idle)
        );
    }

    #[test]
    fn a_session_marked_unread_stays_so_until_it_is_viewed_anew() {
        let s = session(Agent::Claude, State::Idle);
        let on_screen = Some(Viewed::Session(&s.id));
        let marks = Completions::default().updated(&tree(&s), on_screen);
        let marks = marks.marked_unread(&s.id).updated(&tree(&s), on_screen);
        // Still on screen, not shown anew: the mark the user made stands.
        assert!(marks.unread(&s));
        assert_eq!(marks.state(&s), &State::Idle);
        let marks = marks.updated(&tree(&s), None);
        assert!(marks.unread(&s));
        // Shown again, as when the window comes to the front, reads it.
        let marks = marks.updated(&tree(&s), on_screen);
        assert!(!marks.unread(&s));
    }

    #[test]
    fn marking_a_seen_completion_unread_brings_back_its_check() {
        let s = session(Agent::Claude, State::Done);
        let marks = Completions::default().updated(&tree(&s), Some(Viewed::Session(&s.id)));
        let marks = marks.marked_unread(&s.id).updated(&tree(&s), None);
        assert!(marks.unread(&s));
        assert_eq!(marks.state(&s), &State::Done);
    }

    #[test]
    fn a_removed_session_does_not_leave_an_unread_mark_behind() {
        let s = session(Agent::Claude, State::Idle);
        let marks = Completions::default()
            .marked_unread(&s.id)
            .updated(&tree(&s), None);
        let marks = marks.updated(&tsumugi_model::Tree::default(), None);
        assert_eq!(marks, Completions::default());
    }

    #[test]
    fn the_unread_are_counted_across_sessions_and_terminals() {
        let (t, tree) = terminal(Some(State::Done));
        let marks = Completions::default().updated(&tree, None);
        // The anchor session of the tree is idle.
        assert_eq!(marks.unread_count(), 1);
        let marks = marks.updated(&tree, Some(Viewed::Terminal(7)));
        assert_eq!(marks.unread_count(), 0);
        let anchor = tree.sessions().next().unwrap().1.id.clone();
        let marks = marks.marked_unread(&anchor).updated(&tree, None);
        assert_eq!(marks.unread_count(), 1);
        assert_eq!(marks.terminal_state(&t), Some(&State::Idle));
    }
}
