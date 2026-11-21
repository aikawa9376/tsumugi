use super::annotations::{self, Intent, Note};
use super::{Browsers, Page, sync_marks};
use crate::bridge::AppCtx;
use crate::icons::{Icon, IconView};
use crate::prompts::Op;
use base64::Engine;
use dioxus::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tsumugi_core::content::{Area, Tab, WindowId};
use tsumugi_core::msg::Msg;
use tsumugi_model::EnvironmentId;
use tsumugi_protocol::{Answer, Ask};

/// The sessions of `environment` feedback can go to, those of `worktree`
/// first: the page hangs there, so its feedback is most likely for them. The
/// list comes from the environment's host, never the page.
fn sessions(app: AppCtx, environment: &EnvironmentId, worktree: &Path) -> Vec<(String, String)> {
    let hosts = app.hosts.peek();
    let tree = tsumugi_core::tree::listed(hosts.work(environment));
    let mut here = Vec::new();
    let mut elsewhere = Vec::new();
    for w in tree.worktrees() {
        let workspace = tree
            .workspace_of(&w.worktree.path)
            .map(|ws| ws.name.clone())
            .unwrap_or_default();
        for session in &w.sessions {
            let row = (
                session.id.clone(),
                format!("{workspace} / {}", session.name),
            );
            if w.worktree.path == worktree {
                here.push(row);
            } else {
                elsewhere.push(row);
            }
        }
    }
    here.extend(elsewhere);
    here
}

/// What the notes are sent to: the session chosen while it is still listed,
/// else the first session of the page's worktree.
fn destination(choices: &[(String, String)], chosen: &str) -> Option<String> {
    choices
        .iter()
        .find(|(id, _)| id == chosen)
        .or(choices.first())
        .map(|(id, _)| id.clone())
}

/// What a page's feedback is doing: being sent, what went wrong, the session
/// chosen to take it, and the notes it holds.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Sending {
    pub busy: Signal<bool>,
    pub error: Signal<String>,
    pub notes: Signal<Vec<Note>>,
    pub chosen: Signal<String>,
}

/// What goes to the prompt: its text, the images that go with it, and the
/// notes that are off the page once it has gone.
pub(super) struct Delivery {
    pub text: String,
    pub images: Vec<Vec<u8>>,
    pub sent: Vec<Note>,
    /// Run once the prompt has taken it.
    pub after: Option<Box<dyn FnOnce()>>,
}

impl Delivery {
    /// `notes` as the prompt takes them, each element's image beside them.
    pub fn of(notes: Vec<Note>, details: bool) -> Result<Self, &'static str> {
        let text = annotations::instructions(&notes, details)?;
        let images = notes.iter().filter_map(|n| n.image.clone()).collect();
        Ok(Self {
            text,
            images,
            sent: notes,
            after: None,
        })
    }
}

/// Add `delivery` to the prompt of the session chosen, with its images kept
/// by the environment, then take its notes off the page.
pub(super) fn deliver(
    app: AppCtx,
    prompts: crate::prompts::Prompts,
    page: Rc<Page>,
    environment: EnvironmentId,
    worktree: PathBuf,
    mut state: Sending,
    delivery: Delivery,
) {
    if *state.busy.peek() {
        return;
    }
    let choices = sessions(app, &environment, &worktree);
    let Some(selected) = destination(&choices, &state.chosen.peek()) else {
        state
            .error
            .set("Start a session in this environment to add the feedback to its prompt".into());
        return;
    };
    state.busy.set(true);
    state.error.set(String::new());
    spawn(async move {
        let Delivery {
            text,
            images,
            sent,
            after,
        } = delivery;
        let result = async {
            let mut paths = Vec::new();
            for image in &images {
                match app
                    .ask_in(
                        &environment,
                        Ask::KeepImage {
                            mime: "image/png".into(),
                            data: base64::engine::general_purpose::STANDARD.encode(image),
                        },
                    )
                    .await
                {
                    Answer::Kept { path } => paths.push(path),
                    Answer::Failed { text } => return Err(text),
                    _ => {
                        return Err("The environment could not keep the image".into());
                    }
                }
            }
            if app.hosts.peek().session(&environment, &selected).is_none() {
                return Err("The selected session is no longer available".into());
            }
            let tab = Tab::Session(environment.clone(), selected.clone());
            let (tx, rx) = tokio::sync::oneshot::channel();
            prompts
                .send(Op::Context(
                    Some(tab),
                    text.lines().map(String::from).collect(),
                    paths,
                    tx,
                ))
                .map_err(|_| "The prompt is no longer available".to_string())?;
            if !rx.await.unwrap_or(false) {
                return Err("The prompt could not take the feedback; try again".into());
            }
            Ok::<(), String>(())
        }
        .await;
        state.busy.set(false);
        match result {
            Ok(()) => {
                // Keep edits or further selections made during the upload.
                page.notes.borrow_mut().retain(|now| !sent.contains(now));
                state.notes.set(page.notes.borrow().clone());
                sync_marks(&page);
                if let Some(after) = after {
                    after();
                }
                app.dispatch(Msg::ShowSession(environment, selected));
                app.dispatch(Msg::Focus(Area::Prompt));
            }
            Err(message) => state.error.set(message),
        }
    });
}

#[component]
pub(super) fn Feedback(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    state: Sending,
) -> Element {
    let app = use_context::<AppCtx>();
    let prompts = use_context::<crate::shell::Ui>().consoles.prompts;
    let browsers = use_context::<Browsers>();
    let page = use_hook(move || browsers.0.borrow().pages[&window].clone());
    let Sending {
        busy,
        error,
        mut notes,
        mut chosen,
    } = state;
    let mut expanded = use_signal(|| true);
    let mut details = use_signal(|| false);
    let mut copied = use_signal(|| false);
    let mut editing = use_signal(|| None::<u64>);
    let mut edit_text = use_signal(String::new);
    let mut edit_intent = use_signal(Intent::default);
    use_hook({
        let page = page.clone();
        let environment = environment.clone();
        let worktree = worktree.clone();
        move || {
            spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if *notes.peek() == *page.notes.borrow() {
                        continue;
                    }
                    let seen: Vec<u64> = notes.peek().iter().map(|n| n.id).collect();
                    let added: Vec<Note> = page
                        .notes
                        .borrow()
                        .iter()
                        .filter(|n| !seen.contains(&n.id))
                        .cloned()
                        .collect();
                    notes.set(page.notes.borrow().clone());
                    // A grabbed element goes to the prompt as it was picked.
                    let grabbed: Vec<Note> = added.into_iter().filter(|n| n.grab).collect();
                    if let Ok(delivery) = Delivery::of(grabbed, false) {
                        deliver(
                            app,
                            prompts,
                            page.clone(),
                            environment.clone(),
                            worktree.clone(),
                            state,
                            delivery,
                        );
                    }
                }
            })
        }
    });
    let choices = sessions(app, &environment, &worktree);
    let target = destination(&choices, &chosen());
    let can_send =
        target.is_some() && !busy() && annotations::instructions(&notes.read(), details()).is_ok();
    let send = {
        let page = page.clone();
        let environment = environment.clone();
        let worktree = worktree.clone();
        move || {
            let sent = page.notes.borrow().clone();
            if let Ok(delivery) = Delivery::of(sent, *details.peek()) {
                deliver(
                    app,
                    prompts,
                    page.clone(),
                    environment.clone(),
                    worktree.clone(),
                    state,
                    delivery,
                );
            }
        }
    };
    let edit = page.clone();
    let remove = page.clone();
    let clear = page.clone();
    let copy = page.clone();
    let count = notes.read().len();
    let list = notes.read().clone();
    rsx! {
        if count > 0 {
            section { id: "browser-feedback-{window}", class: "browser-tray", aria_label: "Annotations",
                div { class: "tray-head",
                    button { r#type: "button", class: "tray-toggle", aria_expanded: expanded(),
                        title: if expanded() { "Collapse annotations" } else { "Expand annotations" },
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| expanded.set(!expanded()),
                        IconView { icon: if expanded() { Icon::ChevronDown } else { Icon::ChevronRight }, size: 14 }
                        if count == 1 { "1 annotation" } else { "{count} annotations" }
                    }
                    select { aria_label: "Session to send the annotations to", value: target.clone().unwrap_or_default(), disabled: busy(),
                        onchange: move |event| chosen.set(event.value()),
                        if choices.is_empty() { option { value: "", disabled: true, "No session in this environment" } }
                        for (id, label) in choices { option { value: "{id}", "{label}" } }
                    }
                    button { class: "button tray-send", r#type: "button", disabled: !can_send, title: "Add the annotations to the prompt of the session",
                        onclick: move |_| send(),
                        IconView { icon: Icon::Send, size: 12 }
                        if busy() { " Adding…" } else { " Send" }
                    }
                    button { class: "button tray-copy", r#type: "button", title: "Copy the annotations as text",
                        disabled: annotations::instructions(&notes.read(), details()).is_err(),
                        onclick: move |_| {
                            let notes = copy.notes.borrow().clone();
                            if let Ok(text) = annotations::instructions(&notes, *details.peek()) {
                                let _ = crate::clipboard::copy_to_clipboard(&text);
                                copied.set(true);
                                spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                    copied.set(false);
                                });
                            }
                        },
                        IconView { icon: if copied() { Icon::Check } else { Icon::Copy }, size: 12 }
                        if copied() { " Copied" } else { " Copy" }
                    }
                    button { class: "icon-button", r#type: "button", title: "Clear annotations", aria_label: "Clear annotations",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| {
                            clear.notes.borrow_mut().clear();
                            notes.set(Vec::new());
                            sync_marks(&clear);
                        },
                        IconView { icon: Icon::Trash, size: 12 }
                    }
                }
                if !error().is_empty() { div { class: "browser-error", "{error}" } }
                if expanded() {
                    label { class: "tray-details", title: "Send each element's HTML and computed styles as well",
                        input { r#type: "checkbox", checked: details(), onchange: move |e| details.set(e.checked()) }
                        " Include page details"
                    }
                    div { class: "tray-list",
                        for (at, note) in list.into_iter().enumerate() {
                            div { class: "tray-note", key: "{note.id}",
                                span { class: "tray-number", "{at + 1}" }
                                if editing() == Some(note.id) {
                                    div { class: "tray-edit",
                                        textarea {
                                            aria_label: "Annotation comment", rows: "3", autofocus: true,
                                            placeholder: "Describe what the agent should change here…", value: "{edit_text}",
                                            oninput: move |e| edit_text.set(e.value()),
                                            onkeydown: {
                                                let edit = edit.clone(); let id = note.id;
                                                move |event: KeyboardEvent| {
                                                    event.stop_propagation();
                                                    let modifiers = event.modifiers();
                                                    match event.key() {
                                                        Key::Escape => { event.prevent_default(); editing.set(None); }
                                                        Key::Enter if modifiers.ctrl() || modifiers.meta() => {
                                                            event.prevent_default();
                                                            let text = edit_text.peek().trim().to_string();
                                                            if !text.is_empty() {
                                                                if let Some(n) = edit.notes.borrow_mut().iter_mut().find(|n| n.id == id) {
                                                                    n.comment = text;
                                                                    n.intent = *edit_intent.peek();
                                                                }
                                                                notes.set(edit.notes.borrow().clone());
                                                                editing.set(None);
                                                            }
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            },
                                        }
                                        IntentChoice { intent: edit_intent(), onchange: move |i| edit_intent.set(i) }
                                        div { class: "tray-actions",
                                            button { class: "button", r#type: "button", onclick: move |_| editing.set(None), "Cancel" }
                                            button { class: "button primary", r#type: "button", disabled: edit_text.read().trim().is_empty(),
                                                onclick: {
                                                    let edit = edit.clone(); let id = note.id;
                                                    move |_| {
                                                        let text = edit_text.peek().trim().to_string();
                                                        if let Some(n) = edit.notes.borrow_mut().iter_mut().find(|n| n.id == id) {
                                                            n.comment = text;
                                                            n.intent = *edit_intent.peek();
                                                        }
                                                        notes.set(edit.notes.borrow().clone());
                                                        editing.set(None);
                                                    }
                                                },
                                                "Save"
                                            }
                                        }
                                    }
                                } else {
                                    div { class: "tray-body",
                                        div { class: "tray-name", title: "{note.element.selector}", "{note.name()}" }
                                        if note.grab {
                                            div { class: "tray-comment", "Selected element, added as it is" }
                                        } else {
                                            div { class: "tray-comment", "{note.comment}" }
                                        }
                                        div { class: "tray-meta",
                                            if !note.grab { span { "{note.intent.label()}" } }
                                            if note.image.is_some() { span { IconView { icon: Icon::Photo, size: 11 } " 1" } }
                                            if let Some(message) = &note.image_error { span { title: "{message}", "No image" } }
                                        }
                                    }
                                    div { class: "tray-hover",
                                        if !note.grab {
                                            button { class: "icon-button", r#type: "button", title: "Edit annotation {at + 1}", aria_label: "Edit annotation {at + 1}",
                                                onmousedown: move |e| e.prevent_default(),
                                                onclick: { let id = note.id; let text = note.comment.clone(); let intent = note.intent;
                                                    move |_| { edit_text.set(text.clone()); edit_intent.set(intent); editing.set(Some(id)); } },
                                                IconView { icon: Icon::Pencil, size: 12 }
                                            }
                                        }
                                        button { class: "icon-button", r#type: "button", title: "Delete annotation {at + 1}", aria_label: "Delete annotation {at + 1}",
                                            onmousedown: move |e| e.prevent_default(),
                                            onclick: { let page = remove.clone(); let id = note.id; move |_| {
                                                page.notes.borrow_mut().retain(|n| n.id != id);
                                                notes.set(page.notes.borrow().clone());
                                                sync_marks(&page);
                                            } },
                                            IconView { icon: Icon::Trash, size: 12 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The two things a note can ask: a change, or an answer.
#[component]
pub(super) fn IntentChoice(intent: Intent, onchange: EventHandler<Intent>) -> Element {
    rsx! {
        div { class: "intent-choice", role: "group", aria_label: "Annotation intent",
            for (choice, icon) in [(Intent::Change, Icon::Pencil), (Intent::Question, Icon::MessageQuestion)] {
                button {
                    key: "{choice.label()}", r#type: "button",
                    class: if intent == choice { "intent active" } else { "intent" },
                    aria_pressed: intent == choice,
                    onmousedown: move |e| e.prevent_default(),
                    onclick: move |_| onchange.call(choice),
                    IconView { icon, size: 13 }
                    " {choice.label()}"
                }
            }
        }
    }
}
