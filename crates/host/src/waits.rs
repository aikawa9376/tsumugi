//! The waits of the sessions that wait on the user, each numbered as it
//! began, so that an answer can name the wait it answers and one sent after
//! another answered it is refused.

use std::collections::BTreeMap;
use std::sync::Mutex;

use tsumugi_model::{Session, State};

/// The waits as the last listing found them.
#[derive(Debug)]
pub struct Waits {
    next: u64,
    /// By the session's id: the state it waits in and its wait's number.
    current: BTreeMap<String, (State, u64)>,
}

impl Default for Waits {
    fn default() -> Self {
        Self {
            next: 1,
            current: BTreeMap::new(),
        }
    }
}

/// Whether a session in `session`'s state waits on the user: it asks
/// something, or its turn ended. One that rests runs nothing to answer.
fn waits(session: &Session) -> bool {
    session.rest.is_none() && matches!(session.state, State::Waiting(_) | State::Done)
}

impl Waits {
    /// Look at `sessions` as listed now, numbering each that began to wait
    /// since the last look, or stands in another state it waits in: the
    /// waits there are, by the session's id.
    pub fn look<'a>(
        &mut self,
        sessions: impl IntoIterator<Item = &'a Session>,
    ) -> BTreeMap<String, u64> {
        let mut after = BTreeMap::new();
        for session in sessions.into_iter().filter(|s| waits(s)) {
            let number = match self.current.get(&session.id) {
                Some((state, number)) if *state == session.state => *number,
                _ => {
                    self.next += 1;
                    self.next - 1
                }
            };
            after.insert(session.id.clone(), (session.state.clone(), number));
        }
        self.current = after;
        self.numbers()
    }

    pub fn numbers(&self) -> BTreeMap<String, u64> {
        self.current
            .iter()
            .map(|(id, (_, number))| (id.clone(), *number))
            .collect()
    }

    /// Whether a prompt typed for the wait `number` of `session` answers
    /// it: the session still stands in that wait, its turn ended.
    pub fn answers(&self, session: &str, number: u64) -> bool {
        self.current
            .get(session)
            .is_some_and(|(state, n)| *n == number && *state == State::Done)
    }

    /// Take the wait `number` of `session` to be answered, so no other
    /// prompt answers it: whether it was there to take.
    pub fn take(&mut self, session: &str, number: u64) -> bool {
        let answers = self.answers(session, number);
        if answers {
            self.current.remove(session);
        }
        answers
    }

    /// The wait of `session` was answered, by a prompt that named none.
    pub fn answered(&mut self, session: &str) {
        self.current.remove(session);
    }
}

static WAITS: Mutex<Option<Waits>> = Mutex::new(None);

/// Look at `sessions` as listed now; the waits there are.
pub fn look<'a>(sessions: impl IntoIterator<Item = &'a Session>) -> BTreeMap<String, u64> {
    WAITS
        .lock()
        .unwrap()
        .get_or_insert_with(Waits::default)
        .look(sessions)
}

/// Take the wait `number` of `session` to be answered now: whether it
/// stood to be.
pub fn take(session: &str, number: u64) -> bool {
    WAITS
        .lock()
        .unwrap()
        .as_mut()
        .is_some_and(|waits| waits.take(session, number))
}

/// The wait of `session`, if any, was answered.
pub fn answered(session: &str) {
    if let Some(waits) = WAITS.lock().unwrap().as_mut() {
        waits.answered(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::testing::session;
    use tsumugi_model::{Rest, Wait};

    fn listed(states: &[(&str, State)]) -> Vec<Session> {
        states
            .iter()
            .map(|(id, state)| session(*id).state(state.clone()).build())
            .collect()
    }

    #[test]
    fn sessions_waiting_when_first_looked_at_are_numbered_too() {
        let mut waits = Waits::default();

        let numbers = waits.look(&listed(&[("a", State::Done), ("b", State::Working)]));

        assert_eq!(numbers.keys().collect::<Vec<_>>(), ["a"]);
    }

    #[test]
    fn a_session_keeps_its_wait_while_it_stands_in_it() {
        let mut waits = Waits::default();
        let first = waits.look(&listed(&[("a", State::Done)]))["a"];

        let again = waits.look(&listed(&[("a", State::Done)]))["a"];

        assert_eq!(first, again);
    }

    #[test]
    fn a_session_that_waits_again_after_working_has_a_new_wait() {
        let mut waits = Waits::default();
        let first = waits.look(&listed(&[("a", State::Done)]))["a"];
        assert!(waits.look(&listed(&[("a", State::Working)])).is_empty());

        let second = waits.look(&listed(&[("a", State::Done)]))["a"];

        assert_ne!(first, second);
    }

    #[test]
    fn a_session_that_comes_to_wait_on_something_else_has_a_new_wait() {
        let mut waits = Waits::default();
        let asked = waits.look(&listed(&[("a", State::Waiting(Wait::Input))]))["a"];

        let done = waits.look(&listed(&[("a", State::Done)]))["a"];

        assert_ne!(asked, done);
    }

    #[test]
    fn no_two_sessions_share_a_wait() {
        let mut waits = Waits::default();

        let numbers = waits.look(&listed(&[("a", State::Done), ("b", State::Done)]));

        assert_ne!(numbers["a"], numbers["b"]);
    }

    #[test]
    fn a_session_that_rests_does_not_wait() {
        let mut waits = Waits::default();
        let resting = session("a")
            .state(State::Done)
            .rest(Rest::Hibernated)
            .build();

        assert!(waits.look([&resting]).is_empty());
    }

    #[test]
    fn only_a_prompt_for_the_current_wait_of_a_turn_that_ended_answers_it() {
        let mut waits = Waits::default();
        let numbers = waits.look(&listed(&[
            ("done", State::Done),
            ("asking", State::Waiting(Wait::Permission)),
        ]));

        assert!(waits.answers("done", numbers["done"]));
        assert!(!waits.answers("done", numbers["done"] + 100));
        assert!(!waits.answers("asking", numbers["asking"]));
        assert!(!waits.answers("gone", numbers["done"]));
    }

    #[test]
    fn a_wait_is_answered_once_however_soon_a_second_prompt_follows() {
        let mut waits = Waits::default();
        let number = waits.look(&listed(&[("a", State::Done)]))["a"];

        assert!(waits.take("a", number));
        assert!(!waits.take("a", number));
    }

    #[test]
    fn a_wait_answered_by_a_prompt_that_named_none_answers_no_other() {
        let mut waits = Waits::default();
        let number = waits.look(&listed(&[("a", State::Done)]))["a"];

        waits.answered("a");

        assert!(!waits.take("a", number));
    }

    #[test]
    fn a_session_still_waiting_after_its_wait_was_answered_waits_anew() {
        let mut waits = Waits::default();
        let number = waits.look(&listed(&[("a", State::Done)]))["a"];
        waits.take("a", number);

        let again = waits.look(&listed(&[("a", State::Done)]))["a"];

        assert_ne!(again, number);
    }

    #[test]
    fn a_prompt_for_a_wait_answered_meanwhile_does_not_answer() {
        let mut waits = Waits::default();
        let number = waits.look(&listed(&[("a", State::Done)]))["a"];
        waits.look(&listed(&[("a", State::Working)]));

        assert!(!waits.answers("a", number));
    }
}
