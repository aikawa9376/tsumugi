//! The list of the commands a project keeps: searched, run after being
//! shown, written, edited and deleted.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::picker::commands::{
    Commands as List, Do, Editing, Field, Stage, Typed, preview, rows,
};
use tsumugi_core::picker::short;
use tsumugi_protocol::{Answer, Ask};

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;
use crate::components::{Dialog, InlineField, ListRow, PlaceMark};
use crate::icons::Icon;
use crate::shell::Ui;

/// The command's first line, which names it in the list when its label does
/// not say what it runs.
fn first_line(command: &str) -> &str {
    command.lines().next().unwrap_or_default()
}

#[component]
pub(super) fn Commands(list: List) -> Element {
    let app = use_context::<AppCtx>();
    let home = app.picker.read().home.clone();
    let hosts = app.hosts.read();
    let machine = hosts.name(&list.place.environment);
    let place = short(&list.place.worktree, &home);
    let title = format!("Saved commands · {machine}");
    let listed: Vec<_> = rows(&list, &hosts).into_iter().cloned().collect();
    let shown = match &list.stage {
        Stage::Run(command) => Some(preview(&list, command, &hosts)),
        _ => None,
    };
    rsx! {
        Dialog { kind: "picker-dialog commands-dialog", onclose: move |_| app.dispatch(Msg::PickerClose), icon: Icon::Terminal, title,
            match list.stage.clone() {
                Stage::List => rsx! {
                    div { class: "filter-field",
                        QueryField { query: list.list.query.clone(), placeholder: "Run in {place}" }
                    }
                    div { class: "list-rows",
                        if listed.is_empty() {
                            ListRow { kind: "dim",
                                if list.list.query.is_empty() {
                                    "No command is saved for this project. Press Ctrl+a to add one"
                                } else { "Nothing matches" }
                            }
                        }
                        for (i, command) in listed.into_iter().enumerate() {
                            ListRow {
                                key: "{command.id}",
                                selected: i == list.list.cursor,
                                onclick: move |_| app.dispatch(Msg::PickerChoose(i)),
                                PlaceMark { icon: Icon::Terminal }
                                span { class: "list-row-name",
                                    "{command.label}"
                                    span { class: "picker-place", "{first_line(&command.command)}" }
                                }
                            }
                        }
                    }
                },
                Stage::Run(_) => rsx! {
                    if let Some(shown) = shown {
                        div { class: "command-preview",
                            h3 { "{shown.label}" }
                            pre { class: "command-text", "{shown.command}" }
                            dl {
                                dt { "Directory" }
                                dd { "{shown.directory.display()}" }
                                dt { "Runs on" }
                                dd { "{shown.machine}" }
                            }
                        }
                    }
                },
                Stage::Delete(command) => rsx! {
                    div { class: "command-preview",
                        h3 { "Delete “{command.label}”?" }
                        pre { class: "command-text", "{command.command}" }
                    }
                },
                Stage::Edit(editing) => rsx! { Edit { editing } },
            }
            PickerKeys {}
        }
    }
}

#[component]
fn Edit(editing: Editing) -> Element {
    let app = use_context::<AppCtx>();
    let on_command = editing.field == Field::Command;
    let field = editing.field;
    rsx! {
        div { class: "command-form",
            label { r#for: "saved-command-label", "Label" }
            InlineField {
                key: "label-{on_command}",
                class: "adding-input",
                value: editing.label.clone(),
                aria_label: "Label",
                focus: !on_command,
                oninput: move |text| app.dispatch(Msg::CommandsTyped(Typed::Label(text))),
                onenter: move |()| app.dispatch(Msg::PickerKey(tsumugi_model::Key::new("Enter"))),
                onescape: move |()| app.dispatch(Msg::PickerKey(tsumugi_model::Key::new("Escape"))),
                onkey: move |key| app.dispatch(Msg::PickerKey(key)),
            }
            label { r#for: "saved-command-text", "Command" }
            textarea {
                key: "text-{on_command}",
                id: "saved-command-text",
                class: "local-issue-description command-input",
                value: "{editing.command}",
                rows: 6,
                spellcheck: "false",
                "autocorrect": "off",
                "autocapitalize": "off",
                onmounted: move |e| async move {
                    if field == Field::Command {
                        let _ = e.data().set_focus(true).await;
                    }
                },
                oninput: move |e| app.dispatch(Msg::CommandsTyped(Typed::Command(e.value()))),
                onkeydown: move |e: Event<KeyboardData>| {
                    e.stop_propagation();
                    let Some(key) = crate::keys::of_field(&e) else { return };
                    // Enter writes a new line; what leaves the field or keeps
                    // the command is the picker's.
                    let leaves = key.key == "Escape"
                        || (key.ctrl && matches!(key.key.as_str(), "s" | "Enter" | "c"))
                        || (key.shift && key.key == "Tab");
                    if leaves {
                        e.prevent_default();
                        app.dispatch(Msg::PickerKey(key));
                    }
                },
            }
            if let Some(text) = &editing.error {
                p { class: "adding-error", role: "alert", "{text}" }
            }
        }
    }
}

impl Ui {
    /// Run, keep or delete a saved command as the picker chose.
    pub(crate) fn saved_command(&self, done: Do) {
        let app = self.app;
        let (environment, ask) = match done {
            Do::Run {
                place,
                label,
                script,
                behind,
            } => return self.run_saved_command(place, label, script, behind),
            Do::Save {
                environment,
                command,
            } => (environment, Ask::SaveCommand(command)),
            Do::Delete { environment, id } => (environment, Ask::DeleteSavedCommand { id }),
        };
        let answer = app.ask_in(&environment, ask);
        spawn(async move {
            match answer.await {
                Answer::Done => {}
                Answer::Failed { text } | Answer::Unconfirmed { text } => {
                    app.dispatch(Msg::Notice(text));
                }
                _ => app.dispatch(Msg::Notice(
                    "The work host did not confirm the saved command change".into(),
                )),
            }
        });
    }
}
