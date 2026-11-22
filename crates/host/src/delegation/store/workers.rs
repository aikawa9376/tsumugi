//! The sessions a session opened to supervise, and what they tell it: a
//! report of their own, or that they stopped without making one.
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use tsumugi_model::{Rest, Session as Shown, SessionId, State};

use super::{
    Code, Error, MESSAGE_COLUMNS, Message, MessageId, Result, Session, Store, insert_kind,
    read_message, session,
};

/// What `workers.event` holds once the worker reported itself: the last word,
/// after which nothing more is said of it.
const REPORTED: &str = "reported";
/// What it holds once the worker's session ended without a report.
const ENDED: &str = "ended";

/// How a worker stands, as far as it is told of.
#[derive(Debug, PartialEq, Eq)]
enum Standing {
    Working,
    TurnEnded,
    Waiting(String),
    Failed,
    Ended,
}

impl Standing {
    fn key(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::TurnEnded => "turn_ended",
            Self::Waiting(_) => "waiting",
            Self::Failed => "failed",
            Self::Ended => ENDED,
        }
    }

    /// How the tree shows a worker; none while it only starts.
    fn of(state: &State) -> Option<Self> {
        match state {
            State::Starting => None,
            State::Working | State::Background => Some(Self::Working),
            State::Idle | State::Done => Some(Self::TurnEnded),
            State::Waiting(wait) => Some(Self::Waiting(wait.to_string())),
            State::Failed => Some(Self::Failed),
        }
    }

    /// What its opener is told on moving to this from `was`, if anything: a
    /// turn that ends is told only after the worker worked, and a worker
    /// that still works behind its turn has not stopped.
    fn told(&self, was: &str) -> Option<String> {
        let body = match self {
            Self::Working => return None,
            Self::TurnEnded if matches!(was, "working" | "waiting") => {
                "worker_stopped\nevent: turn_ended\nThe worker's turn ended and it made no report. \
                 It may be waiting for a next prompt: read its console, or ask it."
                    .to_owned()
            }
            Self::TurnEnded => return None,
            Self::Waiting(wait) => format!(
                "worker_stopped\nevent: waiting\nThe worker waits on the user ({wait}). \
                 It cannot go on until the user answers in its console."
            ),
            Self::Failed => "worker_stopped\nevent: failed\noutcome: failed\n\
                 The agent of the worker reported a failure."
                .to_owned(),
            Self::Ended => "worker_stopped\nevent: ended\noutcome: failed\n\
                 The worker's session ended before it reported."
                .to_owned(),
        };
        Some(body)
    }
}

/// What a worker's report says, as its opener reads it.
fn report(outcome: &str, summary: &str, files: &[String]) -> String {
    let mut body = format!("worker_done\noutcome: {outcome}\nsummary: {summary}\n");
    if !files.is_empty() {
        body.push_str("files modified:\n");
        for file in files {
            body.push_str(&format!("- {file}\n"));
        }
    }
    body
}

impl Store {
    /// Records that the session that opened `worker` supervises it.
    pub fn supervise(&mut self, worker: SessionId) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = session(&transaction, worker)?
            .ok_or_else(|| Error::new(Code::NotFound, "the session is unknown"))?;
        if record.parent.is_none() {
            return Err(Error::new(
                Code::Refused,
                "only a session another session opened can be supervised",
            ));
        }
        transaction.execute(
            "INSERT OR IGNORE INTO workers (session, event) VALUES (?1, '')",
            [worker.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Records that the session that supervises `worker` ends it itself, so
    /// its session ending is not told to that session as a failure. A worker
    /// that reported, or that nobody supervises, is left as it is.
    pub fn release_worker(&mut self, worker: SessionId) -> Result<()> {
        self.connection.execute(
            "UPDATE workers SET event = ?2 WHERE session = ?1 AND event != ?3",
            params![worker.to_string(), ENDED, REPORTED],
        )?;
        Ok(())
    }

    /// Tells the session that opened `worker` how its work ended, once: a
    /// report that is refused leaves the worker free to make it again.
    pub fn report_done(
        &mut self,
        worker: SessionId,
        outcome: &str,
        summary: &str,
        files: &[String],
    ) -> Result<MessageId> {
        if !matches!(outcome, "succeeded" | "failed") {
            return Err(Error::new(
                Code::Invalid,
                "the outcome is succeeded or failed",
            ));
        }
        let summary = summary.trim();
        if summary.is_empty() {
            return Err(Error::new(Code::Invalid, "the summary is empty"));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let event = event_of(&transaction, worker)?.ok_or_else(|| {
            Error::new(
                Code::Refused,
                "no session supervises this one, so there is no one to report to",
            )
        })?;
        if event == REPORTED {
            return Err(Error::new(Code::Refused, "this session already reported"));
        }
        let opener = opener_of(&transaction, worker)?;
        if opener.ended {
            return Err(Error::new(
                Code::Refused,
                format!("{} has ended and cannot be told", opener.name),
            ));
        }
        let id = insert_kind(
            &transaction,
            worker,
            opener.id,
            "worker",
            &report(outcome, summary, files),
        )?;
        transaction.execute(
            "UPDATE workers SET event = ?2 WHERE session = ?1",
            params![worker.to_string(), REPORTED],
        )?;
        transaction.commit()?;
        Ok(id)
    }

    /// Looks at how the tree shows each worker that has not reported, and
    /// tells its opener once of each stop: the turn ending after work, a
    /// dialog holding it, a failure, or its session ending. What was told is
    /// recorded in the same step as the message, so none is told twice.
    pub fn follow_workers(&mut self, shown: &[Shown]) -> Result<()> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let workers: Vec<(String, String)> = {
            let mut statement = transaction.prepare(
                "SELECT session, event FROM workers WHERE event NOT IN (?1, ?2) ORDER BY rowid",
            )?;
            statement
                .query_map([REPORTED, ENDED], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        for (worker, was) in workers {
            let worker: SessionId = worker.parse().map_err(Error::storage)?;
            let Some(record) = session(&transaction, worker)? else {
                continue;
            };
            // Asleep, not ended: it is woken as the same worker.
            if record.rest == Some(Rest::Hibernated) {
                continue;
            }
            let standing = if record.ended {
                Some(Standing::Ended)
            } else {
                shown
                    .iter()
                    .find(|shown| shown.delegation == Some(worker))
                    .and_then(|shown| Standing::of(&shown.state))
            };
            let Some(standing) = standing.filter(|standing| standing.key() != was) else {
                continue;
            };
            if let Some(body) = standing.told(&was) {
                let opener = opener_of(&transaction, worker)?;
                if !opener.ended {
                    insert_kind(&transaction, worker, opener.id, "worker", &body)?;
                }
            }
            transaction.execute(
                "UPDATE workers SET event = ?2 WHERE session = ?1",
                params![worker.to_string(), standing.key()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Takes the oldest message waiting for `recipient` as delivered, with
    /// its sender: what its agent reads in place of hearing it between its
    /// turns.
    pub fn take_next(
        &mut self,
        recipient: SessionId,
    ) -> Result<Option<(Message, Option<Session>)>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if session(&transaction, recipient)?.is_none_or(|session| session.ended) {
            return Ok(None);
        }
        let waiting = transaction
            .query_row(
                &format!(
                    "SELECT {MESSAGE_COLUMNS} FROM messages WHERE recipient = ?1 AND delivered = 0 ORDER BY seq LIMIT 1"
                ),
                [recipient.to_string()],
                read_message,
            )
            .optional()?
            .transpose()?;
        let Some(message) = waiting else {
            return Ok(None);
        };
        transaction.execute(
            "UPDATE messages SET delivered = 1 WHERE id = ?1",
            [message.id.to_string()],
        )?;
        let sender = match message.sender {
            Some(sender) => session(&transaction, sender)?,
            None => None,
        };
        transaction.commit()?;
        Ok(Some((message, sender)))
    }
}

/// What was last told of `worker`, none when no session supervises it.
fn event_of(connection: &rusqlite::Connection, worker: SessionId) -> Result<Option<String>> {
    Ok(connection
        .query_row(
            "SELECT event FROM workers WHERE session = ?1",
            [worker.to_string()],
            |row| row.get(0),
        )
        .optional()?)
}

/// The session that opened `worker`.
fn opener_of(connection: &rusqlite::Connection, worker: SessionId) -> Result<Session> {
    let parent = session(connection, worker)?
        .and_then(|record| record.parent)
        .ok_or_else(|| Error::new(Code::NotFound, "the session has no opener"))?;
    session(connection, parent)?
        .ok_or_else(|| Error::new(Code::NotFound, "the opening session is unknown"))
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::delegation::store::NewSession;
    use tsumugi_model::{State, Wait, testing::session};

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
        (dir, store)
    }

    fn register(store: &mut Store, parent: Option<SessionId>, name: &str) -> SessionId {
        store
            .register(NewSession {
                parent,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: name.into(),
                cwd: "/repo".into(),
                launch: crate::settings::Settings::default(),
                model: None,
                permissions: AgentPermissions::Default,
                started_ms: 1,
                name_given: false,
                code: None,
            })
            .unwrap()
            .0
    }

    /// A supervisor and a worker it opened and asked to supervise.
    fn supervised(store: &mut Store) -> (SessionId, SessionId) {
        let supervisor = register(store, None, "supervisor");
        let worker = register(store, Some(supervisor), "worker");
        store.supervise(worker).unwrap();
        (supervisor, worker)
    }

    /// What the tree shows of `worker` while it stands as `state`.
    fn shown(worker: SessionId, state: State) -> Vec<tsumugi_model::Session> {
        let mut shown = session("a").build();
        shown.delegation = Some(worker);
        shown.state = state;
        vec![shown]
    }

    fn told(store: &mut Store, supervisor: SessionId) -> Vec<String> {
        let mut bodies = Vec::new();
        while let Some(message) = store.next_undelivered(supervisor).unwrap() {
            store.delivered(message.id).unwrap();
            bodies.push(message.body);
        }
        bodies
    }

    fn follow(store: &mut Store, worker: SessionId, state: State) {
        store.follow_workers(&shown(worker, state)).unwrap();
    }

    #[test]
    fn only_a_session_another_opened_can_be_supervised() {
        let (_dir, mut store) = store();
        let alone = register(&mut store, None, "alone");

        let refused = store.supervise(alone).unwrap_err();

        assert_eq!(refused.code, Code::Refused);
    }

    #[test]
    fn a_worker_s_report_reaches_the_session_that_opened_it() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);

        store
            .report_done(
                worker,
                "succeeded",
                "Fixed the flaky test",
                &["a.rs".into()],
            )
            .unwrap();

        let body = told(&mut store, supervisor).remove(0);
        assert!(
            body.starts_with("worker_done\noutcome: succeeded\n"),
            "{body}"
        );
        assert!(body.contains("Fixed the flaky test"), "{body}");
        assert!(body.contains("- a.rs"), "{body}");
    }

    #[test]
    fn a_worker_reports_once() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        store.report_done(worker, "failed", "stuck", &[]).unwrap();

        let again = store.report_done(worker, "succeeded", "done", &[]);

        assert_eq!(again.unwrap_err().code, Code::Refused);
        assert_eq!(told(&mut store, supervisor).len(), 1);
    }

    #[test]
    fn a_session_nobody_supervises_cannot_report() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let opened = register(&mut store, Some(opener), "opened");

        let refused = store.report_done(opened, "succeeded", "done", &[]);

        assert_eq!(refused.unwrap_err().code, Code::Refused);
        assert!(told(&mut store, opener).is_empty());
    }

    #[test]
    fn an_outcome_is_succeeded_or_failed() {
        let (_dir, mut store) = store();
        let (_, worker) = supervised(&mut store);

        let refused = store.report_done(worker, "meh", "done", &[]);

        assert_eq!(refused.unwrap_err().code, Code::Invalid);
    }

    #[test]
    fn a_report_to_an_opener_that_ended_is_refused_and_may_be_made_again() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        store.end(supervisor).unwrap();

        let refused = store.report_done(worker, "succeeded", "done", &[]);

        assert_eq!(refused.unwrap_err().code, Code::Refused);
        assert!(store.session(worker).unwrap().is_some());
    }

    #[test]
    fn a_worker_whose_turn_ends_is_reported_once_and_again_after_it_works() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);

        follow(&mut store, worker, State::Working);
        follow(&mut store, worker, State::Idle);
        follow(&mut store, worker, State::Idle);
        follow(&mut store, worker, State::Done);
        assert_eq!(told(&mut store, supervisor).len(), 1);

        follow(&mut store, worker, State::Working);
        follow(&mut store, worker, State::Done);
        let again = told(&mut store, supervisor);
        assert_eq!(again.len(), 1);
        assert!(
            again[0].starts_with("worker_stopped\nevent: turn_ended\n"),
            "{again:?}"
        );
    }

    #[test]
    fn a_worker_that_has_not_yet_worked_is_not_reported_idle() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);

        follow(&mut store, worker, State::Starting);
        follow(&mut store, worker, State::Idle);

        assert!(told(&mut store, supervisor).is_empty());
    }

    #[test]
    fn a_worker_that_works_behind_its_turn_is_not_reported() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);

        follow(&mut store, worker, State::Working);
        follow(&mut store, worker, State::Background);

        assert!(told(&mut store, supervisor).is_empty());
    }

    #[test]
    fn a_worker_held_at_a_dialog_is_reported_once_with_what_it_waits_on() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);

        follow(&mut store, worker, State::Working);
        follow(&mut store, worker, State::Waiting(Wait::Permission));
        follow(&mut store, worker, State::Waiting(Wait::Permission));

        let told = told(&mut store, supervisor);
        assert_eq!(told.len(), 1);
        assert!(
            told[0].starts_with("worker_stopped\nevent: waiting\n"),
            "{told:?}"
        );
        assert!(told[0].contains("permission"), "{told:?}");
    }

    #[test]
    fn a_worker_that_failed_is_reported_as_failed() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);

        follow(&mut store, worker, State::Working);
        follow(&mut store, worker, State::Failed);

        let told = told(&mut store, supervisor);
        assert!(told[0].contains("outcome: failed"), "{told:?}");
    }

    #[test]
    fn a_worker_asleep_is_not_reported_as_failed_and_goes_on_supervised_once_woken() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        follow(&mut store, worker, State::Working);
        store.hibernate(worker).unwrap();

        store.follow_workers(&[]).unwrap();
        assert!(told(&mut store, supervisor).is_empty());

        let woken = register(&mut store, Some(supervisor), "worker");
        store.brought_back(worker, woken).unwrap();
        follow(&mut store, woken, State::Working);
        store.end(woken).unwrap();
        store.follow_workers(&[]).unwrap();

        let told = told(&mut store, supervisor);
        assert_eq!(told.len(), 1, "{told:?}");
        assert!(
            told[0].starts_with("worker_stopped\nevent: ended\n"),
            "{told:?}"
        );
    }

    #[test]
    fn a_worker_that_ended_without_reporting_reaches_its_opener_as_a_failure_once() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        follow(&mut store, worker, State::Working);
        store.end(worker).unwrap();

        // The tree no longer shows it.
        store.follow_workers(&[]).unwrap();
        store.follow_workers(&[]).unwrap();

        let told = told(&mut store, supervisor);
        assert_eq!(told.len(), 1);
        assert!(
            told[0].starts_with("worker_stopped\nevent: ended\noutcome: failed\n"),
            "{told:?}"
        );
    }

    #[test]
    fn nothing_more_is_reported_once_the_worker_reported_itself() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        follow(&mut store, worker, State::Working);
        store.report_done(worker, "succeeded", "done", &[]).unwrap();

        follow(&mut store, worker, State::Done);
        follow(&mut store, worker, State::Waiting(Wait::Input));
        store.end(worker).unwrap();
        store.follow_workers(&[]).unwrap();

        assert_eq!(told(&mut store, supervisor).len(), 1);
    }

    #[test]
    fn a_session_opened_without_supervision_is_never_reported() {
        let (_dir, mut store) = store();
        let opener = register(&mut store, None, "opener");
        let opened = register(&mut store, Some(opener), "opened");

        follow(&mut store, opened, State::Working);
        follow(&mut store, opened, State::Done);
        store.end(opened).unwrap();
        store.follow_workers(&[]).unwrap();

        assert!(told(&mut store, opener).is_empty());
    }

    #[test]
    fn a_worker_its_supervisor_ended_is_not_reported_as_a_failure() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        follow(&mut store, worker, State::Working);

        store.release_worker(worker).unwrap();
        store.end(worker).unwrap();
        store.follow_workers(&[]).unwrap();

        assert!(told(&mut store, supervisor).is_empty());
    }

    #[test]
    fn releasing_a_worker_that_reported_or_was_never_supervised_changes_nothing() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        let alone = register(&mut store, Some(supervisor), "alone");
        store.report_done(worker, "succeeded", "done", &[]).unwrap();

        store.release_worker(worker).unwrap();
        store.release_worker(alone).unwrap();

        assert_eq!(
            store
                .report_done(worker, "failed", "again", &[])
                .unwrap_err()
                .message,
            "this session already reported"
        );
        assert!(super::event_of(&store.connection, alone).unwrap().is_none());
    }

    #[test]
    fn waiting_takes_the_oldest_message_waiting_and_nothing_twice() {
        let (_dir, mut store) = store();
        let (supervisor, worker) = supervised(&mut store);
        store.report_done(worker, "succeeded", "done", &[]).unwrap();
        store.send(worker, supervisor, "one more thing").unwrap();

        let (first, sender) = store.take_next(supervisor).unwrap().unwrap();
        let (second, _) = store.take_next(supervisor).unwrap().unwrap();

        assert!(first.body.starts_with("worker_done\n"));
        assert_eq!(sender.unwrap().id, worker);
        assert_eq!(second.body, "one more thing");
        assert!(store.take_next(supervisor).unwrap().is_none());
        assert!(store.next_undelivered(supervisor).unwrap().is_none());
    }
}
