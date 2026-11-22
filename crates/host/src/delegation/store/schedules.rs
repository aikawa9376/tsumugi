//! The schedules the work host runs on its own, beside the queue they add to.
//!
//! A time at which a schedule was due is settled once: its row in
//! `schedulelet mut also_one = Store::open(&directory.path().join("delegation/state.sqlite3")).unwrap();runs` is the record that it was, and it is written in the same
//! transaction that queues the issue it asks for, so a host that restarts, or
//! two that look at once, never queue it twice.

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};
use tsumugi_model::{
    NewSchedule, RunOutcome, Schedule, ScheduleEdit, ScheduleRun, ScheduleTarget, SessionId,
};

use super::{Code, Error, Result, Store};

/// How many times a schedule was due that it remembers.
const KEPT_RUNS: i64 = 50;

/// The states of an issue that is still work in the queue: a schedule adds no
/// request while one of its own is in any of them.
const UNFINISHED: &str = "('queued', 'starting', 'running', 'waiting')";

const COLUMNS: &str = "s.id, s.seq, s.name, s.prompt, s.dir, s.profile, s.timezone, s.cadence,
     s.target, s.enabled, s.grace_minutes, s.cursor_ms";

impl Store {
    /// Keep `new`, disabled. A schedule kept under its id before, with the
    /// same definition, is kept as it is.
    pub fn add_schedule(&mut self, new: NewSchedule) -> Result<()> {
        if new.id.parse::<SessionId>().is_err() {
            return Err(Error::new(Code::Invalid, "A schedule needs an identity"));
        }
        if let Some(refusal) = tsumugi_model::refuse_schedule(&new) {
            return Err(Error::new(Code::Invalid, refusal));
        }
        if let Some((_, kept)) = self
            .numbered_schedules()?
            .into_iter()
            .find(|(_, s)| s.id == new.id)
        {
            let same = kept.name == new.name.trim()
                && kept.prompt == new.prompt
                && kept.dir == new.dir
                && kept.profile == new.profile
                && kept.timezone == new.timezone
                && kept.cadence == new.cadence
                && kept.target == new.target
                && kept.grace_minutes == new.grace_minutes;
            return if same {
                Ok(())
            } else {
                Err(Error::new(
                    Code::Invalid,
                    "This schedule identity already holds a different definition",
                ))
            };
        }
        self.connection.execute(
            "INSERT INTO schedules (id, name, prompt, dir, profile, timezone, cadence, target, enabled, grace_minutes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)",
            params![
                new.id,
                new.name.trim(),
                new.prompt,
                new.dir.to_string_lossy(),
                new.profile,
                new.timezone,
                json(&new.cadence)?,
                json(&new.target)?,
                new.grace_minutes,
            ],
        )?;
        Ok(())
    }

    /// Every schedule with its short number, as it was last settled. `next_ms`
    /// is the host's to fill in, since it holds the clock and the zones.
    pub fn numbered_schedules(&self) -> Result<Vec<(u64, Schedule)>> {
        Ok(self
            .read_schedules()?
            .into_iter()
            .map(|(schedule, _)| (schedule.number, schedule))
            .collect())
    }

    /// The enabled schedules, each with the latest time it was settled for.
    pub fn due_schedules(&self) -> Result<Vec<(Schedule, u64)>> {
        Ok(self
            .read_schedules()?
            .into_iter()
            .filter(|(schedule, _)| schedule.enabled)
            .collect())
    }

    fn read_schedules(&self) -> Result<Vec<(Schedule, u64)>> {
        let mut query = self
            .connection
            .prepare(&format!("SELECT {COLUMNS} FROM schedules s ORDER BY s.seq"))?;
        let rows = query.query_map([], schedule_row)?;
        let mut all = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        for (schedule, _) in &mut all {
            schedule.last = self.schedule_runs(&schedule.id, 1)?.into_iter().next();
        }
        Ok(all)
    }

    /// The schedule `selector` names: its number, or the start of its id when
    /// no schedule has that number.
    pub fn schedule_named(&self, selector: &str) -> Result<(u64, Schedule)> {
        let selector = selector.trim().trim_start_matches('#');
        if selector.is_empty() {
            return Err(Error::new(Code::Invalid, "Name a schedule by its number"));
        }
        let all = self.numbered_schedules()?;
        if let Ok(number) = selector.parse::<u64>()
            && let Some(found) = all.iter().find(|(n, _)| *n == number)
        {
            return Ok(found.clone());
        }
        let prefix = selector.to_ascii_lowercase();
        let mut matching: Vec<_> = all
            .into_iter()
            .filter(|(_, schedule)| schedule.id.starts_with(&prefix))
            .collect();
        match matching.len() {
            0 => Err(Error::new(
                Code::NotFound,
                format!("No schedule is named {selector}"),
            )),
            1 => Ok(matching.remove(0)),
            _ => {
                let numbers: Vec<String> = matching.iter().map(|(n, _)| format!("#{n}")).collect();
                Err(Error::new(
                    Code::Invalid,
                    format!(
                        "{selector} names several schedules ({}); use more of the id or a number",
                        numbers.join(", ")
                    ),
                ))
            }
        }
    }

    /// The times a schedule was due that it remembers, the latest first.
    pub fn schedule_runs(&self, id: &str, limit: usize) -> Result<Vec<ScheduleRun>> {
        let mut query = self.connection.prepare(
            "SELECT due_ms, at_ms, outcome, issue, note FROM schedule_runs
             WHERE schedule = ?1 ORDER BY due_ms DESC, seq DESC LIMIT ?2",
        )?;
        let rows = query.query_map(params![id, limit as i64], |row| {
            let outcome: String = row.get(2)?;
            Ok(ScheduleRun {
                due_ms: row.get::<_, i64>(0)?.unsigned_abs(),
                at_ms: row.get::<_, i64>(1)?.unsigned_abs(),
                outcome: RunOutcome::parse(&outcome).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        format!("no outcome is called {outcome}").into(),
                    )
                })?,
                issue: row.get(3)?,
                note: row.get(4)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Enable or pause a schedule. Enabling allows the times from `now_ms` on
    /// and none before it, so a time that passed while it was paused or
    /// before it was kept never runs.
    pub fn set_schedule_enabled(&mut self, id: &str, enabled: bool, now_ms: u64) -> Result<()> {
        let changed = self.connection.execute(
            "UPDATE schedules SET enabled = ?2, cursor_ms = CASE WHEN ?2 = 1 THEN ?3 ELSE cursor_ms END
             WHERE id = ?1 AND enabled != ?2",
            params![id, enabled, now_ms as i64],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Refused,
                if enabled {
                    "The schedule is already enabled"
                } else {
                    "The schedule is already paused"
                },
            ));
        }
        Ok(())
    }

    /// Change what a schedule says. Work it already queued is not touched. A
    /// new timetable allows the times from `now_ms` on, as enabling does.
    pub fn edit_schedule(&mut self, id: &str, edit: ScheduleEdit, now_ms: u64) -> Result<()> {
        if let Some(refusal) = tsumugi_model::refuse_edit(&edit) {
            return Err(Error::new(Code::Invalid, refusal));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let kept = transaction
            .query_row(
                "SELECT timezone, cadence FROM schedules WHERE id = ?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((timezone, cadence)) = kept else {
            return Err(Error::new(Code::NotFound, "The schedule is gone"));
        };
        let new_cadence = edit.cadence.as_ref().map(json).transpose()?;
        let retimed = edit.timezone.as_ref().is_some_and(|zone| *zone != timezone)
            || new_cadence.as_ref().is_some_and(|c| *c != cadence);
        if let Some(name) = &edit.name {
            transaction.execute(
                "UPDATE schedules SET name = ?2 WHERE id = ?1",
                params![id, name.trim()],
            )?;
        }
        if let Some(prompt) = &edit.prompt {
            transaction.execute(
                "UPDATE schedules SET prompt = ?2 WHERE id = ?1",
                params![id, prompt],
            )?;
        }
        if let Some(dir) = &edit.dir {
            transaction.execute(
                "UPDATE schedules SET dir = ?2 WHERE id = ?1",
                params![id, dir.to_string_lossy()],
            )?;
        }
        if let Some(profile) = &edit.profile {
            transaction.execute(
                "UPDATE schedules SET profile = ?2 WHERE id = ?1",
                params![id, profile],
            )?;
        }
        if let Some(grace) = edit.grace_minutes {
            transaction.execute(
                "UPDATE schedules SET grace_minutes = ?2 WHERE id = ?1",
                params![id, grace],
            )?;
        }
        if let Some(zone) = &edit.timezone {
            transaction.execute(
                "UPDATE schedules SET timezone = ?2 WHERE id = ?1",
                params![id, zone],
            )?;
        }
        if let Some(cadence) = &new_cadence {
            transaction.execute(
                "UPDATE schedules SET cadence = ?2 WHERE id = ?1",
                params![id, cadence],
            )?;
        }
        if retimed {
            transaction.execute(
                "UPDATE schedules SET cursor_ms = MAX(cursor_ms, ?2) WHERE id = ?1",
                params![id, now_ms as i64],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Remove a schedule and what it remembers of its times. The issues it
    /// queued are ordinary issues, and stay as they are, work in progress
    /// included.
    pub fn delete_schedule(&mut self, id: &str) -> Result<()> {
        let changed = self
            .connection
            .execute("DELETE FROM schedules WHERE id = ?1", [id])?;
        if changed != 1 {
            return Err(Error::new(Code::NotFound, "The schedule is gone"));
        }
        Ok(())
    }

    /// Settle the time `due_ms` at which the enabled schedule `id` was due,
    /// seen at `now_ms`, and say how it ended; none when that time, or a later
    /// one, was settled before or the schedule is not enabled. `ready` says
    /// whether the schedule can run here and now, and what to put right if
    /// not.
    pub fn settle_schedule(
        &mut self,
        id: &str,
        due_ms: u64,
        now_ms: u64,
        ready: std::result::Result<(), String>,
    ) -> Result<Option<ScheduleRun>> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found = transaction
            .query_row(
                &format!("SELECT {COLUMNS} FROM schedules s WHERE s.id = ?1"),
                [id],
                schedule_row,
            )
            .optional()?;
        let Some((schedule, cursor)) = found else {
            return Ok(None);
        };
        if !schedule.enabled || cursor >= due_ms {
            return Ok(None);
        }
        let late = now_ms.saturating_sub(due_ms);
        let grace = u64::from(schedule.grace_minutes) * 60_000;
        let (outcome, note) = if late > grace {
            (
                RunOutcome::SkippedMissed,
                Some(format!(
                    "Due {} min before the host saw it; a schedule runs a missed time only within {} min",
                    late / 60_000,
                    schedule.grace_minutes
                )),
            )
        } else {
            decide(&transaction, &schedule, ready)?
        };
        let issue = (outcome == RunOutcome::Queued)
            .then(|| queue_issue(&transaction, &schedule))
            .transpose()?;
        let run = ScheduleRun {
            due_ms,
            at_ms: now_ms,
            outcome,
            issue,
            note,
        };
        record(&transaction, &schedule.id, &run)?;
        transaction.execute(
            "UPDATE schedules SET cursor_ms = ?2 WHERE id = ?1",
            params![schedule.id, due_ms as i64],
        )?;
        transaction.commit()?;
        Ok(Some(run))
    }

    /// Queue the schedule's request now, enabled or not: the user asked.
    /// Refused, with why, when it cannot be queued; it is the next time the
    /// schedule is due that is left alone.
    pub fn run_schedule_now(
        &mut self,
        id: &str,
        now_ms: u64,
        ready: std::result::Result<(), String>,
    ) -> Result<ScheduleRun> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let found = transaction
            .query_row(
                &format!("SELECT {COLUMNS} FROM schedules s WHERE s.id = ?1"),
                [id],
                schedule_row,
            )
            .optional()?;
        let Some((schedule, _)) = found else {
            return Err(Error::new(Code::NotFound, "The schedule is gone"));
        };
        let (outcome, note) = decide(&transaction, &schedule, ready)?;
        if outcome != RunOutcome::Queued {
            return Err(Error::new(
                if outcome == RunOutcome::SkippedBusy {
                    Code::Refused
                } else {
                    Code::Invalid
                },
                note.unwrap_or_default(),
            ));
        }
        let issue = queue_issue(&transaction, &schedule)?;
        let run = ScheduleRun {
            due_ms: now_ms,
            at_ms: now_ms,
            outcome,
            issue: Some(issue),
            note: Some("Run by the user".into()),
        };
        record(&transaction, &schedule.id, &run)?;
        transaction.commit()?;
        Ok(run)
    }

    /// The profile a queued issue is to be worked with; none for the default.
    pub fn local_issue_profile(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT profile FROM schedule_issues WHERE issue = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }
}

fn schedule_row(row: &Row<'_>) -> rusqlite::Result<(Schedule, u64)> {
    let bad = |at: usize, error: serde_json::Error| {
        rusqlite::Error::FromSqlConversionFailure(at, rusqlite::types::Type::Text, Box::new(error))
    };
    let cadence: String = row.get(7)?;
    let target: String = row.get(8)?;
    let schedule = Schedule {
        id: row.get(0)?,
        number: row.get::<_, i64>(1)?.unsigned_abs(),
        name: row.get(2)?,
        prompt: row.get(3)?,
        dir: std::path::PathBuf::from(row.get::<_, String>(4)?),
        profile: row.get(5)?,
        timezone: row.get(6)?,
        cadence: serde_json::from_str(&cadence).map_err(|e| bad(7, e))?,
        target: serde_json::from_str(&target).map_err(|e| bad(8, e))?,
        enabled: row.get(9)?,
        grace_minutes: row.get(10)?,
        next_ms: None,
        last: None,
    };
    Ok((schedule, row.get::<_, i64>(11)?.unsigned_abs()))
}

fn json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| Error::new(Code::Invalid, e.to_string()))
}

/// Whether the request can be queued now, and if not, why.
fn decide(
    transaction: &rusqlite::Transaction<'_>,
    schedule: &Schedule,
    ready: std::result::Result<(), String>,
) -> Result<(RunOutcome, Option<String>)> {
    if let Err(problem) = ready {
        return Ok((RunOutcome::Failed, Some(problem)));
    }
    if matches!(schedule.target, ScheduleTarget::Session { .. }) {
        return Ok((
            RunOutcome::Failed,
            Some("Handing a prompt to a running session is not available yet".into()),
        ));
    }
    let unfinished: bool = transaction.query_row(
        &format!(
            "SELECT EXISTS (SELECT 1 FROM schedule_issues o JOIN local_issues i ON i.id = o.issue
                 WHERE o.schedule = ?1 AND i.state IN {UNFINISHED})"
        ),
        [&schedule.id],
        |row| row.get(0),
    )?;
    if unfinished {
        return Ok((
            RunOutcome::SkippedBusy,
            Some("The previous request of this schedule has not ended".into()),
        ));
    }
    Ok((RunOutcome::Queued, None))
}

/// Queue the request as an ordinary issue that the schedule is named on, which
/// then waits its turn in its directory as any other.
fn queue_issue(transaction: &rusqlite::Transaction<'_>, schedule: &Schedule) -> Result<String> {
    let id = SessionId::new().to_string();
    transaction.execute(
        "INSERT INTO local_issues (id, dir, title, body, state) VALUES (?1, ?2, ?3, ?4, 'queued')",
        params![
            id,
            schedule.dir.to_string_lossy(),
            schedule.name,
            schedule.prompt
        ],
    )?;
    transaction.execute(
        "INSERT INTO schedule_issues (issue, schedule, profile) VALUES (?1, ?2, ?3)",
        params![id, schedule.id, schedule.profile],
    )?;
    Ok(id)
}

fn record(
    transaction: &rusqlite::Transaction<'_>,
    schedule: &str,
    run: &ScheduleRun,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO schedule_runs (schedule, due_ms, at_ms, outcome, issue, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            schedule,
            run.due_ms as i64,
            run.at_ms as i64,
            run.outcome.word(),
            run.issue,
            run.note
        ],
    )?;
    transaction.execute(
        "DELETE FROM schedule_runs WHERE schedule = ?1 AND seq NOT IN
         (SELECT seq FROM schedule_runs WHERE schedule = ?1 ORDER BY due_ms DESC, seq DESC LIMIT ?2)",
        params![schedule, KEPT_RUNS],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use tsumugi_model::{
        Cadence, DEFAULT_GRACE_MINUTES, LocalIssueState as State, NewSchedule, RunOutcome,
        ScheduleEdit, ScheduleTarget,
    };

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("delegation/state.sqlite3")).unwrap();
        (dir, store)
    }

    const MINUTE: u64 = 60_000;
    const HOUR: u64 = 60 * MINUTE;

    fn new(name: &str) -> NewSchedule {
        NewSchedule {
            id: SessionId::new().to_string(),
            name: name.into(),
            prompt: "Triage new issues".into(),
            dir: std::env::temp_dir().join("scheduled"),
            profile: Some("work".into()),
            timezone: "UTC".into(),
            cadence: Cadence::Daily { hour: 9, minute: 0 },
            target: ScheduleTarget::Queue,
            grace_minutes: DEFAULT_GRACE_MINUTES,
        }
    }

    fn enabled(store: &mut Store, name: &str, now: u64) -> String {
        let schedule = new(name);
        let id = schedule.id.clone();
        store.add_schedule(schedule).unwrap();
        store.set_schedule_enabled(&id, true, now).unwrap();
        id
    }

    fn ready() -> std::result::Result<(), String> {
        Ok(())
    }

    #[test]
    fn a_schedule_is_kept_disabled_and_never_queues_anything_by_itself() {
        let (_directory, mut store) = store();
        let schedule = new("Triage");
        let id = schedule.id.clone();
        store.add_schedule(schedule.clone()).unwrap();
        store.add_schedule(schedule).unwrap();
        let (number, kept) = store.schedule_named(&id).unwrap();
        assert!(!kept.enabled && kept.next_ms.is_none() && kept.last.is_none());
        assert_eq!(number, 1);
        assert_eq!(store.numbered_schedules().unwrap().len(), 1);
        // Nothing is due while it is disabled, whatever its times.
        assert!(store.due_schedules().unwrap().is_empty());
        assert_eq!(
            store
                .settle_schedule(&id, 5 * HOUR, 5 * HOUR, ready())
                .unwrap(),
            None
        );
        assert!(store.local_issues().unwrap().is_empty());
    }

    #[test]
    fn an_identity_cannot_hold_two_definitions() {
        let (_directory, mut store) = store();
        let schedule = new("Triage");
        store.add_schedule(schedule.clone()).unwrap();
        let mut other = schedule;
        other.prompt = "Something else".into();
        assert!(store.add_schedule(other).is_err());
    }

    #[test]
    fn a_time_that_comes_due_queues_an_ordinary_issue_that_names_its_schedule() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        let run = store
            .settle_schedule(&id, 9 * HOUR, 9 * HOUR + MINUTE, ready())
            .unwrap()
            .unwrap();
        assert_eq!(run.outcome, RunOutcome::Queued);
        let issues = store.local_issues().unwrap();
        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert_eq!(Some(&issue.id), run.issue.as_ref());
        assert_eq!(issue.state, State::Queued);
        assert_eq!(issue.schedule.as_deref(), Some(id.as_str()));
        assert_eq!(
            (issue.title.as_str(), issue.body.as_str()),
            ("Triage", "Triage new issues")
        );
        assert_eq!(
            store.local_issue_profile(&issue.id).unwrap().as_deref(),
            Some("work")
        );
        // The directory's queue then takes it as any other issue.
        assert_eq!(store.claim_local_issues().unwrap().len(), 1);
    }

    #[test]
    fn a_time_is_settled_once_however_often_or_by_however_many_hosts_it_is_seen() {
        let (directory, mut one) = store();
        let mut also_one =
            Store::open(&directory.path().join("one.sqlite3")).unwrap_or_else(|_| {
                panic!("the same database opens twice");
            });
        let id = enabled(&mut one, "Triage", 0);
        assert!(
            one.settle_schedule(&id, 9 * HOUR, 9 * HOUR, ready())
                .unwrap()
                .is_some()
        );
        assert_eq!(
            also_one
                .settle_schedule(&id, 9 * HOUR, 9 * HOUR + 1, ready())
                .unwrap(),
            None
        );
        assert_eq!(
            one.settle_schedule(&id, 9 * HOUR, 9 * HOUR + 2, ready())
                .unwrap(),
            None
        );
        // An earlier time that was seen late is not settled behind a later one.
        assert_eq!(
            one.settle_schedule(&id, 8 * HOUR, 9 * HOUR + 3, ready())
                .unwrap(),
            None
        );
        assert_eq!(one.local_issues().unwrap().len(), 1);
    }

    #[test]
    fn enabling_allows_the_times_from_then_on_and_none_before() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 10 * HOUR);
        assert_eq!(
            store
                .settle_schedule(&id, 9 * HOUR, 10 * HOUR, ready())
                .unwrap(),
            None,
            "a time before it was enabled never runs"
        );
        assert!(
            store
                .settle_schedule(&id, 33 * HOUR, 33 * HOUR, ready())
                .unwrap()
                .is_some()
        );
        // Pausing and enabling again forgets what passed in between.
        store.set_schedule_enabled(&id, false, 34 * HOUR).unwrap();
        store.set_schedule_enabled(&id, true, 60 * HOUR).unwrap();
        assert_eq!(
            store
                .settle_schedule(&id, 57 * HOUR, 60 * HOUR, ready())
                .unwrap(),
            None
        );
    }

    #[test]
    fn a_time_missed_by_more_than_the_grace_is_skipped_and_recorded() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        let late = 9 * HOUR + u64::from(DEFAULT_GRACE_MINUTES) * MINUTE + 1;
        let run = store
            .settle_schedule(&id, 9 * HOUR, late, ready())
            .unwrap()
            .unwrap();
        assert_eq!(run.outcome, RunOutcome::SkippedMissed);
        assert!(run.issue.is_none() && run.note.is_some());
        assert!(store.local_issues().unwrap().is_empty());
        // At the edge of the grace it still runs.
        let run = store
            .settle_schedule(
                &id,
                33 * HOUR,
                33 * HOUR + u64::from(DEFAULT_GRACE_MINUTES) * MINUTE,
                ready(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(run.outcome, RunOutcome::Queued);
    }

    #[test]
    fn a_request_still_unfinished_keeps_the_next_time_from_adding_another() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        let first = store
            .settle_schedule(&id, 9 * HOUR, 9 * HOUR, ready())
            .unwrap()
            .unwrap();
        assert_eq!(first.outcome, RunOutcome::Queued);
        // Waiting in the queue, working, or waiting on the user all count.
        for (state, at) in [
            ("queued", 33),
            ("starting", 57),
            ("running", 81),
            ("waiting", 105),
        ] {
            store
                .connection
                .execute("UPDATE local_issues SET state = ?1", [state])
                .unwrap();
            let run = store
                .settle_schedule(&id, at * HOUR, at * HOUR, ready())
                .unwrap()
                .unwrap();
            assert_eq!(run.outcome, RunOutcome::SkippedBusy, "{state}");
        }
        assert_eq!(
            store.local_issues().unwrap().len(),
            1,
            "the queue did not grow"
        );
        // Once it ends, the next time queues again.
        store
            .connection
            .execute("UPDATE local_issues SET state = 'done'", [])
            .unwrap();
        let run = store
            .settle_schedule(&id, 129 * HOUR, 129 * HOUR, ready())
            .unwrap()
            .unwrap();
        assert_eq!(run.outcome, RunOutcome::Queued);
        assert_eq!(store.local_issues().unwrap().len(), 2);
    }

    #[test]
    fn a_schedule_that_cannot_run_here_says_what_to_put_right_and_keeps_its_timetable() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        let run = store
            .settle_schedule(
                &id,
                9 * HOUR,
                9 * HOUR,
                Err("Profile `work` is not defined".into()),
            )
            .unwrap()
            .unwrap();
        assert_eq!(run.outcome, RunOutcome::Failed);
        assert_eq!(run.note.as_deref(), Some("Profile `work` is not defined"));
        assert!(store.local_issues().unwrap().is_empty());
        let (_, kept) = store.schedule_named(&id).unwrap();
        assert!(kept.enabled, "the next time is still tried");
        assert_eq!(kept.last.unwrap().outcome, RunOutcome::Failed);
    }

    #[test]
    fn a_schedule_that_wakes_a_session_is_kept_but_is_not_run_yet() {
        let (_directory, mut store) = store();
        let mut schedule = new("Supervise");
        schedule.target = ScheduleTarget::Session {
            code: "ab12".into(),
        };
        let id = schedule.id.clone();
        store.add_schedule(schedule).unwrap();
        store.set_schedule_enabled(&id, true, 0).unwrap();
        let run = store
            .settle_schedule(&id, HOUR, HOUR, ready())
            .unwrap()
            .unwrap();
        assert_eq!(run.outcome, RunOutcome::Failed);
        assert!(store.local_issues().unwrap().is_empty());
    }

    #[test]
    fn changing_or_deleting_a_schedule_leaves_the_work_it_queued_alone() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        store
            .settle_schedule(&id, 9 * HOUR, 9 * HOUR, ready())
            .unwrap();
        let issue = store.local_issues().unwrap().remove(0);
        store.claim_local_issues().unwrap();
        store
            .connection
            .execute("UPDATE local_issues SET state = 'running'", [])
            .unwrap();
        store
            .edit_schedule(
                &id,
                ScheduleEdit {
                    prompt: Some("Another prompt".into()),
                    name: Some("Renamed".into()),
                    ..Default::default()
                },
                10 * HOUR,
            )
            .unwrap();
        let kept = store.local_issues().unwrap().remove(0);
        assert_eq!(
            (kept.title.as_str(), kept.state),
            ("Triage", State::Running)
        );
        assert_eq!(kept.body, issue.body);
        store.delete_schedule(&id).unwrap();
        let kept = store.local_issues().unwrap().remove(0);
        assert_eq!(kept.state, State::Running);
        assert!(store.numbered_schedules().unwrap().is_empty());
        assert!(store.schedule_runs(&id, 10).unwrap().is_empty());
    }

    #[test]
    fn a_new_timetable_allows_the_times_from_then_on_and_other_changes_do_not() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        store
            .edit_schedule(
                &id,
                ScheduleEdit {
                    name: Some("Renamed".into()),
                    ..Default::default()
                },
                20 * HOUR,
            )
            .unwrap();
        assert!(
            store
                .settle_schedule(&id, 9 * HOUR, 21 * HOUR, ready())
                .unwrap()
                .is_some(),
            "a rename keeps the times already allowed"
        );
        store
            .edit_schedule(
                &id,
                ScheduleEdit {
                    cadence: Some(Cadence::Daily { hour: 7, minute: 0 }),
                    ..Default::default()
                },
                40 * HOUR,
            )
            .unwrap();
        assert_eq!(
            store
                .settle_schedule(&id, 31 * HOUR, 41 * HOUR, ready())
                .unwrap(),
            None,
            "a time from before the change is not run"
        );
        let (_, kept) = store.schedule_named(&id).unwrap();
        assert_eq!(kept.cadence, Cadence::Daily { hour: 7, minute: 0 });
        assert_eq!(kept.name, "Renamed");
    }

    #[test]
    fn running_now_queues_a_request_enabled_or_not_unless_the_last_one_is_unfinished() {
        let (_directory, mut store) = store();
        let schedule = new("Triage");
        let id = schedule.id.clone();
        store.add_schedule(schedule).unwrap();
        let run = store.run_schedule_now(&id, 5 * HOUR, ready()).unwrap();
        assert_eq!(run.outcome, RunOutcome::Queued);
        assert_eq!(store.local_issues().unwrap().len(), 1);
        let refused = store.run_schedule_now(&id, 6 * HOUR, ready()).unwrap_err();
        assert_eq!(refused.code, Code::Refused);
        store
            .connection
            .execute("UPDATE local_issues SET state = 'done'", [])
            .unwrap();
        let refused = store
            .run_schedule_now(&id, 7 * HOUR, Err("The directory is gone".into()))
            .unwrap_err();
        assert!(refused.message.contains("directory"));
        assert_eq!(store.local_issues().unwrap().len(), 1);
    }

    #[test]
    fn only_the_latest_times_are_remembered_and_the_newest_is_shown_as_the_last() {
        let (_directory, mut store) = store();
        let id = enabled(&mut store, "Triage", 0);
        for day in 1..=60u64 {
            store
                .settle_schedule(&id, day * 24 * HOUR, day * 24 * HOUR, Err("down".into()))
                .unwrap();
        }
        assert_eq!(store.schedule_runs(&id, 1000).unwrap().len(), 50);
        let (_, kept) = store.schedule_named(&id).unwrap();
        assert_eq!(kept.last.unwrap().due_ms, 60 * 24 * HOUR);
    }

    #[test]
    fn a_schedule_is_named_by_its_number_or_the_start_of_its_id() {
        let (_directory, mut store) = store();
        let one = new("One");
        let two = new("Two");
        let (one_id, two_id) = (one.id.clone(), two.id.clone());
        store.add_schedule(one).unwrap();
        store.add_schedule(two).unwrap();
        assert_eq!(store.schedule_named("#2").unwrap().1.id, two_id);
        assert_eq!(store.schedule_named(&one_id[..12]).unwrap().0, 1);
        assert_eq!(store.schedule_named("zz").unwrap_err().code, Code::NotFound);
        store.delete_schedule(&two_id).unwrap();
        store.add_schedule(new("Three")).unwrap();
        assert_eq!(
            store.schedule_named("3").unwrap().1.name,
            "Three",
            "a number is not handed on"
        );
    }

    #[test]
    fn a_database_from_before_schedules_gains_them_without_losing_issues() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("old.sqlite3");
        let mut store = Store::open(&path).unwrap();
        let issue = tsumugi_model::NewLocalIssue {
            id: SessionId::new().to_string(),
            dir: std::env::temp_dir(),
            title: "Kept".into(),
            body: "Body".into(),
        };
        store.add_local_issue(issue).unwrap();
        drop(store);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "DROP TABLE schedule_issues; DROP TABLE schedule_runs; DROP TABLE schedules;
                 PRAGMA user_version = 14;",
            )
            .unwrap();
        drop(connection);
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.local_issues().unwrap()[0].title, "Kept");
        store.add_schedule(new("Triage")).unwrap();
    }
}
