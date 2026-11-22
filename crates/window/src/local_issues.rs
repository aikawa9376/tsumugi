//! The environment's issue queue and the request to put on it.
use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::settings::{Discretion, Settings, Tier};
use tsumugi_core::tree::Place;
use tsumugi_model::{LocalIssue, LocalIssueState, NewLocalIssue, SessionId};
use tsumugi_protocol::{Answer, Ask};

use crate::bridge::AppCtx;
use crate::components::{Button, Dialog};
use crate::icons::{Icon, IconView};

#[component]
pub(crate) fn StartIssue(environment: tsumugi_model::EnvironmentId, issue: LocalIssue) -> Element {
    let ask = match issue.state {
        LocalIssueState::Open => Ask::StartLocalIssue { id: issue.id },
        LocalIssueState::Failed | LocalIssueState::Cancelled => {
            Ask::RetryLocalIssue { id: issue.id }
        }
        _ => return rsx! {},
    };
    rsx! { IssueAction { environment, ask, title: "Start local issue", "▸" } }
}

#[component]
pub(crate) fn DeleteIssue(environment: tsumugi_model::EnvironmentId, issue: LocalIssue) -> Element {
    if !issue.state.can_delete() {
        return rsx! {};
    }
    rsx! {
        IssueAction { environment, ask: Ask::DeleteLocalIssue { id: issue.id }, title: "Delete local issue",
            IconView { icon: Icon::X, size: 12 }
        }
    }
}

#[component]
fn IssueAction(
    environment: tsumugi_model::EnvironmentId,
    ask: Ask,
    title: String,
    children: Element,
) -> Element {
    let app = use_context::<AppCtx>();
    let mut busy = use_signal(|| false);
    rsx! {
        button {
            r#type: "button", class: "icon-button local-issue-action",
            title: title.clone(), aria_label: title, disabled: busy(),
            onmousedown: move |event| event.prevent_default(),
            onclick: move |event| {
                event.stop_propagation();
                if busy() { return; }
                busy.set(true);
                let answer = app.ask_in(&environment, ask.clone());
                spawn(async move {
                    match answer.await {
                        Answer::Done => {},
                        Answer::Failed { text } | Answer::Unconfirmed { text } => app.dispatch(Msg::Notice(text)),
                        _ => app.dispatch(Msg::Notice("The work host did not confirm the issue change".into())),
                    }
                    busy.set(false);
                });
            }, {children}
        }
    }
}

#[component]
pub(crate) fn LocalIssues() -> Element {
    let app = use_context::<AppCtx>();
    let sheet = app.overlays.read().local_issues.clone();
    rsx! {
        if let Some(sheet) = sheet {
            IssueSheet { key: "{sheet.place.environment}:{sheet.place.worktree.display()}:{sheet.selected:?}:{sheet.leader}", place: sheet.place, selected: sheet.selected, leader: sheet.leader }
        }
    }
}

#[component]
fn IssueSheet(place: Place, selected: Option<String>, leader: bool) -> Element {
    let app = use_context::<AppCtx>();
    let mut id = use_signal(|| SessionId::new().to_string());
    let mut dir = use_signal(|| place.worktree.to_string_lossy().into_owned());
    let mut title = use_signal(String::new);
    let mut body = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let environment = place.environment.clone();
    let issues = app
        .hosts
        .read()
        .work(&environment)
        .map(|work| work.tree.local_issues.clone())
        .unwrap_or_default();
    let schedules = app
        .hosts
        .read()
        .work(&environment)
        .map(|work| work.tree.schedules.clone())
        .unwrap_or_default();
    let name = app.environment_name(&environment);
    let selected_id = selected.clone();
    let mutate = EventHandler::new(move |ask: Ask| {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        let create = matches!(ask, Ask::CreateLocalIssue(_));
        let deleting_selected =
            matches!(&ask, Ask::DeleteLocalIssue { id } if selected_id.as_ref() == Some(id));
        let answer = app.ask_in(&environment, ask);
        spawn(async move {
            match answer.await {
                Answer::Done => {
                    if deleting_selected {
                        app.dispatch(Msg::LocalIssuesClose);
                    }
                    if create {
                        id.set(SessionId::new().to_string());
                        title.set(String::new());
                        body.set(String::new());
                    }
                }
                Answer::Failed { text } | Answer::Unconfirmed { text } => error.set(Some(text)),
                _ => error.set(Some(
                    "The work host did not confirm the issue change".into(),
                )),
            }
            busy.set(false);
        });
    });
    let close = move || app.dispatch(Msg::LocalIssuesClose);
    if leader {
        return rsx! {
            Dialog { kind: "local-issues-dialog", title: "Let the leader handle it · {name}", escapes: true, onclose: move |_| close(),
                LeaderHandOver { environment: place.environment.clone(), issues }
            }
        };
    }
    let field_key = move |event: Event<KeyboardData>| {
        event.stop_propagation();
        if crate::keys::of_field(&event).is_some_and(|key| key.key == "Escape") {
            close();
        }
    };
    rsx! {
        Dialog { kind: "local-issues-dialog", title: "Local Issues · {name}", escapes: true, onclose: move |_| close(),
            div { class: "local-issues-body",
                if selected.is_none() {
                    p { class: "adding-note", "Save issues for later, then press ▸ to start with this environment's default agent. One issue runs at a time in each working directory." }
                }
                div { class: "local-issues-list",
                    if issues.is_empty() { p { class: "adding-note", "No local issues yet." } }
                    for issue in issues {
                        if selected.as_ref().is_none_or(|id| *id == issue.id) {
                            IssueRow { key: "{issue.id}", environment: place.environment.clone(), issue, busy: busy(), mutate }
                        }
                    }
                }
                if selected.is_none() {
                    crate::schedules::Schedules { environment: place.environment.clone(), schedules, dir: dir() }
                }
                if selected.is_none() {
                  div { class: "local-issue-form",
                    label { r#for: "local-issue-dir", "Working directory" }
                    input { id: "local-issue-dir", class: "adding-input", value: "{dir}", disabled: busy(),
                        onkeydown: field_key, oninput: move |e| dir.set(e.value()) }
                    label { r#for: "local-issue-title", "Title" }
                    input { id: "local-issue-title", class: "adding-input", value: "{title}", disabled: busy(),
                        onmounted: move |e| async move { let _ = e.data().set_focus(true).await; },
                        onkeydown: field_key, oninput: move |e| title.set(e.value()) }
                    label { r#for: "local-issue-body", "Description" }
                    textarea { id: "local-issue-body", class: "local-issue-description", value: "{body}", disabled: busy(),
                        rows: 5, onkeydown: field_key, oninput: move |e| body.set(e.value()) }
                    if let Some(text) = error() { p { class: "adding-error", role: "alert", "{text}" } }
                    div { class: "local-issue-actions",
                        Button { onclick: move |_| close(), "Close" }
                        Button { kind: "primary", disabled: busy() || title().trim().is_empty() || dir().trim().is_empty(),
                            onclick: move |_| mutate.call(Ask::CreateLocalIssue(NewLocalIssue {
                                id: id(), dir: dir().into(), title: title(), body: body(),
                            })),
                            if busy() { "Saving…" } else { "Save issue" }
                        }
                    }
                  }
                } else {
                    if let Some(text) = error() { p { class: "adding-error", role: "alert", "{text}" } }
                    Button { onclick: move |_| close(), "Close" }
                }
            }
        }
    }
}

#[component]
fn IssueRow(
    environment: tsumugi_model::EnvironmentId,
    issue: LocalIssue,
    busy: bool,
    mutate: EventHandler<Ask>,
) -> Element {
    let app = use_context::<AppCtx>();
    let state = issue.state.word();
    let cancel_id = issue.id.clone();
    let retry_id = issue.id.clone();
    let start_id = issue.id.clone();
    let close_id = issue.id.clone();
    let reopen_id = issue.id.clone();
    let delete_id = issue.id.clone();
    rsx! {
        div { class: "local-issue",
            details { open: true,
                summary {
                    span { class: "local-issue-title", "{issue.title}" }
                    span { class: "local-issue-state {state}", "{state}" }
                }
                p { class: "local-issue-request", "{issue.body}" }
            }
            div { class: "local-issue-directory",
                "{issue.dir.display()}"
                if issue.schedule.is_some() { " · from a schedule" }
            }
            if let Some(text) = &issue.error { p { class: "adding-error", "{text}" } }
            div { class: "local-issue-actions",
                if let Some(session) = issue.session {
                    Button { onclick: move |_| {
                        app.dispatch(Msg::LocalIssuesClose);
                        app.dispatch(Msg::ShowSession(environment.clone(), session.clone()));
                    }, "Open session" }
                }
                if issue.state == LocalIssueState::Open {
                    Button { title: "Start issue", aria_label: "Start issue", disabled: busy,
                        onclick: move |_| mutate.call(Ask::StartLocalIssue { id: start_id.clone() }), "▸ Start" }
                }
                if matches!(issue.state, LocalIssueState::Open | LocalIssueState::Queued) {
                    Button { disabled: busy, onclick: move |_| mutate.call(Ask::CancelLocalIssue { id: cancel_id.clone() }), "Cancel" }
                }
                if matches!(issue.state, LocalIssueState::Failed | LocalIssueState::Cancelled) {
                    Button { disabled: busy, onclick: move |_| mutate.call(Ask::RetryLocalIssue { id: retry_id.clone() }), "Retry" }
                }
                if issue.state.can_close() {
                    Button { disabled: busy, onclick: move |_| mutate.call(Ask::CloseLocalIssue { id: close_id.clone() }), "Close issue" }
                }
                if issue.state.can_reopen() {
                    Button { disabled: busy, onclick: move |_| mutate.call(Ask::ReopenLocalIssue { id: reopen_id.clone() }), "Reopen" }
                }
                if issue.state.can_delete() {
                    Button { kind: "danger", disabled: busy, onclick: move |_| mutate.call(Ask::DeleteLocalIssue { id: delete_id.clone() }), "Delete issue" }
                }
            }
        }
    }
}

/// What a tier starts, as the confirmation says it.
fn tier_line(tier: Tier, settings: &Settings) -> String {
    match settings.worker_tiers.resolve(tier) {
        Ok(found) => {
            let profile = found
                .profile
                .as_ref()
                .map_or("default profile".to_string(), |p| p.to_string());
            match found.model.as_str() {
                "" => format!("{profile}, its default model"),
                model => format!("{profile}, {model}"),
            }
        }
        Err(_) => "not defined".to_string(),
    }
}

/// The confirmation before issues go to a leader: which issues, how many
/// workers at once, what each tier runs and what the leader may decide.
#[component]
fn LeaderHandOver(environment: tsumugi_model::EnvironmentId, issues: Vec<LocalIssue>) -> Element {
    let app = use_context::<AppCtx>();
    let settings = app.config.read().settings.clone();
    let open: Vec<LocalIssue> = issues
        .into_iter()
        .filter(|issue| issue.state == LocalIssueState::Open)
        .collect();
    let mut chosen = use_signal({
        let ids: Vec<String> = open.iter().map(|issue| issue.id.clone()).collect();
        move || ids
    });
    let mut workers = use_signal(|| settings.leader.max_workers.to_string());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let discretion = &settings.leader.discretion;
    let decides = discretion.listed(Discretion::Decide);
    let asks = discretion.listed(Discretion::Ask);
    let ready = !busy() && !chosen().is_empty() && workers().parse::<u32>().is_ok_and(|n| n > 0);
    let hand_over = move |_| {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        let ask = Ask::StartLeader {
            issues: chosen(),
            max_workers: workers().parse().ok(),
        };
        let answer = app.ask_in(&environment, ask);
        spawn(async move {
            match answer.await {
                Answer::Done => app.dispatch(Msg::LocalIssuesClose),
                Answer::Failed { text } | Answer::Unconfirmed { text } => error.set(Some(text)),
                _ => error.set(Some("The work host did not confirm the leader".into())),
            }
            busy.set(false);
        });
    };
    rsx! {
        div { class: "local-issues-body leader-hand-over",
            p { class: "adding-note", "The leader carries these issues through itself and asks you only for what is outside its discretion." }
            h3 { "Issues" }
            if open.is_empty() { p { class: "adding-note", "No open issues to hand over." } }
            for issue in open {
                label { key: "{issue.id}", class: "leader-issue",
                    input {
                        r#type: "checkbox",
                        checked: chosen().contains(&issue.id),
                        disabled: busy(),
                        onchange: {
                            let id = issue.id.clone();
                            move |event| {
                                let mut now = chosen();
                                now.retain(|held| *held != id);
                                if event.checked() { now.push(id.clone()); }
                                chosen.set(now);
                            }
                        },
                    }
                    " {issue.title} "
                    span { class: "local-issue-directory", "{issue.dir.display()}" }
                }
            }
            h3 { "Workers" }
            label { r#for: "leader-workers", "At most at once " }
            input { id: "leader-workers", class: "adding-input", r#type: "number", min: "1", max: "32",
                value: "{workers}", disabled: busy(), oninput: move |e| workers.set(e.value()) }
            ul { class: "leader-tiers",
                for tier in Tier::ALL {
                    li { key: "{tier.name()}", "{tier.name()}: {tier_line(tier, &settings)}" }
                }
            }
            p { class: "adding-note",
                "Leader: {settings.leader.profile.as_ref().map_or(\"default profile\".to_string(), |p| p.to_string())}"
                if !settings.leader.model.is_empty() { ", {settings.leader.model}" }
            }
            h3 { "It decides" }
            ul { for act in decides { li { key: "{act.label()}", "{act.label()}" } } }
            h3 { "It asks you first" }
            ul { for act in asks { li { key: "{act.label()}", "{act.label()}" } } }
            p { class: "adding-note", "Change these under Settings → Leader." }
            if let Some(text) = error() { p { class: "adding-error", role: "alert", "{text}" } }
            div { class: "local-issue-actions",
                Button { onclick: move |_| app.dispatch(Msg::LocalIssuesClose), "Cancel" }
                Button { kind: "primary", disabled: !ready, onclick: hand_over,
                    if busy() { "Starting…" } else { "Start the leader" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_confirmation_says_what_each_tier_runs_and_which_are_not_defined() {
        let settings = Settings::parse(
            "[worker_tiers.light]\nprofile = 'codex'\nmodel = 'gpt-5'\n[worker_tiers.standard]\nmodel = 'sonnet'\n",
        )
        .unwrap();
        assert_eq!(tier_line(Tier::Light, &settings), "codex, gpt-5");
        assert_eq!(
            tier_line(Tier::Standard, &settings),
            "default profile, sonnet"
        );
        assert_eq!(tier_line(Tier::Heavy, &settings), "not defined");
    }
}
