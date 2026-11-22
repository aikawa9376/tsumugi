//! The environment's issue queue and the request to put on it.
use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
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
            IssueSheet { key: "{sheet.place.environment}:{sheet.place.worktree.display()}:{sheet.selected:?}", place: sheet.place, selected: sheet.selected }
        }
    }
}

#[component]
fn IssueSheet(place: Place, selected: Option<String>) -> Element {
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
