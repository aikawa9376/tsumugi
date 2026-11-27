//! An issue tried several ways: the attempts to start it as, and those it
//! was started as, each opened, read against its start, compared with
//! another or discarded.
use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_model::{Attempt, EnvironmentId, LocalIssue, TryAs};
use tsumugi_protocol::Ask;

use crate::bridge::AppCtx;
use crate::components::Button;
use crate::icons::{Icon, IconView};

/// An attempt's profile and model as the sheet says them.
fn tried_label(tried: &TryAs) -> String {
    let profile = tried.profile.as_deref().unwrap_or("default profile");
    match &tried.model {
        Some(model) => format!("{profile} · {model}"),
        None => profile.to_owned(),
    }
}

/// The attempts to start `issue` as, two of the first profile to begin
/// with, each with the profile and model chosen.
#[component]
pub(super) fn TryForm(issue: String, busy: bool, mutate: EventHandler<Ask>) -> Element {
    let app = use_context::<AppCtx>();
    let profiles: Vec<(String, String)> = app
        .config
        .read()
        .settings
        .profiles
        .iter()
        .map(|p| {
            let id = p.id.to_string();
            let name = if p.name.is_empty() {
                id.clone()
            } else {
                format!("{} ({id})", p.name)
            };
            (id, name)
        })
        .collect();
    let mut tries = use_signal({
        let first = profiles.first().map(|(id, _)| id.clone());
        move || {
            let tried = TryAs {
                profile: first,
                model: None,
            };
            vec![tried.clone(), tried]
        }
    });
    let count = tries().len();
    rsx! {
        details { class: "local-issue-try-form",
            summary { "Try several ways" }
            p { class: "adding-note", "Each attempt is a session in a worktree of its own, all made from the directory's current commit." }
            for (at, tried) in tries().into_iter().enumerate() {
                div { key: "{at}", class: "local-issue-try",
                    select { aria_label: "Profile of attempt {at + 1}", disabled: busy,
                        onchange: move |e| {
                            let mut now = tries();
                            now[at].profile = Some(e.value());
                            tries.set(now);
                        },
                        for (id, name) in profiles.clone() {
                            option { value: "{id}", selected: tried.profile.as_deref() == Some(id.as_str()), "{name}" }
                        }
                    }
                    input { class: "adding-input", aria_label: "Model of attempt {at + 1}",
                        placeholder: "the profile's own model",
                        value: "{tried.model.clone().unwrap_or_default()}", disabled: busy,
                        onkeydown: move |e| e.stop_propagation(),
                        oninput: move |e| {
                            let mut now = tries();
                            let typed = e.value();
                            now[at].model = Some(typed.trim().to_owned()).filter(|m| !m.is_empty());
                            tries.set(now);
                        }
                    }
                    if count > 1 {
                        button { r#type: "button", class: "icon-button", title: "Remove attempt {at + 1}",
                            aria_label: "Remove attempt {at + 1}", disabled: busy,
                            onclick: move |_| {
                                let mut now = tries();
                                now.remove(at);
                                tries.set(now);
                            },
                            IconView { icon: Icon::X, size: 12 }
                        }
                    }
                }
            }
            div { class: "local-issue-actions",
                Button { disabled: busy, onclick: move |_| {
                    let mut now = tries();
                    now.push(now.last().cloned().unwrap_or_default());
                    tries.set(now);
                }, "Add attempt" }
                Button { kind: "primary", disabled: busy,
                    onclick: move |_| mutate.call(Ask::TryLocalIssue { id: issue.clone(), tries: tries() }),
                    "▸ Start {count} attempts"
                }
            }
        }
    }
}

/// The attempts `issue` was started as, two of which can be chosen to be
/// compared.
#[component]
pub(super) fn Attempts(environment: EnvironmentId, issue: LocalIssue) -> Element {
    let app = use_context::<AppCtx>();
    let mut chosen = use_signal(Vec::<u32>::new);
    let attempts = issue.attempts.clone();
    let pair: Option<(Attempt, Attempt)> = match chosen().as_slice() {
        [first, second] => {
            let find = |n: &u32| attempts.iter().find(|a| a.number == *n).cloned();
            find(first).zip(find(second))
        }
        _ => None,
    };
    let compare_label = pair
        .as_ref()
        .map(|(first, second)| format!("Compare #{} with #{}", first.number, second.number));
    rsx! {
        div { class: "local-issue-attempts",
            for attempt in attempts.clone() {
                AttemptRow { key: "{attempt.number}", environment: environment.clone(), attempt: attempt.clone(),
                    chosen: chosen().contains(&attempt.number),
                    onchoose: move |(number, on): (u32, bool)| {
                        let mut now = chosen();
                        now.retain(|held| *held != number);
                        if on {
                            now.push(number);
                            if now.len() > 2 { now.remove(0); }
                        }
                        chosen.set(now);
                    }
                }
            }
            if let (Some((first, second)), Some(label)) = (pair, compare_label) {
                div { class: "local-issue-actions",
                    span { class: "adding-note", "What either has not committed is left out." }
                    Button { onclick: move |_| {
                        let compared = {
                            let hosts = app.hosts.read();
                            let tree = tsumugi_core::tree::listed(hosts.work(&environment));
                            tsumugi_core::attempts::compare(&environment, tree, &first, &second)
                        };
                        match compared {
                            Ok(open) => {
                                app.dispatch(Msg::LocalIssuesClose);
                                app.dispatch(open);
                            }
                            Err(why) => app.dispatch(Msg::Notice(why)),
                        }
                    }, "{label}" }
                }
            } else if attempts.len() > 1 {
                p { class: "adding-note", "Check two attempts to compare them." }
            }
        }
    }
}

#[component]
fn AttemptRow(
    environment: EnvironmentId,
    attempt: Attempt,
    chosen: bool,
    onchoose: EventHandler<(u32, bool)>,
) -> Element {
    let app = use_context::<AppCtx>();
    let state = attempt.state.word();
    let number = attempt.number;
    let change = tsumugi_core::attempts::change(&environment, &attempt);
    let session = attempt.session.clone();
    let worktree = attempt.worktree.clone();
    let no_worktree = worktree.is_none();
    let (to_show, to_discard) = (environment.clone(), environment.clone());
    rsx! {
        div { class: "local-issue-attempt",
            div { class: "local-issue-attempt-head",
                input { r#type: "checkbox", aria_label: "Compare attempt {number}", checked: chosen,
                    disabled: no_worktree,
                    onchange: move |e| onchoose.call((number, e.checked())) }
                span { class: "local-issue-attempt-name", "#{number} {tried_label(&attempt.tried_as)}" }
                span { class: "local-issue-state {state}", "{state}" }
            }
            if let Some(path) = &worktree { div { class: "local-issue-directory", "{path.display()}" } }
            if let Some(text) = &attempt.error { p { class: "adding-error", "{text}" } }
            div { class: "local-issue-actions",
                if let Some(session) = session {
                    Button { onclick: move |_| {
                        app.dispatch(Msg::LocalIssuesClose);
                        app.dispatch(Msg::ShowSession(to_show.clone(), session.clone()));
                    }, "Open session" }
                }
                if let Some(open) = change {
                    Button { onclick: move |_| {
                        app.dispatch(Msg::LocalIssuesClose);
                        app.dispatch(open.clone());
                    }, "Diff" }
                }
                if let Some(path) = worktree {
                    Button { kind: "danger", onclick: move |_| app.dispatch(Msg::WorktreeDiscard {
                        environment: to_discard.clone(),
                        path: path.clone(),
                    }), "Discard" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attempt_is_called_by_its_profile_and_its_model_when_it_has_one() {
        let tried = |word: &str| TryAs::parse(word).unwrap();
        assert_eq!(tried_label(&tried("codex")), "codex");
        assert_eq!(tried_label(&tried("claude:opus")), "claude · opus");
        assert_eq!(tried_label(&TryAs::default()), "default profile");
    }
}
