//! The sessions that wait on the user, as the phone lists them from the
//! work the host sends: what each waits on, whether the phone can answer it,
//! and the last words of its conversation that say why.

use std::collections::BTreeSet;

use tsumugi_model::conversation::Role;
use tsumugi_model::{Agent, Session, State};
use tsumugi_protocol::WorkSnapshot;

/// How much of a message the phone shows.
const SAID: usize = 400;

/// What answers a wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Its turn ended: the next prompt, which the phone can send.
    Reply,
    /// A question or a dialog only a window answers, named in a few words.
    AtWindow { asks: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub session: String,
    /// The host's number of this wait, which a reply names.
    pub wait: u64,
    pub name: String,
    pub workspace: String,
    pub agent: Agent,
    pub answer: Answer,
    /// The last words of the user and of the agent, where the host read
    /// the conversation.
    pub last_user: Option<String>,
    pub last_agent: Option<String>,
}

/// The sessions of `work` that wait on the user, the one that began to
/// wait last first.
pub fn waiting(work: &WorkSnapshot) -> Vec<Waiting> {
    let Some(listed) = &work.tree.listed else {
        return Vec::new();
    };
    let mut waiting: Vec<Waiting> = listed
        .tree
        .sessions()
        .filter_map(|(workspace, session)| {
            let wait = *work.tree.waits.get(&session.id)?;
            let answer = answer(session)?;
            let (last_user, last_agent) = said(work, session);
            Some(Waiting {
                session: session.id.clone(),
                wait,
                name: session.name.clone(),
                workspace: workspace.to_string(),
                agent: session.agent,
                answer,
                last_user,
                last_agent,
            })
        })
        .collect();
    waiting.sort_by_key(|w| std::cmp::Reverse(w.wait));
    waiting
}

/// The sessions whose conversations the host is asked to read: those that
/// wait.
pub fn conversations(work: &WorkSnapshot) -> BTreeSet<String> {
    work.tree.waits.keys().cloned().collect()
}

fn answer(session: &Session) -> Option<Answer> {
    match &session.state {
        State::Done => Some(Answer::Reply),
        State::Waiting(wait) => Some(Answer::AtWindow {
            asks: wait.to_string(),
        }),
        _ => None,
    }
}

fn said(work: &WorkSnapshot, session: &Session) -> (Option<String>, Option<String>) {
    let Some(read) = work
        .details
        .sessions
        .get(&session.id)
        .and_then(|read| read.transcript.as_ref())
        .filter(|read| read.id == session.session_id)
    else {
        return (None, None);
    };
    let last = |role: Role| {
        read.messages
            .iter()
            .rev()
            .find(|m| {
                m.role == role && !m.thinking && m.tool.is_none() && !m.text.trim().is_empty()
            })
            .map(|m| short(&m.text))
    };
    (last(Role::User), last(Role::Assistant))
}

/// `text` on one line, cut to what the phone shows.
fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(SAID) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::Wait;
    use tsumugi_model::conversation::Message;
    use tsumugi_model::testing::session;
    use tsumugi_protocol::testing::{listed, snapshot};
    use tsumugi_protocol::{SessionRead, TranscriptRead};

    fn message(role: Role, text: &str) -> Message {
        Message {
            role,
            text: text.into(),
            at_ms: None,
            thinking: false,
            tool: None,
        }
    }

    fn work(sessions: Vec<Session>, waits: &[(&str, u64)]) -> WorkSnapshot {
        snapshot()
            .listed(listed().sessions(sessions))
            .with(|work| {
                work.tree.waits = waits.iter().map(|(id, n)| (id.to_string(), *n)).collect();
            })
            .build()
    }

    fn ids(waiting: &[Waiting]) -> Vec<&str> {
        waiting.iter().map(|w| w.session.as_str()).collect()
    }

    #[test]
    fn nothing_waits_before_the_sessions_are_listed() {
        assert!(waiting(&WorkSnapshot::default()).is_empty());
    }

    #[test]
    fn only_the_sessions_the_host_numbers_a_wait_of_are_listed() {
        let work = work(
            vec![
                session("a").state(State::Done).build(),
                session("b").state(State::Working).build(),
                session("c").state(State::Done).build(),
            ],
            &[("a", 1)],
        );

        assert_eq!(ids(&waiting(&work)), ["a"]);
    }

    #[test]
    fn the_session_that_began_to_wait_last_comes_first() {
        let work = work(
            vec![
                session("a").state(State::Done).build(),
                session("b").state(State::Done).build(),
            ],
            &[("a", 4), ("b", 9)],
        );

        assert_eq!(ids(&waiting(&work)), ["b", "a"]);
    }

    #[test]
    fn a_turn_that_ended_is_answered_by_a_reply_and_a_question_or_a_dialog_at_a_window() {
        let work = work(
            vec![
                session("done").state(State::Done).build(),
                session("asks").state(State::Waiting(Wait::Input)).build(),
                session("allow")
                    .state(State::Waiting(Wait::Permission))
                    .build(),
            ],
            &[("done", 3), ("asks", 2), ("allow", 1)],
        );

        let answers: Vec<_> = waiting(&work).into_iter().map(|w| w.answer).collect();

        assert_eq!(
            answers,
            [
                Answer::Reply,
                Answer::AtWindow {
                    asks: "waiting for input".into()
                },
                Answer::AtWindow {
                    asks: "permission prompt".into()
                },
            ]
        );
    }

    #[test]
    fn a_wait_carries_the_last_words_of_the_user_and_of_the_agent() {
        let mut work = work(
            vec![session("a").session_id("sid").state(State::Done).build()],
            &[("a", 1)],
        );
        let mut thinking = message(Role::Assistant, "pondering");
        thinking.thinking = true;
        work.details.sessions.insert(
            "a".into(),
            SessionRead {
                transcript: Some(TranscriptRead {
                    id: "sid".into(),
                    messages: vec![
                        message(Role::User, "first"),
                        message(Role::User, "fix   the\nbuild"),
                        message(Role::Assistant, "Should I push it?"),
                        thinking,
                        message(Role::Assistant, "  "),
                    ],
                }),
                ..SessionRead::default()
            },
        );

        let waits = waiting(&work);

        assert_eq!(waits[0].last_user.as_deref(), Some("fix the build"));
        assert_eq!(waits[0].last_agent.as_deref(), Some("Should I push it?"));
    }

    #[test]
    fn the_words_of_a_conversation_the_session_left_are_not_shown() {
        let mut work = work(
            vec![session("a").session_id("now").state(State::Done).build()],
            &[("a", 1)],
        );
        work.details.sessions.insert(
            "a".into(),
            SessionRead {
                transcript: Some(TranscriptRead {
                    id: "before".into(),
                    messages: vec![message(Role::User, "old")],
                }),
                ..SessionRead::default()
            },
        );

        assert_eq!(waiting(&work)[0].last_user, None);
    }

    #[test]
    fn long_words_are_cut() {
        let text = "word ".repeat(200);

        let cut = short(&text);

        assert_eq!(cut.chars().count(), SAID + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn the_conversations_read_are_those_of_the_sessions_that_wait() {
        let work = work(vec![], &[("a", 1), ("b", 2)]);

        assert_eq!(
            conversations(&work),
            ["a".to_string(), "b".to_string()].into_iter().collect()
        );
    }
}
