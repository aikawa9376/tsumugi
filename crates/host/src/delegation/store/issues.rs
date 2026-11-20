//! The environment's durable queue, beside the sessions it starts.

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use tsumugi_model::{LocalIssue, LocalIssueState, NewLocalIssue, Session, SessionId, State};

use super::{Code, Error, Result, Store};

impl Store {
    pub fn add_local_issue(&mut self, new: NewLocalIssue) -> Result<()> {
        if new.id.parse::<SessionId>().is_err()
            || new.title.trim().is_empty()
            || new.title.len() > 256
            || new.body.len() > 65536
            || !new.dir.is_absolute()
        {
            return Err(Error::new(
                Code::Invalid,
                "An issue needs an absolute directory, a title of 1–256 bytes and a description of at most 65536 bytes",
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT dir, title, body FROM local_issues WHERE id = ?1",
                [&new.id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let dir = new.dir.to_string_lossy();
        if let Some((kept_dir, title, body)) = existing {
            if kept_dir != dir || title != new.title.trim() || body != new.body {
                return Err(Error::new(
                    Code::Invalid,
                    "This issue identity already holds a different request",
                ));
            }
            return Ok(());
        }
        transaction.execute(
            "INSERT INTO local_issues (id, dir, title, body, state) VALUES (?1, ?2, ?3, ?4, 'open')",
            params![new.id, dir, new.title.trim(), new.body],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn local_issues(&self) -> Result<Vec<LocalIssue>> {
        let mut query = self.connection.prepare(
            "SELECT i.id, i.dir, i.title, i.body, i.state, i.session, s.terminal, i.error
             FROM local_issues i LEFT JOIN sessions s ON s.id = i.session ORDER BY i.seq",
        )?;
        let rows = query.query_map([], |row| {
            let state: String = row.get(4)?;
            let record: Option<String> = row.get(5)?;
            let parse_error = |error: Box<dyn std::error::Error + Send + Sync>| {
                rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, error)
            };
            Ok(LocalIssue {
                id: row.get(0)?,
                dir: std::path::PathBuf::from(row.get::<_, String>(1)?),
                title: row.get(2)?,
                body: row.get(3)?,
                state: serde_json::from_value(serde_json::Value::String(state))
                    .map_err(|e| parse_error(Box::new(e)))?,
                record: record
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|e| parse_error(Box::new(e)))?,
                session: row.get(6)?,
                error: row.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Reserve one request per directory before any program is started.
    pub fn claim_local_issues(&mut self) -> Result<Vec<LocalIssue>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: Vec<String> = {
            let mut query = transaction.prepare(
                "SELECT i.id FROM local_issues i WHERE i.state = 'queued'
                 AND NOT EXISTS (SELECT 1 FROM local_issues active WHERE active.dir = i.dir
                     AND active.state IN ('starting', 'running', 'waiting'))
                 AND NOT EXISTS (SELECT 1 FROM local_issues earlier WHERE earlier.dir = i.dir
                     AND earlier.state = 'queued' AND earlier.seq < i.seq) ORDER BY i.seq",
            )?;
            query
                .query_map([], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        for id in &ids {
            transaction.execute(
                "UPDATE local_issues SET state = 'starting' WHERE id = ?1",
                [id],
            )?;
        }
        transaction.commit()?;
        Ok(self
            .local_issues()?
            .into_iter()
            .filter(|i| ids.contains(&i.id))
            .collect())
    }

    /// Link the durable record before its program can run.
    pub fn attach_local_issue(&mut self, issue: &str, session: SessionId) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE local_issues SET session = ?2 WHERE id = ?1 AND state = 'starting' AND session IS NULL",
            params![issue, session.to_string()],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                "The issue is no longer waiting for its session",
            ));
        }
        Ok(())
    }

    pub fn fail_local_issue(&mut self, id: &str, error: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE local_issues SET state = 'failed', error = ?2 WHERE id = ?1",
            params![id, error],
        )?;
        Ok(())
    }

    pub fn cancel_local_issue(&mut self, id: &str) -> Result<()> {
        self.change_local_issue(id, "cancelled", "state IN ('open', 'queued')")
    }

    pub fn start_local_issue(&mut self, id: &str) -> Result<()> {
        self.change_local_issue(id, "queued", "state = 'open'")
    }

    pub fn retry_local_issue(&mut self, id: &str) -> Result<()> {
        self.change_local_issue(id, "queued", "state IN ('failed', 'cancelled')")
    }

    pub fn close_local_issue(&mut self, id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE local_issues SET state = 'closed' WHERE id = ?1 AND state IN ('open', 'queued', 'failed', 'cancelled')",
            [id],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                "The issue cannot be closed in its current state",
            ));
        }
        Ok(())
    }

    pub fn reopen_local_issue(&mut self, id: &str) -> Result<()> {
        self.change_local_issue(
            id,
            "open",
            "state IN ('done', 'closed', 'failed', 'cancelled')",
        )
    }

    pub fn delete_local_issue(&mut self, id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "DELETE FROM local_issues WHERE id = ?1 AND state NOT IN ('starting', 'running', 'waiting')",
            [id],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                "The issue cannot be deleted while it is active or absent",
            ));
        }
        Ok(())
    }

    fn change_local_issue(&mut self, id: &str, state: &str, allowed: &str) -> Result<()> {
        let changed = self.connection.execute(
            &format!("UPDATE local_issues SET state = ?2, session = NULL, error = NULL, worked = 0 WHERE id = ?1 AND {allowed}"),
            params![id, state],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                "The issue cannot be changed in its current state",
            ));
        }
        Ok(())
    }

    /// Follow only a successful terminal listing; an unavailable host proves nothing.
    pub fn follow_local_issues(&mut self, sessions: &[Session]) -> Result<()> {
        for issue in self.local_issues()? {
            if !matches!(
                issue.state,
                LocalIssueState::Starting | LocalIssueState::Running | LocalIssueState::Waiting
            ) {
                continue;
            }
            let found = issue
                .record
                .and_then(|id| sessions.iter().find(|s| s.delegation == Some(id)));
            let worked: bool = self.connection.query_row(
                "SELECT worked FROM local_issues WHERE id = ?1",
                [&issue.id],
                |row| row.get(0),
            )?;
            let working = found
                .is_some_and(|session| matches!(session.state, State::Working | State::Background));
            let state = match found.map(|s| &s.state) {
                Some(State::Done) => LocalIssueState::Done,
                Some(State::Idle) if worked => LocalIssueState::Done,
                Some(State::Idle | State::Starting) => LocalIssueState::Starting,
                Some(State::Waiting(_)) => LocalIssueState::Waiting,
                Some(State::Failed) | None => LocalIssueState::Failed,
                Some(_) => LocalIssueState::Running,
            };
            let error = match found {
                None => Some(
                    "The session start or execution was interrupted; inspect the work before retrying",
                ),
                Some(_) if state == LocalIssueState::Failed => Some("The agent reported a failure"),
                _ => None,
            };
            self.connection.execute(
                "UPDATE local_issues SET state = ?2, error = ?3, worked = ?4 WHERE id = ?1",
                params![issue.id, state.word(), error, worked || working],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use tsumugi_model::{LocalIssueState as State, NewLocalIssue};

    fn issue(dir: &str, title: &str) -> NewLocalIssue {
        NewLocalIssue {
            id: SessionId::new().to_string(),
            dir: std::env::temp_dir().join(dir.trim_start_matches('/')),
            title: title.into(),
            body: "Please implement this request.".into(),
        }
    }

    fn queue(store: &mut Store, new: NewLocalIssue) {
        let id = new.id.clone();
        store.add_local_issue(new).unwrap();
        store.start_local_issue(&id).unwrap();
    }

    #[test]
    fn close_reopen_and_delete_respect_issue_states_and_never_start_work() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        for state in [
            "open",
            "queued",
            "starting",
            "running",
            "waiting",
            "done",
            "failed",
            "cancelled",
            "closed",
        ] {
            for action in ["close", "reopen", "delete"] {
                let new = issue("/project", "kept request");
                store.add_local_issue(new.clone()).unwrap();
                store.connection.execute(
                    "UPDATE local_issues SET state = ?2, error = 'old error', worked = 1 WHERE id = ?1",
                    params![new.id, state],
                ).unwrap();
                let allowed = match action {
                    "close" => matches!(state, "open" | "queued" | "failed" | "cancelled"),
                    "reopen" => matches!(state, "done" | "failed" | "cancelled" | "closed"),
                    _ => !matches!(state, "starting" | "running" | "waiting"),
                };
                let value: State = serde_json::from_value(serde_json::json!(state)).unwrap();
                let offered = match action {
                    "close" => value.can_close(),
                    "reopen" => value.can_reopen(),
                    _ => value.can_delete(),
                };
                assert_eq!(offered, allowed, "{action} offered from {state}");
                let result = match action {
                    "close" => store.close_local_issue(&new.id),
                    "reopen" => store.reopen_local_issue(&new.id),
                    _ => store.delete_local_issue(&new.id),
                };
                assert_eq!(result.is_ok(), allowed, "{action} from {state}");
                let found = store
                    .local_issues()
                    .unwrap()
                    .into_iter()
                    .find(|i| i.id == new.id);
                if allowed && action == "delete" {
                    assert!(found.is_none());
                } else {
                    let found = found.unwrap();
                    assert_eq!(found.title, new.title);
                    assert_eq!(found.body, new.body);
                    assert_eq!(found.dir, new.dir);
                    let expected = if !allowed {
                        state
                    } else if action == "close" {
                        "closed"
                    } else {
                        "open"
                    };
                    assert_eq!(found.state.word(), expected);
                    if allowed && action == "reopen" {
                        assert!(found.error.is_none());
                        assert!(found.record.is_none());
                    }
                }
                store
                    .connection
                    .execute("DELETE FROM local_issues WHERE id = ?1", [&new.id])
                    .unwrap();
            }
        }
        assert!(store.claim_local_issues().unwrap().is_empty());
        assert!(store.close_local_issue("missing").is_err());
        assert!(store.reopen_local_issue("missing").is_err());
        assert!(store.delete_local_issue("missing").is_err());
    }

    #[test]
    fn saved_issues_wait_for_start_even_after_reopening_and_do_not_block_selected_work() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let first = issue("/project", "research notes");
        let second = issue("/project", "selected work");
        store.add_local_issue(first.clone()).unwrap();
        store.add_local_issue(second.clone()).unwrap();
        assert!(store.claim_local_issues().unwrap().is_empty());
        drop(store);
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.local_issues().unwrap()[0].state, State::Open);
        store.start_local_issue(&second.id).unwrap();
        assert!(store.start_local_issue(&second.id).is_err());
        assert_eq!(store.claim_local_issues().unwrap()[0].id, second.id);
        assert_eq!(store.local_issues().unwrap()[0].state, State::Open);
        assert!(store.start_local_issue(&second.id).is_err());
        store.cancel_local_issue(&first.id).unwrap();
        store.retry_local_issue(&first.id).unwrap();
    }

    #[test]
    fn upgrading_keeps_previously_authorized_queued_issues_and_new_issues_wait_for_start() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let old = issue("/project", "already queued");
        queue(&mut store, old.clone());
        store
            .connection
            .pragma_update(None, "user_version", 9)
            .unwrap();
        drop(store);
        let mut store = Store::open(&path).unwrap();
        store
            .add_local_issue(issue("/project", "new request"))
            .unwrap();
        let all = store.local_issues().unwrap();
        assert_eq!(all[0].state, State::Queued);
        assert_eq!(all[1].state, State::Open);
        assert_eq!(store.claim_local_issues().unwrap()[0].id, old.id);
    }

    #[test]
    fn an_issue_survives_reopening_and_duplicate_submission_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let new = issue("/project", "日本語の依頼");
        store.add_local_issue(new.clone()).unwrap();
        store.add_local_issue(new.clone()).unwrap();
        let mut changed = new;
        changed.body = "a different request".into();
        assert!(store.add_local_issue(changed).is_err());
        drop(store);
        let all = Store::open(&path).unwrap().local_issues().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "日本語の依頼");
        assert_eq!(all[0].state, State::Open);
    }

    #[test]
    fn claiming_is_fifo_per_directory_and_other_directories_can_start() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let first = issue("/project", "first");
        let second = issue("/project", "second");
        let other = issue("/other", "other");
        for new in [first.clone(), second.clone(), other.clone()] {
            queue(&mut store, new);
        }
        let claimed = store.claim_local_issues().unwrap();
        assert_eq!(
            claimed.iter().map(|i| &i.id).collect::<Vec<_>>(),
            [&first.id, &other.id]
        );
        assert!(store.claim_local_issues().unwrap().is_empty());
        store
            .fail_local_issue(&first.id, "agent unavailable")
            .unwrap();
        assert_eq!(store.claim_local_issues().unwrap()[0].id, second.id);
    }

    #[test]
    fn empty_requests_are_refused_and_only_queued_issues_can_be_cancelled() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        assert!(store.add_local_issue(issue("/project", " \n ")).is_err());
        let queued = issue("/project", "queued");
        queue(&mut store, queued.clone());
        store.cancel_local_issue(&queued.id).unwrap();
        assert!(store.claim_local_issues().unwrap().is_empty());
        store.retry_local_issue(&queued.id).unwrap();
        assert_eq!(store.claim_local_issues().unwrap().len(), 1);
        assert!(store.cancel_local_issue(&queued.id).is_err());
        assert!(store.retry_local_issue(&queued.id).is_err());
    }

    #[test]
    fn a_start_interrupted_by_host_replacement_is_not_replayed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let new = issue("/project", "interrupted");
        queue(&mut store, new.clone());
        store.claim_local_issues().unwrap();
        drop(store);
        let mut store = Store::open(&path).unwrap();
        store.follow_local_issues(&[]).unwrap();
        let all = store.local_issues().unwrap();
        assert_eq!(all[0].state, State::Failed);
        assert!(all[0].error.is_some());
        assert!(store.claim_local_issues().unwrap().is_empty());
    }

    #[test]
    fn a_waiting_session_holds_its_directory_until_its_turn_finishes() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let first = issue("/project", "first");
        let second = issue("/project", "second");
        queue(&mut store, first.clone());
        queue(&mut store, second.clone());
        store.claim_local_issues().unwrap();
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: "first".into(),
                cwd: "/project".into(),
                launch: Settings::default(),
                model: None,
                permissions: tsumugi_model::AgentPermissions::Default,
                started_ms: 1,
                name_given: true,
                code: None,
            })
            .unwrap()
            .0;
        store.attach_local_issue(&first.id, record).unwrap();
        store.attach(record, 15, None).unwrap();
        let mut session = tsumugi_model::testing::session("f")
            .waiting(tsumugi_model::Wait::Permission)
            .build();
        session.delegation = Some(record);
        store.follow_local_issues(&[session.clone()]).unwrap();
        let all = store.local_issues().unwrap();
        assert_eq!(all[0].state, State::Waiting);
        assert_eq!(
            all[0].session.as_deref(),
            Some("0000000000000000000000000000000f")
        );
        assert!(store.claim_local_issues().unwrap().is_empty());
        assert!(store.retry_local_issue(&first.id).is_err());
        session.state = tsumugi_model::State::Done;
        store.follow_local_issues(&[session.clone()]).unwrap();
        assert_eq!(store.local_issues().unwrap()[0].state, State::Done);
        assert_eq!(store.claim_local_issues().unwrap()[0].id, second.id);
        store.reopen_local_issue(&first.id).unwrap();
        store.follow_local_issues(&[session]).unwrap();
        let reopened = &store.local_issues().unwrap()[0];
        assert_eq!(reopened.state, State::Open);
        assert!(reopened.record.is_none());
        assert!(reopened.session.is_none());
        assert!(store.known(record).is_ok());
        assert!(store.claim_local_issues().unwrap().is_empty());
        store.close_local_issue(&first.id).unwrap();
        store.delete_local_issue(&first.id).unwrap();
        assert!(store.known(record).is_ok());
        store.fail_local_issue(&second.id, "finished").unwrap();
        store.delete_local_issue(&second.id).unwrap();
        assert!(store.local_issues().unwrap().is_empty());
    }

    #[test]
    fn a_second_store_cannot_claim_the_same_issue_and_environments_keep_separate_queues() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("one.sqlite3");
        let mut one = Store::open(&path).unwrap();
        let mut also_one = Store::open(&path).unwrap();
        queue(&mut one, issue("/project", "one"));
        assert_eq!(one.claim_local_issues().unwrap().len(), 1);
        assert!(also_one.claim_local_issues().unwrap().is_empty());
        let mut other = Store::open(&directory.path().join("other.sqlite3")).unwrap();
        queue(&mut other, issue("/project", "other"));
        assert_eq!(other.claim_local_issues().unwrap().len(), 1);
    }

    #[test]
    fn an_existing_database_gains_an_empty_queue_without_losing_sessions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: "existing".into(),
                cwd: "/project".into(),
                launch: Settings::default(),
                model: None,
                permissions: tsumugi_model::AgentPermissions::Default,
                started_ms: 1,
                name_given: true,
                code: None,
            })
            .unwrap()
            .0;
        store
            .connection
            .execute_batch("DROP TABLE local_issues; PRAGMA user_version = 8;")
            .unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.known(record).unwrap().name, "existing");
        assert!(store.local_issues().unwrap().is_empty());
    }

    #[test]
    fn an_idle_screen_finishes_only_after_work_was_seen_and_remembers_that_across_restarts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let new = issue("/project", "screen agent");
        queue(&mut store, new.clone());
        store.claim_local_issues().unwrap();
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: "screen agent".into(),
                cwd: "/project".into(),
                launch: Settings::default(),
                model: None,
                permissions: tsumugi_model::AgentPermissions::Default,
                started_ms: 1,
                name_given: true,
                code: None,
            })
            .unwrap()
            .0;
        store.attach_local_issue(&new.id, record).unwrap();
        let mut session = tsumugi_model::testing::session("screen").build();
        session.delegation = Some(record);
        store.follow_local_issues(&[session.clone()]).unwrap();
        assert_ne!(store.local_issues().unwrap()[0].state, State::Done);
        session.state = tsumugi_model::State::Working;
        store.follow_local_issues(&[session.clone()]).unwrap();
        drop(store);
        let mut store = Store::open(&path).unwrap();
        session.state = tsumugi_model::State::Idle;
        store.follow_local_issues(&[session]).unwrap();
        assert_eq!(store.local_issues().unwrap()[0].state, State::Done);
    }
}
