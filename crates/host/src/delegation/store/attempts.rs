//! The attempts an issue is tried as, side by side, each a session in a
//! worktree of its own.

use rusqlite::{TransactionBehavior, params};
use tsumugi_model::{Attempt, LocalIssue, LocalIssueState, Session, SessionId, TryAs};

use super::{Code, Error, Result, Store};

impl Store {
    /// Take up the issue `selector` names as one attempt for each of
    /// `tries`, all made from `base`, in place of the attempts it had. Only
    /// an issue that waits can be taken up, and only as one attempt or more.
    pub fn begin_attempts(
        &mut self,
        selector: &str,
        tries: &[TryAs],
        base: &str,
    ) -> Result<(u64, LocalIssue)> {
        if tries.is_empty() {
            return Err(Error::new(Code::Invalid, "Name at least one attempt"));
        }
        let (number, issue) = self.local_issue_named(selector)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE local_issues SET state = 'starting', session = NULL, error = NULL, worked = 0
             WHERE id = ?1 AND state IN ('open', 'queued', 'failed', 'cancelled')",
            [&issue.id],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                format!(
                    "Issue #{number} is {}: only an open, queued, failed or cancelled issue can be started",
                    issue.state.word()
                ),
            ));
        }
        transaction.execute("DELETE FROM issue_attempts WHERE issue = ?1", [&issue.id])?;
        for (at, tried) in tries.iter().enumerate() {
            transaction.execute(
                "INSERT INTO issue_attempts (issue, number, profile, model, base, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'starting')",
                params![issue.id, at as i64 + 1, tried.profile, tried.model, base],
            )?;
        }
        transaction.commit()?;
        self.local_issue_named(&issue.id)
    }

    /// Link the durable record of attempt `number` before its program can run.
    pub fn attach_attempt(&mut self, issue: &str, number: u32, session: SessionId) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE issue_attempts SET session = ?3
             WHERE issue = ?1 AND number = ?2 AND state = 'starting' AND session IS NULL",
            params![issue, number, session.to_string()],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                "The attempt is no longer waiting for its session",
            ));
        }
        Ok(())
    }

    /// Mark attempt `number` failed, and the issue with it once every
    /// attempt has.
    pub fn fail_attempt(&mut self, issue: &str, number: u32, error: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE issue_attempts SET state = 'failed', error = ?3 WHERE issue = ?1 AND number = ?2",
            params![issue, number, error],
        )?;
        self.stand_by_attempts(issue)
    }

    /// The attempts of `issue`, in their order.
    pub(super) fn attempts_of(&self, issue: &str) -> Result<Vec<Attempt>> {
        let mut query = self.connection.prepare_cached(
            "SELECT a.number, a.profile, a.model, a.base, a.state, a.session, s.terminal, s.cwd, a.error
             FROM issue_attempts a LEFT JOIN sessions s ON s.id = a.session
             WHERE a.issue = ?1 ORDER BY a.number",
        )?;
        let rows = query.query_map([issue], |row| {
            let state: String = row.get(4)?;
            let record: Option<String> = row.get(5)?;
            let invalid = |at: usize| {
                move |error: Box<dyn std::error::Error + Send + Sync>| {
                    rusqlite::Error::FromSqlConversionFailure(
                        at,
                        rusqlite::types::Type::Text,
                        error,
                    )
                }
            };
            Ok(Attempt {
                number: row.get(0)?,
                tried_as: TryAs {
                    profile: row.get(1)?,
                    model: row.get(2)?,
                },
                base: row.get(3)?,
                state: LocalIssueState::parse(&state)
                    .ok_or_else(|| invalid(4)(format!("no state {state}").into()))?,
                record: record
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|e| invalid(5)(Box::new(e)))?,
                session: row.get(6)?,
                worktree: row.get::<_, Option<String>>(7)?.map(Into::into),
                error: row.get(8)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Follow each attempt of `issue` by its session in `sessions`, as an
    /// issue worked where it is is followed, and the issue by them all.
    pub(super) fn follow_attempts(&mut self, issue: &str, sessions: &[Session]) -> Result<()> {
        for attempt in self.attempts_of(issue)? {
            if !matches!(
                attempt.state,
                LocalIssueState::Starting | LocalIssueState::Running | LocalIssueState::Waiting
            ) {
                continue;
            }
            // An attempt still being started has no session to follow yet.
            if attempt.record.is_none() && attempt.state == LocalIssueState::Starting {
                continue;
            }
            let worked: bool = self.connection.query_row(
                "SELECT worked FROM issue_attempts WHERE issue = ?1 AND number = ?2",
                params![issue, attempt.number],
                |row| row.get(0),
            )?;
            let left = self.left(attempt.record)?;
            let (state, error, worked) =
                super::issues::followed(attempt.record, sessions, worked, attempt.state, left);
            self.connection.execute(
                "UPDATE issue_attempts SET state = ?3, error = ?4, worked = ?5 WHERE issue = ?1 AND number = ?2",
                params![issue, attempt.number, state.word(), error, worked],
            )?;
        }
        self.stand_by_attempts(issue)
    }

    /// Set `issue`'s state from its attempts', while it is at work.
    fn stand_by_attempts(&mut self, issue: &str) -> Result<()> {
        let states: Vec<LocalIssueState> = self
            .attempts_of(issue)?
            .into_iter()
            .map(|attempt| attempt.state)
            .collect();
        let state = LocalIssueState::of_attempts(&states);
        let error = (state == LocalIssueState::Failed).then_some("Every attempt failed");
        self.connection.execute(
            "UPDATE local_issues SET state = ?2, error = ?3
             WHERE id = ?1 AND state IN ('starting', 'running', 'waiting')",
            params![issue, state.word(), error],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::*;
    use tsumugi_model::{LocalIssueState as State, NewLocalIssue, TryAs};

    fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        (directory, store)
    }

    fn saved(store: &mut Store, dir: &str, title: &str) -> String {
        let new = NewLocalIssue {
            id: SessionId::new().to_string(),
            dir: dir.into(),
            title: title.into(),
            body: "Try this several ways.".into(),
        };
        let id = new.id.clone();
        store.add_local_issue(new).unwrap();
        id
    }

    fn tries(profiles: &[&str]) -> Vec<TryAs> {
        profiles
            .iter()
            .map(|word| TryAs::parse(word).unwrap())
            .collect()
    }

    fn session_in(store: &mut Store, cwd: &str) -> SessionId {
        store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: cwd.into(),
                cwd: cwd.into(),
                launch: Settings::default(),
                model: None,
                permissions: tsumugi_model::AgentPermissions::Default,
                started_ms: 1,
                name_given: true,
                code: None,
            })
            .unwrap()
            .0
    }

    fn standing(record: SessionId, state: tsumugi_model::State) -> tsumugi_model::Session {
        let mut session = tsumugi_model::testing::session("attempt").build();
        session.delegation = Some(record);
        session.state = state;
        session
    }

    #[test]
    fn an_issue_is_started_as_one_attempt_for_each_try_all_from_one_commit() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");

        let (_, begun) = store
            .begin_attempts(&id, &tries(&["claude:opus", "codex", "codex"]), "abc123")
            .unwrap();

        assert_eq!(begun.state, State::Starting);
        let attempts = &begun.attempts;
        assert_eq!(
            attempts.iter().map(|a| a.number).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(attempts.iter().all(|a| a.base == "abc123"));
        assert!(attempts.iter().all(|a| a.state == State::Starting));
        assert_eq!(attempts[0].tried_as.model.as_deref(), Some("opus"));
        assert_eq!(attempts[2].tried_as.profile.as_deref(), Some("codex"));
    }

    #[test]
    fn no_attempt_at_all_or_an_issue_already_at_work_is_refused() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        assert_eq!(
            store.begin_attempts(&id, &[], "abc").unwrap_err().code,
            Code::Invalid
        );
        store
            .begin_attempts(&id, &tries(&["codex"]), "abc")
            .unwrap();
        assert_eq!(
            store
                .begin_attempts(&id, &tries(&["codex"]), "abc")
                .unwrap_err()
                .code,
            Code::Refused
        );
    }

    #[test]
    fn attempts_neither_wait_for_nor_hold_the_directory_against_the_queue() {
        let (_directory, mut store) = store();
        let busy = saved(&mut store, "/project", "busy");
        store.start_local_issue(&busy).unwrap();
        assert_eq!(store.claim_local_issues().unwrap().len(), 1);
        let tried = saved(&mut store, "/project", "tried");
        store
            .begin_attempts(&tried, &tries(&["codex", "claude"]), "abc")
            .unwrap();
        store.fail_local_issue(&busy, "done with").unwrap();
        let next = saved(&mut store, "/project", "next");
        store.start_local_issue(&next).unwrap();

        let claimed = store.claim_local_issues().unwrap();

        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, next);
    }

    #[test]
    fn the_issue_stands_by_its_attempts_as_their_sessions_go() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["claude", "codex"]), "abc")
            .unwrap();
        let first = session_in(&mut store, "/project-wt/one");
        let second = session_in(&mut store, "/project-wt/two");
        store.attach_attempt(&id, 1, first).unwrap();
        store.attach_attempt(&id, 2, second).unwrap();
        let state = |store: &Store| store.local_issue_named(&id).unwrap().1.state;

        use tsumugi_model::State as Is;
        store
            .follow_local_issues(&[standing(first, Is::Working), standing(second, Is::Done)])
            .unwrap();
        assert_eq!(state(&store), State::Running);
        store
            .follow_local_issues(&[
                standing(first, Is::Waiting(tsumugi_model::Wait::Permission)),
                standing(second, Is::Done),
            ])
            .unwrap();
        assert_eq!(state(&store), State::Waiting);
        store
            .follow_local_issues(&[standing(first, Is::Done), standing(second, Is::Done)])
            .unwrap();
        let done = store.local_issue_named(&id).unwrap().1;
        assert_eq!(done.state, State::Done);
        assert_eq!(
            done.attempts[1].worktree.as_deref(),
            Some(Path::new("/project-wt/two"))
        );
    }

    #[test]
    fn an_attempt_whose_session_is_ended_after_its_work_is_done() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["claude"]), "abc")
            .unwrap();
        let record = session_in(&mut store, "/project-wt/one");
        store.attach_attempt(&id, 1, record).unwrap();

        use tsumugi_model::State as Is;
        store
            .follow_local_issues(&[standing(record, Is::Working)])
            .unwrap();
        store
            .follow_local_issues(&[standing(record, Is::Waiting(tsumugi_model::Wait::Input))])
            .unwrap();
        store.end(record).unwrap();
        store.follow_local_issues(&[]).unwrap();

        let issue = store.local_issue_named(&id).unwrap().1;
        assert_eq!(issue.attempts[0].state, State::Done);
        assert_eq!(issue.state, State::Done);
    }

    #[test]
    fn an_attempt_that_fails_fails_alone_and_the_issue_only_once_all_have() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["claude", "codex"]), "abc")
            .unwrap();
        let state = |store: &Store| store.local_issue_named(&id).unwrap().1;

        store.fail_attempt(&id, 1, "no worktree").unwrap();
        let one_failed = state(&store);
        assert_eq!(one_failed.state, State::Starting);
        assert_eq!(one_failed.attempts[0].state, State::Failed);
        assert_eq!(one_failed.attempts[0].error.as_deref(), Some("no worktree"));

        store.fail_attempt(&id, 2, "agent unavailable").unwrap();
        let all_failed = state(&store);
        assert_eq!(all_failed.state, State::Failed);
        assert!(all_failed.error.is_some());
    }

    #[test]
    fn reopening_keeps_the_attempts_and_starting_again_lists_only_the_new_ones() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["claude", "codex"]), "abc")
            .unwrap();
        store.fail_attempt(&id, 1, "x").unwrap();
        store.fail_attempt(&id, 2, "y").unwrap();
        store.reopen_local_issue(&id).unwrap();
        assert_eq!(store.local_issue_named(&id).unwrap().1.attempts.len(), 2);

        let (_, again) = store.begin_attempts(&id, &tries(&["pi"]), "def").unwrap();

        assert_eq!(again.attempts.len(), 1);
        assert_eq!(again.attempts[0].base, "def");
    }

    #[test]
    fn starting_an_issue_where_it_is_lets_its_old_attempts_go() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["codex"]), "abc")
            .unwrap();
        store.fail_attempt(&id, 1, "x").unwrap();
        store.retry_local_issue(&id).unwrap();
        assert!(store.local_issue_named(&id).unwrap().1.attempts.is_empty());
    }

    #[test]
    fn the_issue_an_attempt_works_on_is_found_by_its_worktree() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["claude", "codex"]), "abc")
            .unwrap();
        let second = session_in(&mut store, "/project-wt/two");
        store.attach_attempt(&id, 2, second).unwrap();

        let (_, found) = store
            .local_issue_worked_in(Path::new("/project-wt/two"))
            .unwrap()
            .unwrap();

        assert_eq!(found.id, id);
    }

    #[test]
    fn deleting_an_issue_takes_its_attempts_but_not_their_sessions() {
        let (_directory, mut store) = store();
        let id = saved(&mut store, "/project", "compare");
        store
            .begin_attempts(&id, &tries(&["codex"]), "abc")
            .unwrap();
        let record = session_in(&mut store, "/project-wt/one");
        store.attach_attempt(&id, 1, record).unwrap();
        store.fail_attempt(&id, 1, "x").unwrap();

        store.delete_local_issue(&id).unwrap();

        assert!(store.attempts_of(&id).unwrap().is_empty());
        assert!(store.known(record).is_ok());
    }
}
