//! When a schedule is due: its cadence read on the clock of its time zone.
//!
//! Every calendar day, and every hour, of a zone yields at most one time, so
//! a clock that jumps never makes a schedule run twice in a day or an hour:
//! a time that falls in the hour a clock skips is run as the clock then
//! reads it, and one that falls in an hour a clock repeats is run the first
//! time round.

use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use tsumugi_model::{Cadence, Weekday};

/// How many days either way a time is looked for: a weekly schedule is due
/// within a week, and a day of slack covers a clock that moves.
const REACH_DAYS: i64 = 8;

/// The zone `name` names, or why there is none.
pub fn zone(name: &str) -> Result<TimeZone, String> {
    TimeZone::get(name).map_err(|_| {
        format!("`{name}` is not an IANA time zone; use a name such as `Asia/Tokyo` or `UTC`")
    })
}

/// The zone of this machine, or UTC when it cannot be told.
pub fn system_zone() -> String {
    TimeZone::system()
        .iana_name()
        .map_or_else(|| "UTC".to_owned(), str::to_owned)
}

/// The latest time the cadence is due after `after_ms` and by `now_ms`, both
/// in milliseconds since the Unix epoch.
pub fn latest_in(timezone: &str, cadence: &Cadence, after_ms: u64, now_ms: u64) -> Option<u64> {
    let zone = zone(timezone).ok()?;
    let today = local_date(&zone, now_ms)?;
    for back in 0..=REACH_DAYS {
        let date = today.checked_sub(Span::new().days(back)).ok()?;
        let latest = on(&zone, cadence, date)
            .into_iter()
            .filter(|at| *at <= now_ms)
            .max();
        if let Some(at) = latest {
            return (at > after_ms).then_some(at);
        }
    }
    None
}

/// The first time the cadence is due after `now_ms`.
pub fn next_after(timezone: &str, cadence: &Cadence, now_ms: u64) -> Option<u64> {
    let zone = zone(timezone).ok()?;
    let today = local_date(&zone, now_ms)?;
    (0..=REACH_DAYS)
        .filter_map(|ahead| today.checked_add(Span::new().days(ahead)).ok())
        .flat_map(|date| on(&zone, cadence, date))
        .filter(|at| *at > now_ms)
        .min()
}

/// A time as the clock of `timezone` reads it: `2026-11-23 09:00`.
pub fn clock(timezone: &str, ms: u64) -> String {
    let stamp = i64::try_from(ms)
        .ok()
        .and_then(|ms| Timestamp::from_millisecond(ms).ok());
    match (zone(timezone), stamp) {
        (Ok(zone), Some(stamp)) => {
            let at = stamp.to_zoned_datetime(zone);
            format!(
                "{} {:02}:{:02}",
                at.date(),
                at.time().hour(),
                at.time().minute()
            )
        }
        _ => ms.to_string(),
    }
}

fn local_date(zone: &TimeZone, ms: u64) -> Option<Date> {
    let stamp = Timestamp::from_millisecond(i64::try_from(ms).ok()?).ok()?;
    Some(stamp.to_zoned_datetime(zone.clone()).date())
}

/// The times the cadence is due on a calendar day of the zone.
fn on(zone: &TimeZone, cadence: &Cadence, date: Date) -> Vec<u64> {
    let weekday = Weekday::ALL[usize::from(date.weekday().to_monday_zero_offset() as u8)];
    let clocks: Vec<(i8, i8)> = match cadence {
        Cadence::Hourly { minute } => (0..24).map(|hour| (hour, *minute as i8)).collect(),
        Cadence::Daily { hour, minute } => vec![(*hour as i8, *minute as i8)],
        Cadence::Weekdays { hour, minute } if weekday < Weekday::Sat => {
            vec![(*hour as i8, *minute as i8)]
        }
        Cadence::Weekly { days, hour, minute } if days.contains(&weekday) => {
            vec![(*hour as i8, *minute as i8)]
        }
        _ => Vec::new(),
    };
    let mut times: Vec<u64> = clocks
        .into_iter()
        .filter_map(|(hour, minute)| {
            let stamp = date
                .at(hour, minute, 0, 0)
                .to_zoned_datetime(zone.clone())
                .ok()?;
            u64::try_from(stamp.timestamp().as_millisecond()).ok()
        })
        .collect();
    times.sort_unstable();
    times.dedup();
    times
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A time as a zone's clock reads it, in milliseconds since the epoch.
    fn at(zone: &str, civil: &str) -> u64 {
        let stamp = civil
            .parse::<jiff::civil::DateTime>()
            .unwrap()
            .to_zoned_datetime(TimeZone::get(zone).unwrap())
            .unwrap();
        stamp.timestamp().as_millisecond() as u64
    }

    fn daily(hour: u8, minute: u8) -> Cadence {
        Cadence::Daily { hour, minute }
    }

    #[test]
    fn a_daily_time_is_due_once_a_day_on_the_clock_of_its_zone() {
        let zone = "Asia/Tokyo";
        let now = at(zone, "2026-03-10T10:00:00");
        assert_eq!(
            latest_in(zone, &daily(9, 0), 0, now),
            Some(at(zone, "2026-03-10T09:00:00"))
        );
        assert_eq!(
            next_after(zone, &daily(9, 0), now),
            Some(at(zone, "2026-03-11T09:00:00"))
        );
        // The same instant is the day before at the same hour in UTC.
        assert_eq!(
            latest_in("UTC", &daily(9, 0), 0, now),
            Some(at("UTC", "2026-03-09T09:00:00"))
        );
    }

    #[test]
    fn a_time_already_taken_is_not_due_again() {
        let zone = "UTC";
        let now = at(zone, "2026-03-10T10:00:00");
        let nine = at(zone, "2026-03-10T09:00:00");
        assert_eq!(latest_in(zone, &daily(9, 0), nine, now), None);
        assert_eq!(latest_in(zone, &daily(9, 0), nine - 1, now), Some(nine));
        assert_eq!(
            latest_in(zone, &daily(11, 0), 0, now),
            Some(at(zone, "2026-03-09T11:00:00"))
        );
    }

    #[test]
    fn a_missed_stretch_of_days_is_due_once_as_its_latest_time() {
        let zone = "UTC";
        let now = at(zone, "2026-03-10T10:00:00");
        let long_ago = at(zone, "2026-03-01T00:00:00");
        assert_eq!(
            latest_in(zone, &daily(9, 0), long_ago, now),
            Some(at(zone, "2026-03-10T09:00:00"))
        );
    }

    #[test]
    fn weekdays_skip_the_weekend_and_weekly_keeps_to_its_days() {
        let zone = "UTC";
        // 2026-03-07 is a Saturday.
        let saturday = at(zone, "2026-03-07T12:00:00");
        let weekdays = Cadence::Weekdays { hour: 9, minute: 0 };
        assert_eq!(
            latest_in(zone, &weekdays, 0, saturday),
            Some(at(zone, "2026-03-06T09:00:00"))
        );
        assert_eq!(
            next_after(zone, &weekdays, saturday),
            Some(at(zone, "2026-03-09T09:00:00"))
        );
        let weekly = Cadence::Weekly {
            days: vec![Weekday::Mon, Weekday::Thu],
            hour: 18,
            minute: 30,
        };
        assert_eq!(
            next_after(zone, &weekly, saturday),
            Some(at(zone, "2026-03-09T18:30:00"))
        );
        assert_eq!(
            latest_in(zone, &weekly, 0, saturday),
            Some(at(zone, "2026-03-05T18:30:00"))
        );
    }

    #[test]
    fn an_hourly_time_is_due_at_its_minute_of_every_hour() {
        let zone = "Asia/Kolkata";
        let hourly = Cadence::Hourly { minute: 15 };
        let now = at(zone, "2026-03-10T10:20:00");
        assert_eq!(
            latest_in(zone, &hourly, 0, now),
            Some(at(zone, "2026-03-10T10:15:00"))
        );
        assert_eq!(
            next_after(zone, &hourly, now),
            Some(at(zone, "2026-03-10T11:15:00"))
        );
    }

    #[test]
    fn a_daily_time_in_the_hour_a_clock_skips_runs_when_the_clock_next_reads_it() {
        // New York put its clocks forward at 02:00 on 2026-03-08.
        let zone = "America/New_York";
        let now = at(zone, "2026-03-08T12:00:00");
        let due = latest_in(zone, &daily(2, 30), 0, now).unwrap();
        assert_eq!(due, at(zone, "2026-03-08T03:30:00"));
        // Once, however often it is asked.
        assert_eq!(latest_in(zone, &daily(2, 30), due, now), None);
        let before = at(zone, "2026-03-07T12:00:00");
        assert_eq!(
            next_after(zone, &daily(2, 30), before),
            Some(at(zone, "2026-03-08T03:30:00"))
        );
        assert_eq!(
            next_after(zone, &daily(2, 30), due),
            Some(at(zone, "2026-03-09T02:30:00"))
        );
    }

    #[test]
    fn a_daily_time_in_the_hour_a_clock_repeats_runs_the_first_time_round() {
        // New York put its clocks back at 02:00 on 2026-11-01, so 01:30 came twice.
        let zone = "America/New_York";
        let first = at(zone, "2026-11-01T01:30:00");
        let second = first + 3_600_000;
        let late = at(zone, "2026-11-01T12:00:00");
        assert_eq!(latest_in(zone, &daily(1, 30), 0, late), Some(first));
        assert_eq!(latest_in(zone, &daily(1, 30), first, second + 60_000), None);
        assert_eq!(
            next_after(zone, &daily(1, 30), first),
            Some(at(zone, "2026-11-02T01:30:00"))
        );
    }

    #[test]
    fn an_hourly_time_runs_once_in_the_hour_a_clock_repeats_and_never_in_the_one_it_skips() {
        let zone = "America/New_York";
        let hourly = Cadence::Hourly { minute: 10 };
        // The clock skipped 02:00–03:00 on 2026-03-08: from 01:10 the next is 03:10.
        assert_eq!(
            next_after(zone, &hourly, at(zone, "2026-03-08T01:10:00")),
            Some(at(zone, "2026-03-08T03:10:00"))
        );
        // And the hour that came twice on 2026-11-01 ran once.
        let first = at(zone, "2026-11-01T01:10:00");
        let after_first = next_after(zone, &hourly, first).unwrap();
        assert_eq!(after_first, at(zone, "2026-11-01T02:10:00"));
    }

    #[test]
    fn a_time_is_written_as_the_clock_of_its_zone_reads_it() {
        let zone = "Asia/Tokyo";
        assert_eq!(
            clock(zone, at(zone, "2026-11-23T09:05:00")),
            "2026-11-23 09:05"
        );
        assert_eq!(
            clock("UTC", at(zone, "2026-11-23T09:05:00")),
            "2026-11-23 00:05"
        );
    }

    #[test]
    fn an_unknown_zone_is_refused_with_what_to_write() {
        assert!(zone("Asia/Tokyo").is_ok());
        let error = zone("Mars/Base").unwrap_err();
        assert!(error.contains("IANA"), "{error}");
        assert_eq!(latest_in("Mars/Base", &daily(9, 0), 0, 1), None);
    }
}
