//! A request made again on a timetable. A schedule queues an ordinary local
//! issue at each time it is due, once the user has enabled it; it saves
//! nothing the user did not write and starts nothing while it is disabled.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const MAX_SCHEDULE_NAME: usize = 256;
pub const MAX_SCHEDULE_PROMPT: usize = 65536;
/// How late a missed time may still run, unless the schedule says otherwise.
pub const DEFAULT_GRACE_MINUTES: u32 = 720;
/// The longest a missed time may still run: a week.
pub const MAX_GRACE_MINUTES: u32 = 7 * 24 * 60;

/// A day of the week, Monday first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub const ALL: [Self; 7] = [
        Self::Mon,
        Self::Tue,
        Self::Wed,
        Self::Thu,
        Self::Fri,
        Self::Sat,
        Self::Sun,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Self::Mon => "mon",
            Self::Tue => "tue",
            Self::Wed => "wed",
            Self::Thu => "thu",
            Self::Fri => "fri",
            Self::Sat => "sat",
            Self::Sun => "sun",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|day| day.word() == word)
    }

    /// 1 for Monday to 7 for Sunday, as ISO 8601 counts.
    pub fn number(self) -> u8 {
        Self::ALL.iter().position(|day| *day == self).unwrap_or(0) as u8 + 1
    }
}

/// When a schedule is due, on the clock of its time zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cadence {
    /// At `minute` past every hour.
    Hourly {
        minute: u8,
    },
    Daily {
        hour: u8,
        minute: u8,
    },
    /// Monday to Friday.
    Weekdays {
        hour: u8,
        minute: u8,
    },
    Weekly {
        days: Vec<Weekday>,
        hour: u8,
        minute: u8,
    },
}

impl Cadence {
    /// What is wrong with the cadence, if anything.
    pub fn refuse(&self) -> Option<&'static str> {
        let (hour, minute) = match self {
            Self::Hourly { minute } => (0, *minute),
            Self::Daily { hour, minute }
            | Self::Weekdays { hour, minute }
            | Self::Weekly { hour, minute, .. } => (*hour, *minute),
        };
        if hour > 23 || minute > 59 {
            return Some("A time is between 00:00 and 23:59");
        }
        if let Self::Weekly { days, .. } = self
            && days.is_empty()
        {
            return Some("A weekly schedule needs at least one day");
        }
        None
    }

    /// The cadence a person writes as a preset word, a `HH:MM` time (only
    /// the minutes of it for an hourly one) and, for a weekly one, days such
    /// as `mon,thu` (Monday when none).
    pub fn compose(preset: &str, time: &str, days: &str) -> Result<Self, String> {
        let time = time.trim();
        let (hour, minute) = if time.is_empty() {
            (9, 0)
        } else {
            time.split_once(':')
                .and_then(|(h, m)| Some((h.parse::<u8>().ok()?, m.parse::<u8>().ok()?)))
                .ok_or("Write the time as HH:MM, such as 09:30")?
        };
        let cadence = match preset.trim().to_ascii_lowercase().as_str() {
            "hourly" => Self::Hourly { minute },
            "daily" => Self::Daily { hour, minute },
            "weekdays" => Self::Weekdays { hour, minute },
            "weekly" => Self::Weekly {
                days: if days.trim().is_empty() {
                    vec![Weekday::Mon]
                } else {
                    days.split(',')
                        .map(Weekday::parse)
                        .collect::<Option<Vec<_>>>()
                        .ok_or("Write the days as mon,thu")?
                },
                hour,
                minute,
            },
            other => {
                return Err(format!(
                    "`{other}` is not a trigger; use hourly, daily, weekdays or weekly"
                ));
            }
        };
        match cadence.refuse() {
            Some(reason) => Err(reason.to_owned()),
            None => Ok(cadence),
        }
    }

    /// The word the preset goes by.
    pub fn preset(&self) -> &'static str {
        match self {
            Self::Hourly { .. } => "hourly",
            Self::Daily { .. } => "daily",
            Self::Weekdays { .. } => "weekdays",
            Self::Weekly { .. } => "weekly",
        }
    }

    /// The cadence as a person reads it: `daily 09:00`, `weekly mon,thu 18:30`.
    pub fn describe(&self) -> String {
        match self {
            Self::Hourly { minute } => format!("hourly at :{minute:02}"),
            Self::Daily { hour, minute } => format!("daily {hour:02}:{minute:02}"),
            Self::Weekdays { hour, minute } => format!("weekdays {hour:02}:{minute:02}"),
            Self::Weekly { days, hour, minute } => {
                let days: Vec<_> = days.iter().map(|day| day.word()).collect();
                format!("weekly {} {hour:02}:{minute:02}", days.join(","))
            }
        }
    }
}

/// What a schedule does each time it is due.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleTarget {
    /// Queue a local issue, which waits its turn in its directory.
    Queue,
    /// Hand the prompt to a session that is running, when it waits for input.
    Session { code: String },
}

/// A schedule to keep. It is kept disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSchedule {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub dir: PathBuf,
    /// The profile that works each request; none for the default profile.
    pub profile: Option<String>,
    /// An IANA time zone name, in which the cadence is read.
    pub timezone: String,
    pub cadence: Cadence,
    pub target: ScheduleTarget,
    pub grace_minutes: u32,
}

/// A change to a schedule's definition; what is none stays as it was.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleEdit {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub dir: Option<PathBuf>,
    /// `Some(None)` returns to the default profile.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub profile: Option<Option<String>>,
    pub timezone: Option<String>,
    pub cadence: Option<Cadence>,
    pub grace_minutes: Option<u32>,
}

/// A field that is there, even as null, is a change; one that is not is none.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// How a time at which a schedule was due ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// A local issue was queued.
    Queued,
    /// The previous request of the schedule had not ended, so none was added.
    SkippedBusy,
    /// The time had passed by more than the schedule's grace.
    SkippedMissed,
    /// Nothing could be queued; the note says what to put right.
    Failed,
}

impl RunOutcome {
    pub const ALL: [Self; 4] = [
        Self::Queued,
        Self::SkippedBusy,
        Self::SkippedMissed,
        Self::Failed,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::SkippedBusy => "skipped_busy",
            Self::SkippedMissed => "skipped_missed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|outcome| outcome.word() == word)
    }
}

/// One time a schedule was due, and what came of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleRun {
    /// When it was due, in milliseconds since the Unix epoch.
    pub due_ms: u64,
    /// When the host settled it.
    pub at_ms: u64,
    pub outcome: RunOutcome,
    /// The local issue it queued.
    pub issue: Option<String>,
    pub note: Option<String>,
}

/// A schedule as the host keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    /// The short number a person types, never given to another schedule.
    pub number: u64,
    pub name: String,
    pub prompt: String,
    pub dir: PathBuf,
    pub profile: Option<String>,
    pub timezone: String,
    pub cadence: Cadence,
    pub target: ScheduleTarget,
    pub enabled: bool,
    pub grace_minutes: u32,
    /// When it is next due, while it is enabled.
    pub next_ms: Option<u64>,
    /// How the time it was last due ended.
    pub last: Option<ScheduleRun>,
}

/// What is wrong with a schedule to keep, if anything. The time zone is the
/// work host's to check, which holds the zone database.
pub fn refuse_schedule(new: &NewSchedule) -> Option<&'static str> {
    refuse_fields(
        Some(&new.name),
        Some(&new.prompt),
        Some(&new.dir),
        Some(&new.cadence),
        Some(new.grace_minutes),
    )
}

/// What is wrong with a change to a schedule, if anything.
pub fn refuse_edit(edit: &ScheduleEdit) -> Option<&'static str> {
    refuse_fields(
        edit.name.as_ref(),
        edit.prompt.as_ref(),
        edit.dir.as_ref(),
        edit.cadence.as_ref(),
        edit.grace_minutes,
    )
}

fn refuse_fields(
    name: Option<&String>,
    prompt: Option<&String>,
    dir: Option<&PathBuf>,
    cadence: Option<&Cadence>,
    grace: Option<u32>,
) -> Option<&'static str> {
    if let Some(name) = name
        && (name.trim().is_empty() || name.len() > MAX_SCHEDULE_NAME || name.contains(['\n', '\r']))
    {
        return Some("A schedule needs a one-line name of 1–256 bytes");
    }
    if let Some(prompt) = prompt
        && (prompt.trim().is_empty() || prompt.len() > MAX_SCHEDULE_PROMPT)
    {
        return Some("A schedule needs a prompt of 1–65536 bytes");
    }
    if dir.is_some_and(|dir| !dir.is_absolute()) {
        return Some("A schedule needs an absolute working directory");
    }
    if let Some(reason) = cadence.and_then(Cadence::refuse) {
        return Some(reason);
    }
    if grace.is_some_and(|minutes| minutes == 0 || minutes > MAX_GRACE_MINUTES) {
        return Some("The grace for a missed time is 1 minute to 7 days");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_weekday_and_outcome_is_read_back_from_its_word() {
        for day in Weekday::ALL {
            assert_eq!(Weekday::parse(day.word()), Some(day));
        }
        assert_eq!(Weekday::parse(" MON "), Some(Weekday::Mon));
        assert_eq!(Weekday::parse("monday"), None);
        assert_eq!(Weekday::Mon.number(), 1);
        assert_eq!(Weekday::Sun.number(), 7);
        for outcome in RunOutcome::ALL {
            assert_eq!(RunOutcome::parse(outcome.word()), Some(outcome));
        }
    }

    #[test]
    fn a_cadence_needs_a_real_time_and_a_weekly_one_a_day() {
        assert_eq!(Cadence::Daily { hour: 9, minute: 0 }.refuse(), None);
        assert!(
            Cadence::Daily {
                hour: 24,
                minute: 0
            }
            .refuse()
            .is_some()
        );
        assert!(Cadence::Hourly { minute: 60 }.refuse().is_some());
        let weekly = |days: Vec<Weekday>| Cadence::Weekly {
            days,
            hour: 9,
            minute: 5,
        };
        assert!(weekly(vec![]).refuse().is_some());
        assert_eq!(weekly(vec![Weekday::Mon]).refuse(), None);
    }

    #[test]
    fn a_cadence_is_composed_from_the_words_a_person_writes() {
        assert_eq!(
            Cadence::compose("daily", "07:30", ""),
            Ok(Cadence::Daily {
                hour: 7,
                minute: 30
            })
        );
        assert_eq!(
            Cadence::compose("Weekly", "", "mon, thu"),
            Ok(Cadence::Weekly {
                days: vec![Weekday::Mon, Weekday::Thu],
                hour: 9,
                minute: 0
            })
        );
        assert_eq!(
            Cadence::compose("hourly", "00:20", ""),
            Ok(Cadence::Hourly { minute: 20 })
        );
        assert!(Cadence::compose("daily", "7", "").is_err());
        assert!(Cadence::compose("daily", "24:00", "").is_err());
        assert!(Cadence::compose("weekly", "09:00", "someday").is_err());
        assert!(
            Cadence::compose("0 9 * * *", "", "")
                .unwrap_err()
                .contains("hourly")
        );
    }

    #[test]
    fn a_cadence_is_described_as_a_person_would_say_it() {
        assert_eq!(Cadence::Hourly { minute: 5 }.describe(), "hourly at :05");
        assert_eq!(
            Cadence::Weekdays { hour: 9, minute: 0 }.describe(),
            "weekdays 09:00"
        );
        assert_eq!(
            Cadence::Weekly {
                days: vec![Weekday::Mon, Weekday::Thu],
                hour: 18,
                minute: 30
            }
            .describe(),
            "weekly mon,thu 18:30"
        );
    }

    fn new() -> NewSchedule {
        NewSchedule {
            id: "0".repeat(32),
            name: "Triage".into(),
            prompt: "Triage new issues".into(),
            dir: "/work".into(),
            profile: None,
            timezone: "UTC".into(),
            cadence: Cadence::Daily { hour: 9, minute: 0 },
            target: ScheduleTarget::Queue,
            grace_minutes: DEFAULT_GRACE_MINUTES,
        }
    }

    #[test]
    fn a_schedule_is_refused_for_what_cannot_be_run() {
        assert_eq!(refuse_schedule(&new()), None);
        let mut bad = new();
        bad.name = " ".into();
        assert!(refuse_schedule(&bad).is_some());
        let mut bad = new();
        bad.prompt = String::new();
        assert!(refuse_schedule(&bad).is_some());
        let mut bad = new();
        bad.dir = "relative".into();
        assert!(refuse_schedule(&bad).is_some());
        let mut bad = new();
        bad.grace_minutes = 0;
        assert!(refuse_schedule(&bad).is_some());
        bad.grace_minutes = MAX_GRACE_MINUTES + 1;
        assert!(refuse_schedule(&bad).is_some());
    }

    #[test]
    fn a_change_is_judged_only_in_what_it_changes() {
        assert_eq!(refuse_edit(&ScheduleEdit::default()), None);
        let edit = ScheduleEdit {
            cadence: Some(Cadence::Hourly { minute: 99 }),
            ..Default::default()
        };
        assert!(refuse_edit(&edit).is_some());
    }
}
