//! The schedules the work host runs by itself: each time one is due it queues
//! an ordinary local issue, whether or not any window is open, and the
//! windows are told how the schedules stand.
use tsumugi_model::{NewSchedule, Schedule, ScheduleEdit, ScheduleRun};
use tsumugi_protocol::{Answer, TreeChange};

use crate::delegation::Error;
use crate::delegation::app::{root, with_store};
use crate::delegation::store::Store;
use crate::watch::Watched;

/// Every schedule, as the host last settled it, with when each enabled one is
/// next due.
pub(crate) fn listed(store: &Store, now_ms: u64) -> crate::delegation::Result<Vec<Schedule>> {
    Ok(store
        .numbered_schedules()?
        .into_iter()
        .map(|(_, mut schedule)| {
            schedule.next_ms = schedule
                .enabled
                .then(|| {
                    crate::schedule_time::next_after(&schedule.timezone, &schedule.cadence, now_ms)
                })
                .flatten();
            schedule
        })
        .collect())
}

pub(crate) async fn publish(watched: &Watched) -> Result<(), String> {
    let now = tsumugi_model::time::now_ms();
    let schedules = with_store(&root(), move |store| listed(store, now))
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::Schedules { schedules }]);
    Ok(())
}

/// Settle every time that came due by `now_ms`: each queues its request as an
/// issue, or is skipped, and is recorded either way so that it is not seen
/// again. `ready` says whether a schedule can run here, and what to put right
/// if not.
pub(crate) fn fire_due(
    store: &mut Store,
    now_ms: u64,
    ready: &dyn Fn(&Schedule) -> Result<(), String>,
) -> crate::delegation::Result<Vec<(Schedule, ScheduleRun)>> {
    let mut settled = Vec::new();
    for (schedule, cursor) in store.due_schedules()? {
        let Some(due) =
            crate::schedule_time::latest_in(&schedule.timezone, &schedule.cadence, cursor, now_ms)
        else {
            continue;
        };
        if let Some(run) = store.settle_schedule(&schedule.id, due, now_ms, ready(&schedule))? {
            settled.push((schedule, run));
        }
    }
    Ok(settled)
}

/// Whether `schedule` can queue a request on this machine now.
pub(crate) fn ready(schedule: &Schedule) -> Result<(), String> {
    if !schedule.dir.is_dir() {
        return Err(format!(
            "The directory {} is not available on this machine; choose another with `tsumugi schedule edit {} --dir <path>`",
            schedule.dir.display(),
            schedule.number
        ));
    }
    if let Some(profile) = &schedule.profile {
        let known = profile
            .parse::<tsumugi_model::profile::ProfileId>()
            .ok()
            .is_some_and(|id| crate::settings::current().find_profile(&id).is_some());
        if !known {
            return Err(format!(
                "Profile `{profile}` is not defined; choose another with `tsumugi schedule edit {} --profile <name>`, or add it under Settings → Agents",
                schedule.number
            ));
        }
    }
    Ok(())
}

/// Settle what is due now and tell the windows when anything was.
pub(crate) async fn tick(watched: &Watched, now_ms: u64) -> Result<(), String> {
    let settled = with_store(&root(), move |store| fire_due(store, now_ms, &ready))
        .await
        .map_err(|e| e.to_string())?;
    for (schedule, run) in &settled {
        tracing::info!(
            schedule = %schedule.name,
            outcome = run.outcome.word(),
            note = run.note.as_deref().unwrap_or(""),
            "a schedule was due"
        );
    }
    if !settled.is_empty() {
        publish(watched).await?;
        crate::local_issues::publish(watched).await?;
    }
    Ok(())
}

/// Keep `new`, disabled.
pub(crate) async fn save(watched: &Watched, mut new: NewSchedule) -> Result<Answer, String> {
    if new.timezone.trim().is_empty() {
        new.timezone = crate::schedule_time::system_zone();
    }
    crate::schedule_time::zone(&new.timezone)?;
    new.dir = tsumugi_sys::fs::canonicalize(&new.dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Choose an existing working directory on the work machine".to_owned())?;
    changed(watched, move |store, _| store.add_schedule(new)).await
}

pub(crate) async fn edit(
    watched: &Watched,
    id: String,
    mut edit: ScheduleEdit,
) -> Result<Answer, String> {
    if let Some(zone) = &edit.timezone {
        crate::schedule_time::zone(zone)?;
    }
    if let Some(dir) = &mut edit.dir {
        *dir = tsumugi_sys::fs::canonicalize(&*dir)
            .ok()
            .filter(|dir| dir.is_dir())
            .ok_or_else(|| "Choose an existing working directory on the work machine".to_owned())?;
    }
    changed(watched, move |store, now| {
        store.edit_schedule(&id, edit, now)
    })
    .await
}

/// Enable or pause the schedule: enabling is what allows its times to run.
pub(crate) async fn enable(watched: &Watched, id: String, on: bool) -> Result<Answer, String> {
    changed(watched, move |store, now| {
        store.set_schedule_enabled(&id, on, now)
    })
    .await
}

pub(crate) async fn delete(watched: &Watched, id: String) -> Result<Answer, String> {
    changed(watched, move |store, _| store.delete_schedule(&id)).await
}

/// Queue the schedule's request now, enabled or not.
pub(crate) async fn run_now(watched: &Watched, id: String) -> Result<Answer, String> {
    run_by_name(watched, &id)
        .await
        .map(|_| Answer::Done)
        .map_err(|e| e.message)
}

/// The schedule `selector` names, and the run that queued its request.
pub(crate) async fn run_by_name(
    watched: &Watched,
    selector: &str,
) -> crate::delegation::Result<(Schedule, ScheduleRun)> {
    let selector = selector.to_owned();
    let done = with_store(&root(), move |store| {
        let (_, schedule) = store.schedule_named(&selector)?;
        let run = store.run_schedule_now(
            &schedule.id,
            tsumugi_model::time::now_ms(),
            ready(&schedule),
        )?;
        Ok((schedule, run))
    })
    .await?;
    publish(watched).await.map_err(Error::storage)?;
    crate::local_issues::publish(watched)
        .await
        .map_err(Error::storage)?;
    watched.relist();
    Ok(done)
}

/// Change the schedules with `change`, given the time it is made at, and tell
/// the windows.
pub(crate) async fn changed(
    watched: &Watched,
    change: impl FnOnce(&mut Store, u64) -> crate::delegation::Result<()> + Send + 'static,
) -> Result<Answer, String> {
    with_store(&root(), move |store| {
        change(store, tsumugi_model::time::now_ms())
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    watched.relist();
    Ok(Answer::Done)
}

/// What a schedule says of itself to a command.
pub(crate) fn view(schedule: &Schedule) -> serde_json::Value {
    serde_json::json!({
        "number": schedule.number,
        "id": schedule.id,
        "name": schedule.name,
        "prompt": schedule.prompt,
        "dir": schedule.dir,
        "profile": schedule.profile,
        "timezone": schedule.timezone,
        "cadence": schedule.cadence,
        "target": schedule.target,
        "enabled": schedule.enabled,
        "grace_minutes": schedule.grace_minutes,
        "next_ms": schedule.next_ms,
        "last": schedule.last,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::{Cadence, DEFAULT_GRACE_MINUTES, RunOutcome, ScheduleTarget, SessionId};

    const HOUR: u64 = 3_600_000;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("delegation/state.sqlite3")).unwrap();
        (dir, store)
    }

    fn daily(store: &mut Store, zone: &str, hour: u8, now: u64) -> String {
        let new = NewSchedule {
            id: SessionId::new().to_string(),
            name: "Triage".into(),
            prompt: "Triage".into(),
            dir: std::env::temp_dir(),
            profile: None,
            timezone: zone.into(),
            cadence: Cadence::Daily { hour, minute: 0 },
            target: ScheduleTarget::Queue,
            grace_minutes: DEFAULT_GRACE_MINUTES,
        };
        let id = new.id.clone();
        store.add_schedule(new).unwrap();
        store.set_schedule_enabled(&id, true, now).unwrap();
        id
    }

    fn ok(_: &Schedule) -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn each_day_queues_one_request_however_often_the_clock_is_read() {
        let (_dir, mut store) = store();
        // 1970-01-02 00:00 UTC onward.
        let start = 24 * HOUR;
        daily(&mut store, "UTC", 9, start);
        let mut queued = 0;
        // Read every ten minutes for three days.
        let mut now = start;
        while now <= start + 72 * HOUR {
            queued += fire_due(&mut store, now, &ok)
                .unwrap()
                .iter()
                .filter(|(_, run)| run.outcome == RunOutcome::Queued)
                .count();
            // A request that ended frees the schedule for the next day.
            for issue in store.local_issues().unwrap() {
                store.fail_local_issue(&issue.id, "ended").unwrap();
            }
            now += HOUR / 6;
        }
        assert_eq!(queued, 3);
    }

    #[test]
    fn a_restarted_host_picks_up_where_the_last_one_stopped_without_repeating_a_time() {
        let (dir, mut store) = store();
        let start = 24 * HOUR;
        daily(&mut store, "UTC", 9, start);
        let at = start + 10 * HOUR;
        assert_eq!(fire_due(&mut store, at, &ok).unwrap().len(), 1);
        drop(store);
        let mut again = Store::open(&dir.path().join("delegation/state.sqlite3")).unwrap();
        assert!(fire_due(&mut again, at + 1, &ok).unwrap().is_empty());
        assert_eq!(again.local_issues().unwrap().len(), 1);
    }

    #[test]
    fn a_host_down_for_days_runs_the_latest_time_once_if_within_grace_and_skips_it_otherwise() {
        let (_dir, mut store) = store();
        let start = 24 * HOUR;
        daily(&mut store, "UTC", 9, start);
        // Down from day 1 to day 5 at 10:00: the time of day 5, 09:00, is an hour old.
        let back = start + 4 * 24 * HOUR + 10 * HOUR;
        let settled = fire_due(&mut store, back, &ok).unwrap();
        assert_eq!(settled.len(), 1, "the days missed are not run one by one");
        assert_eq!(settled[0].1.outcome, RunOutcome::Queued);
        // Down again until 23:00 the next day: its time is 14 h old, past the 12 h grace.
        let later = start + 5 * 24 * HOUR + 23 * HOUR;
        let settled = fire_due(&mut store, later, &ok).unwrap();
        assert_eq!(settled[0].1.outcome, RunOutcome::SkippedMissed);
    }

    #[test]
    fn the_time_zone_decides_the_day_a_time_falls_on() {
        let (_dir, mut store) = store();
        let start = 24 * HOUR;
        daily(&mut store, "Asia/Tokyo", 9, start);
        // 09:00 in Tokyo is 00:00 UTC, so the first time after `start` (itself
        // 09:00 there) is `start` a day on, and not 09:00 UTC.
        assert!(
            fire_due(&mut store, start + 10 * HOUR, &ok)
                .unwrap()
                .is_empty()
        );
        let settled = fire_due(&mut store, start + 24 * HOUR + HOUR / 2, &ok).unwrap();
        assert_eq!(settled.len(), 1);
        assert_eq!(settled[0].1.due_ms, start + 24 * HOUR);
    }

    #[test]
    fn a_schedule_that_is_not_ready_records_why_and_queues_nothing() {
        let (_dir, mut store) = store();
        let start = 24 * HOUR;
        daily(&mut store, "UTC", 9, start);
        let settled = fire_due(&mut store, start + 10 * HOUR, &|_| {
            Err("The directory is not available".into())
        })
        .unwrap();
        assert_eq!(settled[0].1.outcome, RunOutcome::Failed);
        assert!(store.local_issues().unwrap().is_empty());
    }

    #[test]
    fn a_listing_says_when_each_enabled_schedule_is_next_due_and_none_for_a_paused_one() {
        let (_dir, mut store) = store();
        let start = 24 * HOUR;
        let id = daily(&mut store, "UTC", 9, start);
        let all = listed(&store, start + 10 * HOUR).unwrap();
        assert_eq!(all[0].next_ms, Some(start + 33 * HOUR));
        store.set_schedule_enabled(&id, false, start).unwrap();
        assert_eq!(listed(&store, start).unwrap()[0].next_ms, None);
    }

    #[test]
    fn a_schedule_that_is_missing_its_directory_is_not_ready() {
        let schedule = |dir: &str, profile: Option<&str>| Schedule {
            id: "x".into(),
            number: 3,
            name: "n".into(),
            prompt: "p".into(),
            dir: dir.into(),
            profile: profile.map(Into::into),
            timezone: "UTC".into(),
            cadence: Cadence::Daily { hour: 9, minute: 0 },
            target: ScheduleTarget::Queue,
            enabled: true,
            grace_minutes: 10,
            next_ms: None,
            last: None,
        };
        let error = ready(&schedule("/no/such/directory/at/all", None)).unwrap_err();
        assert!(
            error.contains("not available") && error.contains("edit 3"),
            "{error}"
        );
        let error = ready(&schedule(
            &std::env::temp_dir().to_string_lossy(),
            Some("no-such-profile"),
        ))
        .unwrap_err();
        assert!(
            error.contains("no-such-profile") && error.contains("Settings"),
            "{error}"
        );
        assert_eq!(
            ready(&schedule(&std::env::temp_dir().to_string_lossy(), None)),
            Ok(())
        );
    }
}
