//! What came of a reply the phone sent, and the replies it will not send:
//! one is sent once, and a second for the same wait only when the first was
//! known not to reach the session.

use std::collections::BTreeMap;

use tsumugi_protocol::{Answer, MutationState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Typed into the session's prompt.
    Sent,
    /// Never written to the work machine: the phone could not reach it, or
    /// the connection failed before the reply went.
    NotSent { why: String },
    /// The work machine refused it and typed nothing: the session moved on,
    /// or the reply could not be taken.
    Refused { why: String },
    /// Written, and what came of it was lost. It may have been typed.
    Unknown { why: String },
}

impl Outcome {
    /// Whether the draft may be let go.
    pub fn delivered(&self) -> bool {
        matches!(self, Outcome::Sent)
    }
}

/// What the work host said of an applied reply.
pub fn settled(state: MutationState) -> Outcome {
    match state {
        MutationState::Result { answer } => match *answer {
            Answer::Failed { text } => Outcome::Refused { why: text },
            Answer::Unconfirmed { text } => Outcome::Unknown { why: text },
            _ => Outcome::Sent,
        },
        MutationState::Rejected { text } => Outcome::Refused { why: text },
        MutationState::Full => Outcome::Refused {
            why: "the work machine has too many changes in flight".into(),
        },
        MutationState::Retired | MutationState::Conflict => Outcome::Refused {
            why: "the work machine did not take the reply".into(),
        },
        MutationState::Accepted | MutationState::Unknown => Outcome::Unknown {
            why: "the work machine did not say whether the reply was typed".into(),
        },
    }
}

/// Why a reply is not sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Held {
    #[error("a reply to this wait is on its way")]
    Sending,
    #[error("a reply to this wait was sent")]
    Sent,
    #[error("a reply to this wait may have been typed; look at the session before replying again")]
    Unconfirmed,
}

/// The replies sent for each wait, by the session and the wait's number.
#[derive(Debug, Default)]
pub struct Replies {
    sent: BTreeMap<(String, u64), Option<Outcome>>,
}

impl Replies {
    /// Mark a reply to `wait` of `session` as going, unless one went
    /// before that may have reached it.
    pub fn begin(&mut self, session: &str, wait: u64) -> Result<(), Held> {
        let key = (session.to_string(), wait);
        match self.sent.get(&key) {
            Some(None) => Err(Held::Sending),
            Some(Some(Outcome::Sent)) => Err(Held::Sent),
            Some(Some(Outcome::Unknown { .. })) => Err(Held::Unconfirmed),
            Some(Some(Outcome::NotSent { .. } | Outcome::Refused { .. })) | None => {
                self.sent.insert(key, None);
                Ok(())
            }
        }
    }

    pub fn end(&mut self, session: &str, wait: u64, outcome: &Outcome) {
        self.sent
            .insert((session.to_string(), wait), Some(outcome.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_host_said_of_a_reply_is_told_as_sent_refused_or_unknown() {
        let result = |answer| MutationState::Result {
            answer: Box::new(answer),
        };
        let cases = [
            (result(Answer::Done), "sent"),
            (
                result(Answer::Failed {
                    text: "moved on".into(),
                }),
                "refused",
            ),
            (
                result(Answer::Unconfirmed {
                    text: "lost".into(),
                }),
                "unknown",
            ),
            (MutationState::Rejected { text: "no".into() }, "refused"),
            (MutationState::Full, "refused"),
            (MutationState::Retired, "refused"),
            (MutationState::Conflict, "refused"),
            (MutationState::Accepted, "unknown"),
            (MutationState::Unknown, "unknown"),
        ];
        for (state, want) in cases {
            let told = match settled(state.clone()) {
                Outcome::Sent => "sent",
                Outcome::Refused { .. } => "refused",
                Outcome::Unknown { .. } => "unknown",
                Outcome::NotSent { .. } => "not sent",
            };
            assert_eq!(told, want, "{state:?}");
        }
    }

    #[test]
    fn only_a_reply_that_was_sent_lets_its_draft_go() {
        assert!(Outcome::Sent.delivered());
        for kept in [
            Outcome::NotSent { why: "".into() },
            Outcome::Refused { why: "".into() },
            Outcome::Unknown { why: "".into() },
        ] {
            assert!(!kept.delivered(), "{kept:?}");
        }
    }

    #[test]
    fn a_reply_is_not_sent_while_another_to_the_same_wait_is_on_its_way() {
        let mut replies = Replies::default();
        replies.begin("a", 1).unwrap();

        assert_eq!(replies.begin("a", 1), Err(Held::Sending));
        assert_eq!(replies.begin("a", 2), Ok(()));
        assert_eq!(replies.begin("b", 1), Ok(()));
    }

    #[test]
    fn a_wait_replied_to_or_maybe_replied_to_takes_no_second_reply() {
        let mut replies = Replies::default();
        for (session, outcome, held) in [
            ("a", Outcome::Sent, Held::Sent),
            ("b", Outcome::Unknown { why: "".into() }, Held::Unconfirmed),
        ] {
            replies.begin(session, 1).unwrap();
            replies.end(session, 1, &outcome);
            assert_eq!(replies.begin(session, 1), Err(held));
        }
    }

    #[test]
    fn a_reply_known_not_to_have_reached_the_session_may_be_sent_again() {
        let mut replies = Replies::default();
        for (session, outcome) in [
            ("a", Outcome::NotSent { why: "".into() }),
            ("b", Outcome::Refused { why: "".into() }),
        ] {
            replies.begin(session, 1).unwrap();
            replies.end(session, 1, &outcome);
            assert_eq!(replies.begin(session, 1), Ok(()), "{outcome:?}");
        }
    }
}
