//! The sheet keeping a path or an address by hand in a project, or changing
//! the label and note of a resource it keeps.
use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_protocol::{Answer, Ask};

use crate::bridge::AppCtx;
use crate::components::{Button, Dialog};

#[component]
pub(crate) fn ResourceSheet() -> Element {
    let app = use_context::<AppCtx>();
    let sheet = app.overlays.read().resource_sheet.clone();
    rsx! {
        if let Some(sheet) = sheet {
            Sheet {
                key: "{sheet.environment}:{sheet.dir.display()}:{sheet.editing:?}",
                environment: sheet.environment,
                dir: sheet.dir,
                editing: sheet.editing,
            }
        }
    }
}

#[component]
fn Sheet(
    environment: tsumugi_model::EnvironmentId,
    dir: std::path::PathBuf,
    editing: Option<tsumugi_model::Resource>,
) -> Element {
    let app = use_context::<AppCtx>();
    let editing_id = editing.as_ref().map(|r| r.id.clone());
    let adding = editing_id.is_none();
    let mut target = use_signal(String::new);
    let mut label = use_signal({
        let editing = editing.clone();
        move || editing.and_then(|r| r.label).unwrap_or_default()
    });
    let mut note = use_signal({
        let editing = editing.clone();
        move || editing.and_then(|r| r.note).unwrap_or_default()
    });
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let close = move || app.dispatch(Msg::ResourceSheetClose);
    let field_key = move |event: Event<KeyboardData>| {
        event.stop_propagation();
        if crate::keys::of_field(&event).is_some_and(|key| key.key == "Escape") {
            close();
        }
    };
    let title = match &editing {
        Some(resource) => format!("Edit {}", resource.title()),
        None => "Pin a path or an address".to_owned(),
    };
    let ask_dir = dir.clone();
    let save = EventHandler::new({
        let environment = environment.clone();
        move |()| {
            if busy() {
                return;
            }
            let said = |text: String| Some(text).filter(|t| !t.trim().is_empty());
            let ask = match &editing_id {
                Some(id) => Ask::DescribeResource {
                    id: id.clone(),
                    label: said(label()),
                    note: said(note()),
                },
                None => Ask::PinResource {
                    dir: ask_dir.clone(),
                    target: target().trim().to_owned(),
                    label: said(label()),
                },
            };
            busy.set(true);
            error.set(None);
            let answer = app.ask_in(&environment, ask);
            spawn(async move {
                match answer.await {
                    Answer::Done => close(),
                    Answer::Failed { text } | Answer::Unconfirmed { text } => error.set(Some(text)),
                    _ => error.set(Some(
                        "The work host did not confirm the resource change".into(),
                    )),
                }
                busy.set(false);
            });
        }
    });
    let submit_key = move |event: Event<KeyboardData>| {
        if crate::keys::of_field(&event).is_some_and(|key| key.key == "Enter") {
            event.stop_propagation();
            save.call(());
        } else {
            field_key(event);
        }
    };
    rsx! {
        Dialog { kind: "resource-dialog", title: "{title}", escapes: true, onclose: move |_| close(),
            div { class: "local-issue-form",
                if adding {
                    p { class: "adding-note", "A path or an address, kept in the project holding {dir.display()}. A relative path is taken from there." }
                    label { r#for: "resource-target", "Path or address" }
                    input { id: "resource-target", class: "adding-input", value: "{target}", disabled: busy(),
                        onmounted: move |e| async move { let _ = e.data().set_focus(true).await; },
                        onkeydown: submit_key, oninput: move |e| target.set(e.value()) }
                }
                label { r#for: "resource-label", "Label" }
                input { id: "resource-label", class: "adding-input", value: "{label}", disabled: busy(),
                    onmounted: move |e| async move { if !adding { let _ = e.data().set_focus(true).await; } },
                    onkeydown: submit_key, oninput: move |e| label.set(e.value()) }
                if !adding {
                    label { r#for: "resource-note", "Note" }
                    input { id: "resource-note", class: "adding-input", value: "{note}", disabled: busy(),
                        onkeydown: submit_key, oninput: move |e| note.set(e.value()) }
                }
                if let Some(text) = error() { p { class: "adding-error", role: "alert", "{text}" } }
                div { class: "local-issue-actions",
                    Button { onclick: move |_| close(), "Cancel" }
                    Button { kind: "primary", disabled: busy() || (adding && target().trim().is_empty()),
                        onclick: move |_| save.call(()),
                        if busy() { "Saving…" } else if adding { "Pin" } else { "Save" }
                    }
                }
            }
        }
    }
}
