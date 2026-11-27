//! The environment's durable queue, beside the sessions it starts.

use std::path::Path;

use rusqlite::{OptionalExtension, TransactionBehavior, params};
use tsumugi_model::{
    LocalIssue, LocalIssueAction, LocalIssueState, NewLocalIssue, Session, SessionId, State,
};

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
        Ok(self
            .numbered_local_issues()?
            .into_iter()
            .map(|(_, issue)| issue)
            .collect())
    }

    /// Every issue with its number: the short, stable one a person types,
    /// which a deletion never hands on to another issue.
    pub fn numbered_local_issues(&self) -> Result<Vec<(u64, LocalIssue)>> {
        let mut query = self.connection.prepare(
            "SELECT i.id, i.dir, i.title, i.body, i.state, i.session, s.terminal, i.error, i.seq, o.schedule
             FROM local_issues i LEFT JOIN sessions s ON s.id = i.session
             LEFT JOIN schedule_issues o ON o.issue = i.id ORDER BY i.seq",
        )?;
        let rows = query.query_map([], |row| {
            let state: String = row.get(4)?;
            let record: Option<String> = row.get(5)?;
            let parse_error = |error: Box<dyn std::error::Error + Send + Sync>| {
                rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, error)
            };
            let issue = LocalIssue {
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
                schedule: row.get(9)?,
                attempts: Vec::new(),
            };
            Ok((row.get::<_, i64>(8)?.unsigned_abs(), issue))
        })?;
        let mut issues = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        for (_, issue) in &mut issues {
            issue.attempts = self.attempts_of(&issue.id)?;
        }
        Ok(issues)
    }

    /// The issue `selector` names: its number, or the start of its id when
    /// no issue has that number. One that names none, or several, is
    /// refused.
    pub fn local_issue_named(&self, selector: &str) -> Result<(u64, LocalIssue)> {
        let selector = selector.trim().trim_start_matches('#');
        if selector.is_empty() {
            return Err(Error::new(Code::Invalid, "Name an issue by its number"));
        }
        let all = self.numbered_local_issues()?;
        if let Ok(number) = selector.parse::<u64>()
            && let Some(found) = all.iter().find(|(n, _)| *n == number)
        {
            return Ok(found.clone());
        }
        let prefix = selector.to_ascii_lowercase();
        let mut matching: Vec<_> = all
            .into_iter()
            .filter(|(_, issue)| issue.id.starts_with(&prefix))
            .collect();
        match matching.len() {
            0 => Err(Error::new(
                Code::NotFound,
                format!("No local issue is named {selector}"),
            )),
            1 => Ok(matching.remove(0)),
            _ => {
                let numbers: Vec<String> = matching.iter().map(|(n, _)| format!("#{n}")).collect();
                Err(Error::new(
                    Code::Invalid,
                    format!(
                        "{selector} names several issues ({}); use more of the id or a number",
                        numbers.join(", ")
                    ),
                ))
            }
        }
    }

    /// Do `action` to the issue `selector` names and hand it back as it is
    /// then, or as it was when it is deleted.
    pub fn act_on_local_issue(
        &mut self,
        selector: &str,
        action: LocalIssueAction,
    ) -> Result<(u64, LocalIssue)> {
        let (number, issue) = self.local_issue_named(selector)?;
        match action {
            LocalIssueAction::Start => self.start_local_issue(&issue.id)?,
            LocalIssueAction::Cancel => self.cancel_local_issue(&issue.id)?,
            LocalIssueAction::Retry => self.retry_local_issue(&issue.id)?,
            LocalIssueAction::Close => self.close_local_issue(&issue.id)?,
            LocalIssueAction::Reopen => self.reopen_local_issue(&issue.id)?,
            LocalIssueAction::Delete => {
                self.delete_local_issue(&issue.id)?;
                return Ok((number, issue));
            }
        }
        self.local_issue_named(&issue.id)
    }

    /// Reserve one request per directory before any program is started. An
    /// issue tried as attempts works in worktrees of its own and holds no
    /// directory.
    pub fn claim_local_issues(&mut self) -> Result<Vec<LocalIssue>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: Vec<String> = {
            let mut query = transaction.prepare(
                "SELECT i.id FROM local_issues i WHERE i.state = 'queued'
                 AND NOT EXISTS (SELECT 1 FROM local_issues active WHERE active.dir = i.dir
                     AND active.state IN ('starting', 'running', 'waiting')
                     AND NOT EXISTS (SELECT 1 FROM issue_attempts a WHERE a.issue = active.id))
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

    /// Take up the issue `selector` names for a session another session
    /// starts for it, as a claim does for the queue's: it is `starting` and
    /// holds its directory against the queue, whatever else works there. Only
    /// an issue that waits can be taken up.
    pub fn begin_local_issue(&mut self, selector: &str) -> Result<(u64, LocalIssue)> {
        let (number, issue) = self.local_issue_named(selector)?;
        let changed = self.connection.execute(
            "UPDATE local_issues SET state = 'starting', session = NULL, error = NULL, worked = 0
             WHERE id = ?1 AND state IN ('open', 'queued', 'failed', 'cancelled')",
            [&issue.id],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                format!(
                    "Issue #{number} is {}: only an open, queued, failed or cancelled issue can be worked on",
                    issue.state.word()
                ),
            ));
        }
        self.forget_attempts(&issue.id)?;
        self.local_issue_named(&issue.id)
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
        self.change_local_issue(id, "queued", "state = 'open'")?;
        self.forget_attempts(id)
    }

    pub fn retry_local_issue(&mut self, id: &str) -> Result<()> {
        self.change_local_issue(id, "queued", "state IN ('failed', 'cancelled')")?;
        self.forget_attempts(id)
    }

    /// Let go of the attempts of an issue now worked where it is; their
    /// sessions and worktrees stay.
    fn forget_attempts(&mut self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM issue_attempts WHERE issue = ?1", [id])?;
        Ok(())
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

    /// Close the issue whose work was merged into the main checkout, from
    /// whatever state it is in: its session is done with once its branch is
    /// in, even if it still waits or has not yet been seen to end.
    pub fn close_merged_local_issue(&mut self, id: &str) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE local_issues SET state = 'closed', error = NULL WHERE id = ?1 AND state != 'closed'",
            [id],
        )?;
        if changed != 1 {
            return Err(Error::new(Code::Refused, "The issue is closed already"));
        }
        Ok(())
    }

    /// The issue, not closed yet, that a session working in `dir` or below
    /// it was started for.
    pub fn local_issue_worked_in(&self, dir: &Path) -> Result<Option<(u64, LocalIssue)>> {
        let sessions = self.sessions()?;
        Ok(self
            .numbered_local_issues()?
            .into_iter()
            .find(|(_, issue)| {
                let works_here = |record: &SessionId| {
                    sessions
                        .iter()
                        .any(|s| s.id == *record && s.cwd.starts_with(dir))
                };
                issue.state != LocalIssueState::Closed
                    && (issue.record.as_ref().is_some_and(works_here)
                        || issue
                            .attempts
                            .iter()
                            .any(|a| a.record.as_ref().is_some_and(works_here)))
            }))
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
            if !issue.attempts.is_empty() {
                self.follow_attempts(&issue.id, sessions)?;
                continue;
            }
            let worked: bool = self.connection.query_row(
                "SELECT worked FROM local_issues WHERE id = ?1",
                [&issue.id],
                |row| row.get(0),
            )?;
            let left = self.left(issue.record)?;
            let (state, error, worked) =
                followed(issue.record, sessions, worked, issue.state, left);
            self.connection.execute(
                "UPDATE local_issues SET state = ?2, error = ?3, worked = ?4 WHERE id = ?1",
                params![issue.id, state.word(), error, worked],
            )?;
        }
        Ok(())
    }

    /// How the durable record of the session `record` says it left, should
    /// the listing no longer show it.
    pub(super) fn left(&self, record: Option<SessionId>) -> Result<Left> {
        let Some(id) = record else {
            return Ok(Left::Unrecorded);
        };
        Ok(match super::session(&self.connection, id)? {
            None => Left::Unrecorded,
            Some(session) if session.interrupted => Left::Lost,
            Some(session) => match session.rest {
                Some(tsumugi_model::Rest::Hibernated) => Left::Not,
                Some(tsumugi_model::Rest::Exited) => Left::Lost,
                None if session.ended => Left::Ended,
                None => Left::Not,
            },
        })
    }
}

/// How a session's record says it left, for one the listing does not show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Left {
    /// No session was recorded: its start was cut short.
    Unrecorded,
    /// It has not left by its record: its end is not recorded yet, or it
    /// sleeps.
    Not,
    /// It was ended from Tsumugi, by the user or by itself.
    Ended,
    /// It went with its terminal host, or its program exited on its own.
    Lost,
}

/// How the work of the session `record` stands among `sessions`, with
/// whether it was ever seen at work: an idle screen ends the work only once
/// it was. A session the listing no longer shows is judged by how it `left`;
/// one whose end is not recorded yet leaves the work standing as `current`.
pub(super) fn followed(
    record: Option<SessionId>,
    sessions: &[Session],
    worked: bool,
    current: LocalIssueState,
    left: Left,
) -> (LocalIssueState, Option<&'static str>, bool) {
    let found = record.and_then(|id| sessions.iter().find(|s| s.delegation == Some(id)));
    let working =
        found.is_some_and(|session| matches!(session.state, State::Working | State::Background));
    let state = match found.map(|s| &s.state) {
        Some(State::Done) => LocalIssueState::Done,
        Some(State::Idle) if worked => LocalIssueState::Done,
        Some(State::Idle | State::Starting) => LocalIssueState::Starting,
        Some(State::Waiting(_)) => LocalIssueState::Waiting,
        Some(State::Failed) => LocalIssueState::Failed,
        Some(_) => LocalIssueState::Running,
        // Ending a session after its work is how its work is finished: the
        // agent asks whether to close, waiting on the user, and then closes.
        None => match left {
            Left::Not => current,
            Left::Ended if worked => LocalIssueState::Done,
            Left::Ended | Left::Lost | Left::Unrecorded => LocalIssueState::Failed,
        },
    };
    let error = match found {
        None if state == LocalIssueState::Failed => {
            Some("The session start or execution was interrupted; inspect the work before retrying")
        }
        Some(_) if state == LocalIssueState::Failed => Some("The agent reported a failure"),
        _ => None,
    };
    (state, error, worked || working)
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use tsumugi_model::{LocalIssueAction, LocalIssueState as State, NewLocalIssue};

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

    #[test]
    fn issues_have_short_numbers_that_a_deletion_does_not_hand_on() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let [one, two] = ["one", "two"].map(|title| issue("/project", title));
        store.add_local_issue(one.clone()).unwrap();
        store.add_local_issue(two.clone()).unwrap();
        store.delete_local_issue(&two.id).unwrap();
        let three = issue("/project", "three");
        store.add_local_issue(three.clone()).unwrap();
        let numbers: Vec<_> = store
            .numbered_local_issues()
            .unwrap()
            .into_iter()
            .map(|(number, issue)| (number, issue.id))
            .collect();
        assert_eq!(numbers, [(1, one.id), (3, three.id)]);
    }

    fn named(store: &Store, selector: &str) -> std::result::Result<String, Error> {
        store
            .local_issue_named(selector)
            .map(|(_, issue)| issue.title)
    }

    #[test]
    fn an_issue_is_named_by_its_number_or_by_a_unique_prefix_of_its_id() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let mut one = issue("/project", "one");
        let mut two = issue("/project", "two");
        one.id = "ab12".to_owned() + &"0".repeat(28);
        two.id = "ab34".to_owned() + &"0".repeat(28);
        for new in [one.clone(), two.clone()] {
            store.add_local_issue(new).unwrap();
        }
        assert_eq!(named(&store, "2").unwrap(), "two");
        assert_eq!(named(&store, "#1").unwrap(), "one");
        assert_eq!(named(&store, "ab3").unwrap(), "two");
        assert_eq!(named(&store, "AB12").unwrap(), "one");
        assert_eq!(named(&store, &two.id).unwrap(), "two");
        let ambiguous = named(&store, "ab").unwrap_err();
        assert_eq!(ambiguous.code, Code::Invalid);
        assert!(ambiguous.message.contains("#1") && ambiguous.message.contains("#2"));
        for selector in ["9", "ff", "", "  ", "ab12/"] {
            let error = named(&store, selector).unwrap_err();
            assert!(
                matches!(error.code, Code::NotFound | Code::Invalid),
                "{selector:?}"
            );
        }
        assert_eq!(named(&store, "9").unwrap_err().code, Code::NotFound);
    }

    #[test]
    fn a_number_wins_over_an_id_that_starts_with_the_same_digits() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let mut one = issue("/project", "one");
        let mut two = issue("/project", "two");
        one.id = "2".to_owned() + &"0".repeat(31);
        two.id = "1".to_owned() + &"0".repeat(31);
        store.add_local_issue(one).unwrap();
        store.add_local_issue(two).unwrap();
        assert_eq!(named(&store, "1").unwrap(), "one");
        assert_eq!(named(&store, "10").unwrap(), "two");
    }

    #[test]
    fn an_action_changes_the_issue_named_and_hands_it_back() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        store.add_local_issue(issue("/project", "kept")).unwrap();
        let mut act = |action| {
            store
                .act_on_local_issue("1", action)
                .map(|(_, issue)| issue.state)
        };
        assert_eq!(act(LocalIssueAction::Start).unwrap(), State::Queued);
        assert_eq!(act(LocalIssueAction::Cancel).unwrap(), State::Cancelled);
        assert_eq!(act(LocalIssueAction::Retry).unwrap(), State::Queued);
        assert_eq!(act(LocalIssueAction::Close).unwrap(), State::Closed);
        assert_eq!(act(LocalIssueAction::Reopen).unwrap(), State::Open);
        assert_eq!(act(LocalIssueAction::Delete).unwrap(), State::Open);
        assert_eq!(
            store
                .act_on_local_issue("1", LocalIssueAction::Close)
                .unwrap_err()
                .code,
            Code::NotFound
        );
    }

    #[test]
    fn a_session_takes_up_an_issue_that_is_open_queued_failed_or_cancelled_and_no_other() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let new = issue("/project", "taken up");
        let id = new.id.clone();
        store.add_local_issue(new).unwrap();
        let stand = |store: &mut Store, state: &str| {
            store
                .connection
                .execute(
                    "UPDATE local_issues SET state = ?2 WHERE id = ?1",
                    params![id, state],
                )
                .unwrap();
        };
        for state in ["open", "queued", "failed", "cancelled"] {
            stand(&mut store, state);
            let (number, taken) = store.begin_local_issue("1").unwrap();
            assert_eq!((number, taken.state), (1, State::Starting), "{state}");
        }
        for state in ["starting", "running", "waiting", "done", "closed"] {
            stand(&mut store, state);
            let refused = store.begin_local_issue("1").unwrap_err();
            assert_eq!(refused.code, Code::Refused, "{state}");
        }
        assert_eq!(
            store.begin_local_issue("9").unwrap_err().code,
            Code::NotFound
        );
    }

    #[test]
    fn an_issue_taken_up_holds_its_directory_against_the_queue_and_keeps_its_session_across_restarts()
     {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let (taken, waiting) = (issue("/project", "taken"), issue("/project", "waiting"));
        store.add_local_issue(taken.clone()).unwrap();
        queue(&mut store, waiting.clone());
        let (_, begun) = store.begin_local_issue("1").unwrap();
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: "taken".into(),
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
        store.attach_local_issue(&begun.id, record).unwrap();
        store.attach(record, 15, None).unwrap();

        assert!(store.claim_local_issues().unwrap().is_empty());
        drop(store);
        let store = Store::open(&path).unwrap();
        let kept = &store.local_issues().unwrap()[0];
        assert_eq!(kept.record, Some(record));
        assert_eq!(
            kept.session.as_deref(),
            Some("0000000000000000000000000000000f")
        );
    }

    fn worked_in(store: &mut Store, title: &str, dir: &str) -> (String, SessionId) {
        let new = issue(dir, title);
        let id = new.id.clone();
        store.add_local_issue(new).unwrap();
        let (_, begun) = store.begin_local_issue(&id).unwrap();
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: title.into(),
                cwd: dir.into(),
                launch: Settings::default(),
                model: None,
                permissions: tsumugi_model::AgentPermissions::Default,
                started_ms: 1,
                name_given: true,
                code: None,
            })
            .unwrap()
            .0;
        store.attach_local_issue(&begun.id, record).unwrap();
        (id, record)
    }

    #[test]
    fn the_issue_a_session_in_a_worktree_works_on_is_found_by_the_worktree() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        worked_in(&mut store, "elsewhere", "/project-wt/other");
        worked_in(&mut store, "here", "/project-wt/here");

        let (number, found) = store
            .local_issue_worked_in(Path::new("/project-wt/here"))
            .unwrap()
            .unwrap();

        assert_eq!((number, found.title.as_str()), (2, "here"));
        assert!(
            store
                .local_issue_worked_in(Path::new("/project-wt/none"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn an_issue_closed_already_is_not_the_one_a_worktree_works_on() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let (id, _) = worked_in(&mut store, "done", "/project-wt/here");
        store.close_merged_local_issue(&id).unwrap();

        assert!(
            store
                .local_issue_worked_in(Path::new("/project-wt/here"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.close_merged_local_issue(&id).unwrap_err().code,
            Code::Refused
        );
    }

    #[test]
    fn a_merged_issue_is_closed_whatever_state_its_session_left_it_in() {
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
        ] {
            let (id, _) = worked_in(&mut store, state, "/project-wt/here");
            store
                .connection
                .execute(
                    "UPDATE local_issues SET state = ?2 WHERE id = ?1",
                    params![id, state],
                )
                .unwrap();

            store.close_merged_local_issue(&id).unwrap();

            let closed = store.local_issue_named(&id).unwrap().1;
            assert_eq!(closed.state, State::Closed, "{state}");
        }
    }

    #[test]
    fn an_issue_at_work_is_neither_closed_nor_deleted_by_name() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        queue(&mut store, issue("/project", "busy"));
        store.claim_local_issues().unwrap();
        for action in [LocalIssueAction::Close, LocalIssueAction::Delete] {
            let error = store.act_on_local_issue("1", action).unwrap_err();
            assert_eq!(error.code, Code::Refused, "{}", action.word());
        }
        assert_eq!(store.local_issues().unwrap()[0].state, State::Starting);
    }

    /// An issue claimed and worked by a session, which the listing last
    /// showed waiting on the user, as it is when it asks whether to close.
    fn worked_then_waiting(store: &mut Store) -> SessionId {
        let new = issue("/project", "worked");
        queue(store, new.clone());
        store.claim_local_issues().unwrap();
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: "worked".into(),
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
        let mut session = tsumugi_model::testing::session("w").working().build();
        session.delegation = Some(record);
        store.follow_local_issues(&[session.clone()]).unwrap();
        session.state = tsumugi_model::State::Waiting(tsumugi_model::Wait::Input);
        store.follow_local_issues(&[session]).unwrap();
        assert_eq!(store.local_issues().unwrap()[0].state, State::Waiting);
        record
    }

    #[test]
    fn a_session_ended_after_its_work_leaves_its_issue_done() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let record = worked_then_waiting(&mut store);
        store.end(record).unwrap();
        store.follow_local_issues(&[]).unwrap();
        let issue = &store.local_issues().unwrap()[0];
        assert_eq!(issue.state, State::Done);
        assert!(issue.error.is_none());
    }

    #[test]
    fn a_session_gone_before_its_end_is_recorded_leaves_its_issue_as_it_stood() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        worked_then_waiting(&mut store);
        store.follow_local_issues(&[]).unwrap();
        assert_eq!(store.local_issues().unwrap()[0].state, State::Waiting);
    }

    #[test]
    fn a_session_interrupted_after_its_work_fails_its_issue() {
        let directory = tempfile::tempdir().unwrap();
        let mut store = Store::open(&directory.path().join("state.sqlite3")).unwrap();
        let record = worked_then_waiting(&mut store);
        store.interrupt(record).unwrap();
        store.follow_local_issues(&[]).unwrap();
        let issue = &store.local_issues().unwrap()[0];
        assert_eq!(issue.state, State::Failed);
        assert!(issue.error.is_some());
    }
}
