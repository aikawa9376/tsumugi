//! What needed the user, kept so it can be looked at later: a session that
//! began waiting on them, one that failed, one whose turn ended.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{State, Tree};

/// How many notices are kept; the oldest go first.
pub const KEPT: usize = 200;

/// Words of a reason longer than this are cut.
const REASON: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    Waiting,
    Failed,
    Done,
}

impl NoticeKind {
    pub const ALL: [NoticeKind; 3] = [NoticeKind::Waiting, NoticeKind::Failed, NoticeKind::Done];

    /// The kind in one word, as the settings and the list name it.
    pub fn word(self) -> &'static str {
        match self {
            NoticeKind::Waiting => "waiting",
            NoticeKind::Failed => "failed",
            NoticeKind::Done => "done",
        }
    }

    fn of(state: &State) -> Option<NoticeKind> {
        match state {
            State::Waiting(_) => Some(NoticeKind::Waiting),
            State::Failed => Some(NoticeKind::Failed),
            State::Done => Some(NoticeKind::Done),
            _ => None,
        }
    }
}

/// One time a session needed the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    /// Which notice this is among the host's; later ones count higher, and a
    /// number is never given twice.
    pub id: u64,
    pub kind: NoticeKind,
    /// The session's id, and its name and workspace as they were then.
    pub session: String,
    pub name: String,
    pub workspace: String,
    pub at_ms: u64,
    /// Why, in a few words.
    pub reason: String,
}

/// Whether a notice still describes how its session stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// The session is still in the state the notice tells of, and nothing
    /// later was noticed of it.
    Current,
    /// The session has moved on.
    Past,
    /// The session is no longer listed.
    Ended,
}

/// The notices a work host keeps, and what it needs to add the next ones.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notices {
    next: u64,
    notices: Vec<Notice>,
    /// The kind each session stood in when last looked at; none until the
    /// first look, which tells of nothing, since what stood then is not news.
    #[serde(skip)]
    seen: Option<BTreeMap<String, Option<NoticeKind>>>,
}

impl Notices {
    /// From what was written by [`Notices::to_json`]; empty for a text that
    /// is not that.
    pub fn from_json(text: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(text)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("notices are plain data")
    }

    /// The notices kept, the oldest first.
    pub fn list(&self) -> &[Notice] {
        &self.notices
    }

    /// Look at the sessions of `tree` at `now_ms`, noticing each that entered
    /// a state the user is needed in since the last look; whether any was.
    ///
    /// A session that stays in its state, or changes only what it waits for,
    /// is noticed once, however often it is looked at.
    pub fn look(&mut self, tree: &Tree, now_ms: u64) -> bool {
        let first = self.seen.is_none();
        let before = self.seen.take().unwrap_or_default();
        let mut after = BTreeMap::new();
        let mut added = false;
        for (workspace, session) in tree.sessions() {
            let kind = NoticeKind::of(&session.state);
            let known = before.get(&session.id).copied();
            after.insert(session.id.clone(), kind);
            let Some(kind) = kind else { continue };
            // A session new since the first look is noticed in whatever
            // state it is first seen, as its state is news.
            if first || known == Some(Some(kind)) {
                continue;
            }
            let reason = match &session.state {
                State::Waiting(wait) => wait.to_string(),
                _ => session.status_message.clone().unwrap_or_default(),
            };
            self.notices.push(Notice {
                id: self.next,
                kind,
                session: session.id.clone(),
                name: session.name.clone(),
                workspace: workspace.to_owned(),
                at_ms: now_ms,
                reason: short(&reason),
            });
            self.next += 1;
            added = true;
        }
        self.seen = Some(after);
        if self.notices.len() > KEPT {
            let over = self.notices.len() - KEPT;
            self.notices.drain(..over);
        }
        added
    }

    /// How each notice stands against `tree`, in the order of
    /// [`Notices::list`].
    pub fn standings(&self, tree: &Tree) -> Vec<Standing> {
        standings(&self.notices, tree)
    }
}

/// How each of `notices`, the oldest first, stands against `tree`.
pub fn standings(notices: &[Notice], tree: &Tree) -> Vec<Standing> {
    notices
        .iter()
        .enumerate()
        .map(|(at, notice)| {
            let Some((_, _, session)) = tree.find(&notice.session) else {
                return Standing::Ended;
            };
            let later = notices[at + 1..]
                .iter()
                .any(|n| n.session == notice.session);
            if !later && NoticeKind::of(&session.state) == Some(notice.kind) {
                Standing::Current
            } else {
                Standing::Past
            }
        })
        .collect()
}

/// `text` on one line, cut where it runs long.
fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= REASON {
        return line;
    }
    let cut: String = line.chars().take(REASON - 1).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Wait;
    use crate::testing::{session, tree};

    fn looked(notices: &mut Notices, states: &[(&str, State)], at: u64) -> bool {
        let mut builder = tree();
        for (name, state) in states {
            builder = builder.session(session(*name).cwd("/api").state(state.clone()));
        }
        notices.look(&builder.build(), at)
    }

    fn started() -> Notices {
        let mut notices = Notices::default();
        looked(&mut notices, &[("a", State::Working)], 0);
        notices
    }

    #[test]
    fn what_stood_at_the_first_look_is_not_news() {
        let mut notices = Notices::default();
        let added = looked(&mut notices, &[("a", State::Waiting(Wait::Input))], 1);
        assert!(!added);
        assert!(notices.list().is_empty());
    }

    #[test]
    fn a_session_entering_each_state_is_noticed_once_with_where_and_when() {
        for (state, kind) in [
            (State::Waiting(Wait::Permission), NoticeKind::Waiting),
            (State::Failed, NoticeKind::Failed),
            (State::Done, NoticeKind::Done),
        ] {
            let mut notices = started();
            assert!(looked(&mut notices, &[("a", state.clone())], 5));
            for at in 6..9 {
                assert!(!looked(&mut notices, &[("a", state.clone())], at));
            }
            let [notice] = notices.list() else {
                panic!("one notice")
            };
            assert_eq!(notice.kind, kind);
            assert_eq!(notice.name, "a");
            assert_eq!(notice.workspace, "api");
            assert_eq!(notice.at_ms, 5);
        }
    }

    #[test]
    fn states_that_need_nobody_are_not_noticed() {
        let mut notices = started();
        for state in [
            State::Idle,
            State::Background,
            State::Starting,
            State::Working,
        ] {
            assert!(!looked(&mut notices, &[("a", state)], 1));
        }
    }

    #[test]
    fn a_session_waiting_for_something_else_is_not_noticed_again() {
        let mut notices = started();
        looked(&mut notices, &[("a", State::Waiting(Wait::Permission))], 1);
        assert!(!looked(
            &mut notices,
            &[("a", State::Waiting(Wait::Input))],
            2
        ));
        assert_eq!(notices.list().len(), 1);
    }

    #[test]
    fn a_session_that_waits_again_after_working_is_noticed_again() {
        let mut notices = started();
        looked(&mut notices, &[("a", State::Waiting(Wait::Input))], 1);
        looked(&mut notices, &[("a", State::Working)], 2);
        assert!(looked(
            &mut notices,
            &[("a", State::Waiting(Wait::Input))],
            3
        ));
        let ids: Vec<u64> = notices.list().iter().map(|n| n.id).collect();
        assert_eq!(ids, [0, 1]);
    }

    #[test]
    fn a_session_first_seen_after_the_first_look_is_noticed_in_the_state_it_shows() {
        let mut notices = started();
        let added = looked(
            &mut notices,
            &[("a", State::Working), ("b", State::Done)],
            1,
        );
        assert!(added);
        assert_eq!(notices.list()[0].name, "b");
    }

    #[test]
    fn a_reason_is_one_short_line() {
        let mut notices = started();
        let long = format!("line one\n  line two {}", "x".repeat(300));
        let mut s = session("a").cwd("/api").state(State::Failed).build();
        s.status_message = Some(long);
        notices.look(&tree().session(s).build(), 1);
        let reason = &notices.list()[0].reason;
        assert!(reason.starts_with("line one line two"));
        assert!(!reason.contains('\n'));
        assert!(reason.chars().count() <= REASON);
        assert!(reason.ends_with('…'));
    }

    #[test]
    fn the_oldest_go_past_the_limit_and_their_numbers_are_not_given_again() {
        let mut notices = started();
        for n in 0..2 * (KEPT as u64 + 5) {
            let state = if n % 2 == 0 {
                State::Done
            } else {
                State::Working
            };
            looked(&mut notices, &[("a", state)], n);
        }
        assert_eq!(notices.list().len(), KEPT);
        let ids: Vec<u64> = notices.list().iter().map(|n| n.id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        assert!(ids[0] > 0);
        let last = *ids.last().unwrap();
        looked(&mut notices, &[("a", State::Working)], 1000);
        looked(&mut notices, &[("a", State::Failed)], 1001);
        assert_eq!(notices.list().last().unwrap().id, last + 1);
    }

    #[test]
    fn what_is_kept_comes_back_and_does_not_tell_of_what_still_stands() {
        let mut notices = started();
        looked(&mut notices, &[("a", State::Done)], 1);
        let mut back = Notices::from_json(notices.to_json().as_bytes()).unwrap();
        assert_eq!(back.list(), notices.list());
        // The first look after coming back only takes in how things stand.
        assert!(!looked(&mut back, &[("a", State::Done)], 2));
        looked(&mut back, &[("a", State::Working)], 3);
        looked(&mut back, &[("a", State::Failed)], 4);
        assert_eq!(back.list().last().unwrap().id, 1);
    }

    #[test]
    fn a_notice_stands_current_only_while_its_session_stays_in_that_state() {
        let mut notices = started();
        looked(&mut notices, &[("a", State::Waiting(Wait::Input))], 1);
        let now = |notices: &Notices, state: State| {
            let t = tree()
                .session(session("a").cwd("/api").state(state))
                .build();
            notices.standings(&t)
        };
        assert_eq!(
            now(&notices, State::Waiting(Wait::Input)),
            [Standing::Current]
        );
        assert_eq!(now(&notices, State::Working), [Standing::Past]);
        assert_eq!(notices.standings(&tree().build()), [Standing::Ended]);
    }

    #[test]
    fn an_earlier_notice_of_a_session_is_past_once_a_later_one_was_made() {
        let mut notices = started();
        looked(&mut notices, &[("a", State::Done)], 1);
        looked(&mut notices, &[("a", State::Working)], 2);
        looked(&mut notices, &[("a", State::Done)], 3);
        let t = tree()
            .session(session("a").cwd("/api").state(State::Done))
            .build();
        assert_eq!(notices.standings(&t), [Standing::Past, Standing::Current]);
    }
}
