//! The sheet that keeps a path or an address in the project's resources, or
//! changes the label and note of one kept.
use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::tree::Place;
use tsumugi_protocol::{Answer, Ask};

use crate::bridge::AppCtx;
use crate::components::{Button, Dialog};

#[component]
pub(crate) fn ResourceSheet() -> Element {
    let app = use_context::<AppCtx>();
    let sheet = app.overlays.read().resources.clone();
    let editing_key = sheet
        .as_ref()
        .and_then(|s| s.editing.as_ref())
        .map(|r| r.id.clone())
        .unwrap_or_default();
    rsx! {
        if let Some(sheet) = sheet {
            Form {
                key: "{sheet.place.environment}:{sheet.place.worktree.display()}:{editing_key}",
                place: sheet.place,
                editing: sheet.editing,
            }
        }
    }
}

#[component]
fn Form(place: Place, editing: Option<tsumugi_model::Resource>) -> Element {
    let app = use_context::<AppCtx>();
    let mut target = use_signal(|| {
        editing
            .as_ref()
            .map(|r| r.target.clone())
            .unwrap_or_default()
    });
    let mut label = use_signal(|| {
        editing
            .as_ref()
            .and_then(|r| r.label.clone())
            .unwrap_or_default()
    });
    let mut note = use_signal(|| {
        editing
            .as_ref()
            .and_then(|r| r.note.clone())
            .unwrap_or_default()
    });
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let name = app.environment_name(&place.environment);
    let close = move || app.dispatch(Msg::ResourcesClose);
    let editing_id = editing.as_ref().map(|r| r.id.clone());
    let save = use_callback({
        let environment = place.environment.clone();
        let dir = place.worktree.clone();
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
                None => Ask::KeepResource {
                    dir: dir.clone(),
                    target: target(),
                    label: said(label()),
                    note: said(note()),
                },
            };
            busy.set(true);
            error.set(None);
            let answer = app.ask_in(&environment, ask);
            spawn(async move {
                match answer.await {
                    Answer::Done => app.dispatch(Msg::ResourcesClose),
                    Answer::Failed { text } | Answer::Unconfirmed { text } => error.set(Some(text)),
                    _ => error.set(Some("The work host did not confirm the resource".into())),
                }
                busy.set(false);
            });
        }
    });
    let is_editing = editing.is_some();
    let (title, action) = if is_editing {
        ("Edit resource", "Save")
    } else {
        ("Add resource", "Keep")
    };
    let hint = if place.worktree.as_os_str().is_empty() {
        String::new()
    } else {
        format!(
            "A relative path is taken from {}.",
            place.worktree.display()
        )
    };
    // Enter in a one-line field keeps the resource; Escape puts the sheet away.
    let field_key = move |event: Event<KeyboardData>| {
        event.stop_propagation();
        match crate::keys::of_field(&event).map(|key| key.key) {
            Some(key) if key == "Escape" => close(),
            Some(key) if key == "Enter" => save(()),
            _ => {}
        }
    };
    let note_key = move |event: Event<KeyboardData>| {
        event.stop_propagation();
        if crate::keys::of_field(&event).is_some_and(|key| key.key == "Escape") {
            close();
        }
    };
    rsx! {
        Dialog { kind: "resource-dialog", title: "{title} · {name}", escapes: true, onclose: move |_| close(),
            div { class: "resource-form",
                p { class: "adding-note",
                    "Keep a path or an address of this project, such as a design document, a configuration file or a site. Only the reference is kept; open it from the panel's Resources. {hint}"
                }
                label { r#for: "resource-target", "Path or address" }
                if is_editing {
                    input { id: "resource-target", class: "adding-input", value: "{target}", disabled: true }
                } else {
                    input { id: "resource-target", class: "adding-input", value: "{target}", disabled: busy(),
                        placeholder: "https://… or /path/to/file",
                        onmounted: move |e| async move { let _ = e.data().set_focus(true).await; },
                        onkeydown: field_key,
                        oninput: move |e| target.set(e.value()) }
                }
                label { r#for: "resource-label", "Label (optional)" }
                input { id: "resource-label", class: "adding-input", value: "{label}", disabled: busy(),
                    onkeydown: field_key, oninput: move |e| label.set(e.value()) }
                label { r#for: "resource-note", "Note (optional)" }
                textarea { id: "resource-note", class: "resource-note", value: "{note}", disabled: busy(),
                    rows: 4, onkeydown: note_key, oninput: move |e| note.set(e.value()) }
                if let Some(text) = error() { p { class: "adding-error", role: "alert", "{text}" } }
                div { class: "resource-actions",
                    Button { onclick: move |_| close(), "Cancel" }
                    Button { kind: "primary", disabled: busy() || target().trim().is_empty(),
                        onclick: move |_| save(()),
                        if busy() { "Saving…" } else { "{action}" }
                    }
                }
            }
        }
    }
}
