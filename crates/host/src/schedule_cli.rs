//! `tsumugi schedule`: keep, read and change schedules from a command,
//! through the running work host that runs them. Its words follow Orca's
//! `automations`.

use std::path::{Path, PathBuf};

use serde_json::json;
use tsumugi_model::{
    Cadence, DEFAULT_GRACE_MINUTES, NewSchedule, Schedule, ScheduleEdit, ScheduleRun,
    ScheduleTarget, Weekday,
};

use crate::command::Output;
use crate::delegation::{Code, Error};
use crate::watch::Watched;

/// What a tick says when the command names none: the session it wakes is a
/// supervisor, and its workers' reports reach it as they come.
const TICK_PROMPT: &str = "Check how the sessions you opened stand. Read what changed since the last time, act only on what needs you, and otherwise end your turn.";

const USAGE: &str = "usage: tsumugi schedule <list | show <schedule> | create --name <name> --prompt <text|-> --trigger <hourly|daily|weekdays|weekly|every> [--time HH:MM] [--days mon,thu] [--every-minutes <n>] [--session <code>] [--timezone <zone>] [--dir <path>] [--profile <name>] [--missed-run-grace-minutes <n>] [--disabled] | edit <schedule> [--name …] [--prompt …] [--trigger …] [--time …] [--days …] [--timezone …] [--dir …] [--profile <name>|--default-profile] [--missed-run-grace-minutes <n>] [--enabled|--disabled] | run <schedule> | runs <schedule> | remove <schedule>> [--json] [--instance <name>]";

/// What `tsumugi schedule` was asked.
#[derive(Debug, PartialEq, Eq)]
struct Asked {
    json: bool,
    what: What,
}

#[derive(Debug, PartialEq, Eq)]
enum What {
    List,
    Show(String),
    Create(Fields),
    Edit(String, Fields),
    Run(String),
    Runs(String),
    Remove(String),
}

/// What a command says of a schedule; what it leaves out is none.
#[derive(Debug, Default, PartialEq, Eq)]
struct Fields {
    name: Option<String>,
    /// `-` is the standard input.
    prompt: Option<String>,
    dir: Option<PathBuf>,
    /// `Some(None)` is the default profile.
    profile: Option<Option<String>>,
    timezone: Option<String>,
    trigger: Option<String>,
    time: Option<(u8, u8)>,
    days: Option<Vec<Weekday>>,
    grace: Option<u32>,
    enabled: Option<bool>,
    /// The minutes between the times of an `every` trigger.
    every: Option<u32>,
    /// The session a new schedule wakes instead of queueing requests.
    session: Option<String>,
}

/// `09:30` as an hour and a minute.
fn clock(text: &str) -> Option<(u8, u8)> {
    let (hour, minute) = text.split_once(':')?;
    let (hour, minute) = (hour.parse::<u8>().ok()?, minute.parse::<u8>().ok()?);
    (hour < 24 && minute < 60).then_some((hour, minute))
}

fn parse(args: &[String]) -> Option<Asked> {
    let (verb, mut rest) = args.split_first()?;
    if !matches!(
        verb.as_str(),
        "list" | "show" | "create" | "edit" | "run" | "runs" | "remove"
    ) {
        return None;
    }
    let (mut json, mut fields, mut selector) = (false, Fields::default(), None);
    let writes = matches!(verb.as_str(), "create" | "edit");
    while let Some((word, after)) = rest.split_first() {
        rest = after;
        let mut value = || {
            let (value, after) = rest.split_first()?;
            rest = after;
            Some(value.clone())
        };
        match word.as_str() {
            "--json" if !json => json = true,
            "--instance" => {
                value().filter(|name| crate::paths::is_instance(name))?;
            }
            "--name" if writes && fields.name.is_none() => fields.name = Some(value()?),
            "--prompt" if writes && fields.prompt.is_none() => fields.prompt = Some(value()?),
            "--dir" if writes && fields.dir.is_none() => fields.dir = Some(PathBuf::from(value()?)),
            "--profile" if writes && fields.profile.is_none() => {
                fields.profile = Some(Some(value()?));
            }
            "--default-profile" if verb == "edit" && fields.profile.is_none() => {
                fields.profile = Some(None);
            }
            "--timezone" if writes && fields.timezone.is_none() => {
                fields.timezone = Some(value()?);
            }
            "--trigger" if writes && fields.trigger.is_none() => {
                fields.trigger = Some(value()?.to_ascii_lowercase());
            }
            "--time" if writes && fields.time.is_none() => fields.time = Some(clock(&value()?)?),
            "--days" if writes && fields.days.is_none() => {
                fields.days = Some(
                    value()?
                        .split(',')
                        .map(Weekday::parse)
                        .collect::<Option<Vec<_>>>()?,
                );
            }
            "--every-minutes" if writes && fields.every.is_none() => {
                fields.every = Some(value()?.parse().ok()?);
            }
            "--session" if verb == "create" && fields.session.is_none() => {
                fields.session = Some(value()?.to_ascii_lowercase());
            }
            "--missed-run-grace-minutes" if writes && fields.grace.is_none() => {
                fields.grace = Some(value()?.parse().ok()?);
            }
            "--enabled" if verb == "edit" && fields.enabled.is_none() => {
                fields.enabled = Some(true);
            }
            "--disabled" if writes && fields.enabled.is_none() => fields.enabled = Some(false),
            name if name.starts_with("--") => return None,
            word if verb != "list" && verb != "create" && selector.is_none() => {
                selector = Some(word.to_owned());
            }
            _ => return None,
        }
    }
    let what = match verb.as_str() {
        "list" => What::List,
        "create" => {
            // Orca's `--disabled` is what a schedule is made as; enabling is
            // for `edit`.
            if fields.enabled == Some(true) {
                return None;
            }
            What::Create(fields)
        }
        "edit" => What::Edit(selector?, fields),
        "show" => What::Show(selector?),
        "run" => What::Run(selector?),
        "runs" => What::Runs(selector?),
        _ => What::Remove(selector?),
    };
    Some(Asked { json, what })
}

/// The cadence a command asks for, over `kept` when it changes one.
fn cadence(fields: &Fields, kept: Option<&Cadence>) -> Result<Option<Cadence>, Error> {
    if fields.trigger.is_none()
        && fields.time.is_none()
        && fields.days.is_none()
        && fields.every.is_none()
    {
        return Ok(None);
    }
    let trigger = fields
        .trigger
        .as_deref()
        .or(kept.map(Cadence::preset))
        .ok_or_else(|| {
            refused("Say when it runs: --trigger hourly, daily, weekdays, weekly or every")
        })?;
    let (kept_hour, kept_minute) = match kept {
        Some(
            Cadence::Daily { hour, minute }
            | Cadence::Weekdays { hour, minute }
            | Cadence::Weekly { hour, minute, .. },
        ) => (*hour, *minute),
        Some(Cadence::Hourly { minute }) => (9, *minute),
        Some(Cadence::Every { .. }) => (9, 0),
        None => (9, 0),
    };
    let (hour, minute) = fields.time.unwrap_or((kept_hour, kept_minute));
    let cadence = match trigger {
        "every" => Cadence::Every {
            minutes: fields
                .every
                .or(match kept {
                    Some(Cadence::Every { minutes }) => Some(*minutes),
                    _ => None,
                })
                .ok_or_else(|| refused("Say how often: --every-minutes <n>"))?,
        },
        "hourly" => Cadence::Hourly { minute },
        "daily" => Cadence::Daily { hour, minute },
        "weekdays" => Cadence::Weekdays { hour, minute },
        "weekly" => Cadence::Weekly {
            days: fields
                .days
                .clone()
                .or_else(|| match kept {
                    Some(Cadence::Weekly { days, .. }) => Some(days.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| vec![Weekday::Mon]),
            hour,
            minute,
        },
        other => {
            return Err(refused(format!(
                "`{other}` is not a trigger Tsumugi runs; use hourly, daily, weekdays, weekly or every. Cron and RRULE are not read"
            )));
        }
    };
    if fields.days.is_some() && trigger != "weekly" {
        return Err(refused("--days belongs to a weekly trigger"));
    }
    if fields.every.is_some() && trigger != "every" {
        return Err(refused("--every-minutes belongs to the every trigger"));
    }
    Ok(Some(cadence))
}

/// `tsumugi schedule <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    let Some(asked) = parse(args) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let input = match &asked.what {
        What::Create(Fields { prompt, .. }) | What::Edit(_, Fields { prompt, .. })
            if prompt.as_deref() == Some("-") =>
        {
            let mut text = String::new();
            if let Err(error) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text) {
                eprintln!("tsumugi: cannot read the prompt: {error}");
                return 1;
            }
            Some(text)
        }
        _ => None,
    };
    let mut ask = vec!["schedule".to_owned()];
    ask.extend(args.iter().cloned());
    crate::work_host::run_command(ask, input)
}

/// `tsumugi schedule <args>` as the work host runs it for a command run in
/// `cwd` with `input` on its standard input.
pub(crate) async fn run(
    watched: &Watched,
    args: &[String],
    cwd: &Path,
    input: Option<String>,
) -> Output {
    let Some(asked) = parse(args) else {
        return Output {
            wrote: Err(USAGE.into()),
            code: 2,
            effects_possible: false,
        };
    };
    let effects_possible = !matches!(asked.what, What::List | What::Show(_) | What::Runs(_));
    let json = asked.json;
    match carry_out(watched, asked.what, cwd, input).await {
        Ok(shown) => Output {
            wrote: Ok(shown.render(json)),
            code: 0,
            effects_possible,
        },
        Err(error) if json => Output {
            wrote: Ok(
                json!({"ok": false, "error": {"code": error.code, "message": error.message}})
                    .to_string(),
            ),
            code: 1,
            effects_possible,
        },
        Err(error) => Output {
            wrote: Err(error.message),
            code: 1,
            effects_possible,
        },
    }
}

/// What a command shows once it is done.
enum Shown {
    Schedules(Vec<Schedule>),
    One(Schedule),
    Runs(Schedule, Vec<ScheduleRun>),
    Queued(Schedule, ScheduleRun),
    /// A tick is held for the session of a schedule run by hand.
    Held(Schedule),
    Removed(Schedule),
}

async fn carry_out(
    watched: &Watched,
    what: What,
    cwd: &Path,
    input: Option<String>,
) -> Result<Shown, Error> {
    use crate::delegation::app::{root, with_store};
    let now = tsumugi_model::time::now_ms();
    let prompt_of = |prompt: Option<String>| match prompt.as_deref() {
        Some("-") => input.clone(),
        _ => prompt,
    };
    match what {
        What::List => Ok(Shown::Schedules(
            with_store(&root(), move |store| crate::schedules::listed(store, now)).await?,
        )),
        What::Show(selector) => Ok(Shown::One(named(&selector).await?)),
        What::Runs(selector) => {
            let schedule = named(&selector).await?;
            let id = schedule.id.clone();
            let runs = with_store(&root(), move |store| store.schedule_runs(&id, 50)).await?;
            Ok(Shown::Runs(schedule, runs))
        }
        What::Create(fields) => {
            let cadence = cadence(&fields, None)?.ok_or_else(|| {
                refused("Say when it runs: --trigger hourly, daily, weekdays, weekly or every")
            })?;
            let target = match &fields.session {
                Some(code) => {
                    let kept = code.clone();
                    let known = with_store(&root(), move |store| {
                        Ok(kept
                            .parse::<tsumugi_model::SessionCode>()
                            .ok()
                            .map(|code| store.by_code(&code))
                            .transpose()?
                            .flatten())
                    })
                    .await?;
                    if !known.is_some_and(|session| !session.ended) {
                        return Err(refused(format!(
                            "No session that runs has the code {code}; `tsumugi session list` shows them"
                        )));
                    }
                    ScheduleTarget::Session { code: code.clone() }
                }
                None => ScheduleTarget::Queue,
            };
            let new = NewSchedule {
                id: tsumugi_model::SessionId::new().to_string(),
                name: fields
                    .name
                    .ok_or_else(|| refused("A schedule needs a --name"))?,
                prompt: prompt_of(fields.prompt)
                    .or_else(|| fields.session.as_ref().map(|_| TICK_PROMPT.to_owned()))
                    .ok_or_else(|| refused("A schedule needs a --prompt"))?,
                dir: resolved(cwd, &fields.dir.unwrap_or_default()),
                profile: fields.profile.flatten(),
                timezone: fields
                    .timezone
                    .unwrap_or_else(crate::schedule_time::system_zone),
                cadence,
                target,
                grace_minutes: fields.grace.unwrap_or(DEFAULT_GRACE_MINUTES),
            };
            let id = new.id.clone();
            crate::schedules::save(watched, new)
                .await
                .map_err(refused)?;
            Ok(Shown::One(named(&id).await?))
        }
        What::Edit(selector, fields) => {
            let kept = named(&selector).await?;
            let new_cadence = cadence(&fields, Some(&kept.cadence))?;
            let edit = ScheduleEdit {
                name: fields.name,
                prompt: prompt_of(fields.prompt),
                dir: fields.dir.map(|dir| resolved(cwd, &dir)),
                profile: fields.profile,
                timezone: fields.timezone,
                cadence: new_cadence,
                grace_minutes: fields.grace,
            };
            let id = kept.id.clone();
            crate::schedules::edit(watched, id.clone(), edit)
                .await
                .map_err(refused)?;
            if let Some(on) = fields.enabled.filter(|on| *on != kept.enabled) {
                crate::schedules::enable(watched, id.clone(), on)
                    .await
                    .map_err(refused)?;
            }
            Ok(Shown::One(named(&id).await?))
        }
        What::Run(selector) => {
            let (schedule, run) = crate::schedules::run_by_name(watched, &selector).await?;
            Ok(match run {
                Some(run) => Shown::Queued(schedule, run),
                None => Shown::Held(schedule),
            })
        }
        What::Remove(selector) => {
            let schedule = named(&selector).await?;
            crate::schedules::delete(watched, schedule.id.clone())
                .await
                .map_err(refused)?;
            Ok(Shown::Removed(schedule))
        }
    }
}

/// The schedule `selector` names, with when it is next due.
async fn named(selector: &str) -> Result<Schedule, Error> {
    use crate::delegation::app::{root, with_store};
    let selector = selector.to_owned();
    let now = tsumugi_model::time::now_ms();
    with_store(&root(), move |store| {
        let (_, schedule) = store.schedule_named(&selector)?;
        crate::schedules::listed(store, now)?
            .into_iter()
            .find(|listed| listed.id == schedule.id)
            .ok_or_else(|| Error::new(Code::NotFound, "The schedule is gone"))
    })
    .await
}

fn refused(message: impl Into<String>) -> Error {
    Error::new(Code::Invalid, message)
}

/// `dir` as a directory of this machine, taken from `cwd` when relative.
fn resolved(cwd: &Path, dir: &Path) -> PathBuf {
    let dir = cwd.join(dir);
    tsumugi_sys::fs::canonicalize(&dir).unwrap_or(dir)
}

impl Shown {
    fn render(&self, json: bool) -> String {
        let view = crate::schedules::view;
        if json {
            let result = match self {
                Self::Schedules(all) => {
                    json!({"schedules": all.iter().map(view).collect::<Vec<_>>()})
                }
                Self::One(schedule) => json!({"schedule": view(schedule)}),
                Self::Runs(schedule, runs) => json!({"schedule": view(schedule), "runs": runs}),
                Self::Queued(schedule, run) => json!({"schedule": view(schedule), "run": run}),
                Self::Held(schedule) => json!({"schedule": view(schedule), "held": true}),
                Self::Removed(schedule) => json!({"schedule": view(schedule), "removed": true}),
            };
            return json!({"ok": true, "result": result}).to_string();
        }
        match self {
            Self::Schedules(all) if all.is_empty() => "No schedules".into(),
            Self::Schedules(all) => all.iter().map(line).collect::<Vec<_>>().join("\n"),
            Self::One(schedule) => detail(schedule),
            Self::Runs(_, runs) if runs.is_empty() => "No time has come due yet".into(),
            Self::Runs(schedule, runs) => runs
                .iter()
                .map(|run| run_line(schedule, run))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Queued(schedule, run) => format!(
                "#{} queued issue {}",
                schedule.number,
                run.issue.as_deref().unwrap_or("")
            ),
            Self::Held(schedule) => format!(
                "#{} holds a tick; the session gets it once it waits for input",
                schedule.number
            ),
            Self::Removed(schedule) => format!("#{} removed  {}", schedule.number, schedule.name),
        }
    }
}

fn state_word(schedule: &Schedule) -> &'static str {
    if schedule.enabled {
        "enabled"
    } else {
        "paused"
    }
}

fn outcome_word(run: &ScheduleRun) -> &'static str {
    run.outcome.word()
}

fn line(schedule: &Schedule) -> String {
    let next = schedule.next_ms.map_or_else(
        || "-".to_owned(),
        |ms| crate::schedule_time::clock(&schedule.timezone, ms),
    );
    let last = schedule.last.as_ref().map_or("-", outcome_word);
    let wakes = match &schedule.target {
        ScheduleTarget::Session { code } => format!("  -> {code}"),
        ScheduleTarget::Queue => String::new(),
    };
    let held = if schedule.held_ms.is_some() {
        "  held"
    } else {
        ""
    };
    format!(
        "#{}  {:<7}  {} {}  next {next}  last {last}{held}  {}{wakes}",
        schedule.number,
        state_word(schedule),
        schedule.cadence.describe(),
        schedule.timezone,
        schedule.name
    )
}

fn run_line(schedule: &Schedule, run: &ScheduleRun) -> String {
    let mut text = format!(
        "{}  {}",
        crate::schedule_time::clock(&schedule.timezone, run.due_ms),
        outcome_word(run)
    );
    if let Some(issue) = &run.issue {
        text.push_str(&format!("  issue {issue}"));
    }
    if let Some(note) = &run.note {
        text.push_str(&format!("  {note}"));
    }
    text
}

fn detail(schedule: &Schedule) -> String {
    let mut text = format!(
        "#{} {}\nstate: {}\nwhen: {} ({})\ndir: {}\nid: {}",
        schedule.number,
        schedule.name,
        state_word(schedule),
        schedule.cadence.describe(),
        schedule.timezone,
        schedule.dir.display(),
        schedule.id
    );
    match &schedule.target {
        ScheduleTarget::Queue => text.push_str(&format!(
            "\nprofile: {}",
            schedule.profile.as_deref().unwrap_or("default")
        )),
        ScheduleTarget::Session { code } => text.push_str(&format!("\nwakes: session {code}")),
    }
    text.push_str(&format!(
        "\nmissed-run grace: {} min",
        schedule.grace_minutes
    ));
    if let Some(next) = schedule.next_ms {
        text.push_str(&format!(
            "\nnext: {}",
            crate::schedule_time::clock(&schedule.timezone, next)
        ));
    }
    if let Some(held) = schedule.held_ms {
        text.push_str(&format!(
            "\nheld: {} (handed over once the session waits for input)",
            crate::schedule_time::clock(&schedule.timezone, held)
        ));
    }
    if let Some(last) = &schedule.last {
        text.push_str(&format!("\nlast: {}", run_line(schedule, last)));
    }
    text.push_str(&format!("\n\n{}", schedule.prompt));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tsumugi_model::RunOutcome;

    fn asked(words: &[&str]) -> Option<What> {
        let args: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        parse(&args).map(|asked| asked.what)
    }

    fn kept() -> Schedule {
        Schedule {
            id: "ab12".to_owned() + &"0".repeat(28),
            number: 3,
            name: "Weekday triage".into(),
            prompt: "Triage new issues".into(),
            dir: "/project".into(),
            profile: None,
            timezone: "Asia/Tokyo".into(),
            cadence: Cadence::Weekdays { hour: 9, minute: 0 },
            target: ScheduleTarget::Queue,
            enabled: true,
            grace_minutes: 720,
            next_ms: Some(0),
            last: Some(ScheduleRun {
                due_ms: 0,
                at_ms: 1,
                outcome: RunOutcome::SkippedBusy,
                issue: None,
                note: Some("The previous request of this schedule has not ended".into()),
            }),
            held_ms: None,
        }
    }

    #[test]
    fn a_schedule_is_created_as_orca_creates_an_automation() {
        let Some(What::Create(fields)) = asked(&[
            "create",
            "--name",
            "Weekday triage",
            "--trigger",
            "weekdays",
            "--time",
            "09:00",
            "--prompt",
            "Triage",
            "--profile",
            "codex",
            "--timezone",
            "Asia/Tokyo",
            "--disabled",
        ]) else {
            panic!("create is understood");
        };
        assert_eq!(fields.trigger.as_deref(), Some("weekdays"));
        assert_eq!(fields.time, Some((9, 0)));
        assert_eq!(fields.profile, Some(Some("codex".into())));
        assert_eq!(fields.enabled, Some(false));
        assert!(
            asked(&["create", "--name", "x", "--enabled"]).is_none(),
            "made paused"
        );
        assert!(asked(&["create", "--time", "25:00"]).is_none());
        assert!(asked(&["create", "--days", "someday"]).is_none());
    }

    #[test]
    fn editing_names_a_schedule_and_may_enable_or_pause_it() {
        let Some(What::Edit(selector, fields)) = asked(&["edit", "3", "--enabled"]) else {
            panic!("edit is understood");
        };
        assert_eq!((selector.as_str(), fields.enabled), ("3", Some(true)));
        assert!(asked(&["edit", "--enabled"]).is_none(), "which schedule?");
        let Some(What::Edit(_, fields)) = asked(&["edit", "3", "--default-profile"]) else {
            panic!("edit is understood");
        };
        assert_eq!(fields.profile, Some(None));
    }

    #[test]
    fn the_other_verbs_name_one_schedule_and_nothing_more() {
        for verb in ["show", "run", "runs", "remove"] {
            assert!(asked(&[verb, "3"]).is_some(), "{verb}");
            assert!(asked(&[verb]).is_none(), "{verb}");
            assert!(asked(&[verb, "3", "4"]).is_none(), "{verb}");
        }
        assert_eq!(asked(&["list"]), Some(What::List));
        assert!(asked(&["list", "3"]).is_none());
        assert!(asked(&["pause", "3"]).is_none());
        assert!(asked(&["list", "--verbose"]).is_none());
        assert!(asked(&[]).is_none());
    }

    #[test]
    fn a_trigger_makes_a_cadence_and_an_edit_keeps_what_it_leaves_out() {
        let make = |words: &[&str], kept: Option<&Cadence>| {
            let mut args = vec!["create"];
            args.extend(words);
            let Some(What::Create(fields)) = asked(&args) else {
                panic!("understood");
            };
            cadence(&fields, kept)
        };
        assert_eq!(
            make(&["--trigger", "daily", "--time", "07:30"], None).unwrap(),
            Some(Cadence::Daily {
                hour: 7,
                minute: 30
            })
        );
        assert_eq!(
            make(&["--trigger", "weekly", "--days", "mon,thu"], None).unwrap(),
            Some(Cadence::Weekly {
                days: vec![Weekday::Mon, Weekday::Thu],
                hour: 9,
                minute: 0
            })
        );
        assert_eq!(
            make(&["--trigger", "hourly", "--time", "00:20"], None).unwrap(),
            Some(Cadence::Hourly { minute: 20 })
        );
        let weekdays = Cadence::Weekdays {
            hour: 18,
            minute: 0,
        };
        assert_eq!(
            make(&["--time", "08:15"], Some(&weekdays)).unwrap(),
            Some(Cadence::Weekdays {
                hour: 8,
                minute: 15
            })
        );
        assert_eq!(make(&["--name", "x"], Some(&weekdays)).unwrap(), None);
        assert!(make(&["--time", "08:15"], None).is_err(), "when, exactly?");
        let error = make(&["--trigger", "0 18 * * 1-5"], None).unwrap_err();
        assert!(error.message.contains("Cron"), "{}", error.message);
        assert!(make(&["--trigger", "daily", "--days", "mon"], None).is_err());
    }

    #[test]
    fn a_tick_for_a_supervisor_is_made_with_an_interval_and_the_session_it_wakes() {
        let Some(What::Create(fields)) = asked(&[
            "create",
            "--name",
            "Supervise",
            "--trigger",
            "every",
            "--every-minutes",
            "30",
            "--session",
            "ab12",
        ]) else {
            panic!("create is understood");
        };
        assert_eq!(fields.session.as_deref(), Some("ab12"));
        assert_eq!(
            cadence(&fields, None).unwrap(),
            Some(Cadence::Every { minutes: 30 })
        );
        // A session is named when the schedule is made, not changed after.
        assert!(asked(&["edit", "3", "--session", "ab12"]).is_none());
        // The interval is read with its trigger, and no other.
        let make = |words: &[&str], kept: Option<&Cadence>| {
            let mut args = vec!["create"];
            args.extend(words);
            let Some(What::Create(fields)) = asked(&args) else {
                panic!("understood");
            };
            cadence(&fields, kept)
        };
        assert!(make(&["--trigger", "every"], None).is_err(), "how often?");
        assert!(make(&["--trigger", "daily", "--every-minutes", "30"], None).is_err());
        assert_eq!(
            make(&["--every-minutes", "60"], Some(&Cadence::Every { minutes: 30 })).unwrap(),
            Some(Cadence::Every { minutes: 60 })
        );
        assert!(asked(&["create", "--every-minutes", "soon"]).is_none());
    }

    #[test]
    fn a_tick_schedule_says_whom_it_wakes_and_whether_a_tick_is_held() {
        let mut schedule = kept();
        schedule.cadence = Cadence::Every { minutes: 30 };
        schedule.target = ScheduleTarget::Session {
            code: "ab12".into(),
        };
        schedule.held_ms = Some(0);
        let shown = Shown::One(schedule.clone()).render(false);
        assert!(shown.contains("wakes: session ab12"), "{shown}");
        assert!(shown.contains("held: 1970-01-01 09:00"), "{shown}");
        assert!(shown.contains("when: every 30 min"), "{shown}");
        assert!(!shown.contains("profile:"), "{shown}");
        let listed = Shown::Schedules(vec![schedule.clone()]).render(false);
        assert!(listed.contains("-> ab12") && listed.contains("held"), "{listed}");
        assert_eq!(
            Shown::Held(schedule).render(false),
            "#3 holds a tick; the session gets it once it waits for input"
        );
    }

    #[test]
    fn schedules_are_listed_one_to_a_line_and_shown_with_their_prompt() {
        let schedule = kept();
        assert_eq!(
            Shown::Schedules(vec![schedule.clone()]).render(false),
            "#3  enabled  weekdays 09:00 Asia/Tokyo  next 1970-01-01 09:00  last skipped_busy  Weekday triage"
        );
        assert_eq!(Shown::Schedules(vec![]).render(false), "No schedules");
        let shown = Shown::One(schedule.clone()).render(false);
        assert!(
            shown.starts_with(
                "#3 Weekday triage\nstate: enabled\nwhen: weekdays 09:00 (Asia/Tokyo)"
            )
        );
        assert!(shown.contains("profile: default") && shown.contains("missed-run grace: 720 min"));
        assert!(shown.contains("last: 1970-01-01 09:00  skipped_busy  The previous request"));
        assert!(shown.ends_with("\n\nTriage new issues"));
        let runs = Shown::Runs(schedule, vec![]).render(false);
        assert_eq!(runs, "No time has come due yet");
    }

    #[test]
    fn json_wraps_a_result_the_way_a_script_reads_it() {
        let listed: Value =
            serde_json::from_str(&Shown::Schedules(vec![kept()]).render(true)).unwrap();
        assert_eq!(listed["ok"], true);
        let schedule = &listed["result"]["schedules"][0];
        assert_eq!(schedule["number"], 3);
        assert_eq!(schedule["enabled"], true);
        assert_eq!(schedule["timezone"], "Asia/Tokyo");
        assert_eq!(schedule["cadence"]["kind"], "weekdays");
        assert_eq!(schedule["last"]["outcome"], "skipped_busy");
        let removed: Value = serde_json::from_str(&Shown::Removed(kept()).render(true)).unwrap();
        assert_eq!(removed["result"]["removed"], true);
    }
}
