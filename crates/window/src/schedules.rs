//! The environment's schedules, in the issue sheet: what each runs and when,
//! how its last time ended, and the switch that allows it to run.
use dioxus::prelude::*;
use tsumugi_model::{Cadence, NewSchedule, Schedule, ScheduleTarget, SessionId, time};
use tsumugi_protocol::{Answer, Ask};

use crate::components::Button;

#[component]
pub(crate) fn Schedules(
    environment: tsumugi_model::EnvironmentId,
    schedules: Vec<Schedule>,
    dir: String,
) -> Element {
    let app = use_context::<crate::bridge::AppCtx>();
    let mut id = use_signal(|| SessionId::new().to_string());
    let mut name = use_signal(String::new);
    let mut prompt = use_signal(String::new);
    let mut trigger = use_signal(|| "daily".to_owned());
    let mut at = use_signal(|| "09:00".to_owned());
    let mut days = use_signal(String::new);
    let mut zone = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mutate = {
        let environment = environment.clone();
        EventHandler::new(move |(ask, created): (Ask, bool)| {
            if busy() {
                return;
            }
            busy.set(true);
            error.set(None);
            let answer = app.ask_in(&environment, ask);
            spawn(async move {
                match answer.await {
                    Answer::Done => {
                        if created {
                            id.set(SessionId::new().to_string());
                            name.set(String::new());
                            prompt.set(String::new());
                        }
                    }
                    Answer::Failed { text } | Answer::Unconfirmed { text } => error.set(Some(text)),
                    _ => error.set(Some(
                        "The work host did not confirm the schedule change".into(),
                    )),
                }
                busy.set(false);
            });
        })
    };
    let now = time::now_ms();
    rsx! {
        div { class: "schedules",
            h3 { class: "schedules-title", "Schedules" }
            p { class: "adding-note",
                "A schedule queues its prompt as a local issue each time it is due. It is saved paused; enable it to allow the times from then on. Only one request of a schedule is queued or working at a time."
            }
            div { class: "local-issues-list",
                if schedules.is_empty() { p { class: "adding-note", "No schedules yet." } }
                for schedule in schedules {
                    ScheduleRow { key: "{schedule.id}", schedule, now, busy: busy(), mutate }
                }
            }
            div { class: "local-issue-form",
                label { r#for: "schedule-name", "Schedule name" }
                input { id: "schedule-name", class: "adding-input", value: "{name}", disabled: busy(),
                    oninput: move |e| name.set(e.value()) }
                label { r#for: "schedule-prompt", "Prompt" }
                textarea { id: "schedule-prompt", class: "local-issue-description", value: "{prompt}", disabled: busy(),
                    rows: 3, oninput: move |e| prompt.set(e.value()) }
                div { class: "schedule-when",
                    select { aria_label: "Trigger", value: "{trigger}", disabled: busy(),
                        onchange: move |e| trigger.set(e.value()),
                        for word in ["hourly", "daily", "weekdays", "weekly"] {
                            option { value: word, selected: trigger() == word, "{word}" }
                        }
                    }
                    input { class: "adding-input", aria_label: "Time (HH:MM; minutes only when hourly)", placeholder: "09:00",
                        value: "{at}", disabled: busy(), oninput: move |e| at.set(e.value()) }
                    if trigger() == "weekly" {
                        input { class: "adding-input", aria_label: "Days", placeholder: "mon,thu",
                            value: "{days}", disabled: busy(), oninput: move |e| days.set(e.value()) }
                    }
                    input { class: "adding-input", aria_label: "Time zone", placeholder: "this machine's zone",
                        value: "{zone}", disabled: busy(), oninput: move |e| zone.set(e.value()) }
                }
                if let Some(text) = error() { p { class: "adding-error", role: "alert", "{text}" } }
                div { class: "local-issue-actions",
                    Button { kind: "primary",
                        disabled: busy() || name().trim().is_empty() || prompt().trim().is_empty() || dir.trim().is_empty(),
                        onclick: move |_| {
                            match Cadence::compose(&trigger(), &at(), &days()) {
                                Ok(cadence) => mutate.call((Ask::SaveSchedule(NewSchedule {
                                    id: id(),
                                    name: name(),
                                    prompt: prompt(),
                                    dir: dir.clone().into(),
                                    profile: None,
                                    timezone: zone().trim().to_owned(),
                                    cadence,
                                    target: ScheduleTarget::Queue,
                                    grace_minutes: tsumugi_model::DEFAULT_GRACE_MINUTES,
                                }), true)),
                                Err(text) => error.set(Some(text)),
                            }
                        },
                        if busy() { "Saving…" } else { "Save schedule" }
                    }
                }
            }
        }
    }
}

#[component]
fn ScheduleRow(
    schedule: Schedule,
    now: u64,
    busy: bool,
    mutate: EventHandler<(Ask, bool)>,
) -> Element {
    let state = if schedule.enabled {
        "enabled"
    } else {
        "paused"
    };
    let next = schedule
        .next_ms
        .map(|ms| time::until(ms.saturating_sub(now)));
    let last = schedule.last.as_ref().map(|run| {
        format!(
            "{} · {}{}",
            time::ago(now.saturating_sub(run.due_ms)),
            run.outcome.word().replace('_', " "),
            run.note
                .as_ref()
                .map(|n| format!(" · {n}"))
                .unwrap_or_default()
        )
    });
    let (toggle_id, run_id, delete_id) = (
        schedule.id.clone(),
        schedule.id.clone(),
        schedule.id.clone(),
    );
    let enable = !schedule.enabled;
    rsx! {
        div { class: "local-issue schedule",
            div { class: "schedule-head",
                span { class: "local-issue-title", "{schedule.name}" }
                span { class: "local-issue-state {state}", "{state}" }
            }
            div { class: "local-issue-directory",
                "{schedule.cadence.describe()} · {schedule.timezone} · {schedule.dir.display()}"
            }
            if let Some(next) = next { div { class: "schedule-next", "Next {next}" } }
            if let Some(last) = last { div { class: "schedule-last", "Last {last}" } }
            div { class: "local-issue-actions",
                Button { disabled: busy,
                    title: if enable { "Allow the times from now on" } else { "Pause; times that pass are not run" },
                    onclick: move |_| mutate.call((Ask::SetScheduleEnabled { id: toggle_id.clone(), enabled: enable }, false)),
                    if enable { "Enable" } else { "Pause" }
                }
                Button { disabled: busy, onclick: move |_| mutate.call((Ask::RunSchedule { id: run_id.clone() }, false)), "Run now" }
                Button { kind: "danger", disabled: busy,
                    onclick: move |_| mutate.call((Ask::DeleteSchedule { id: delete_id.clone() }, false)), "Delete schedule" }
            }
        }
    }
}
