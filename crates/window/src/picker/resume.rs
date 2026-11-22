//! The list of past conversations.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::picker::resume::{Resume as List, rows};
use tsumugi_core::picker::short;
use tsumugi_model::time::{ago, now_ms};
use tsumugi_model::{Key, Preview, Speaker};
use tsumugi_protocol::{Answer, Ask};

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;
use crate::components::{Button, Dialog, ListRow, PlaceMark};
use crate::icons::Icon;

#[component]
pub(super) fn Resume(list: List) -> Element {
    let app = use_context::<AppCtx>();
    let home = app.picker.read().home.clone();
    let settings = app.config.read().settings.clone();
    let hosts = app.hosts.read();
    let rows = rows(&list, &home, hosts.work(&list.environment));
    let now = now_ms();
    let group = list.group;
    let selected = rows
        .as_ref()
        .and_then(|rows| rows.get(list.list.cursor))
        .map(|row| row.past.clone());
    let environment = list.environment.clone();
    // The conversation under the cursor is read once the cursor rests on it,
    // so moving through the rows reads none of those passed.
    let preview = use_resource(use_reactive!(|(selected, environment)| async move {
        let past = selected?;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        match app
            .ask_in(
                &environment,
                Ask::PastPreview {
                    past: Box::new(past),
                },
            )
            .await
        {
            Answer::PastPreview { preview } => preview,
            _ => None,
        }
    }));
    let scope = match (&list.dir, list.every) {
        (Some(dir), false) => short(dir, &home),
        _ => "every directory".to_string(),
    };
    rsx! {
        Dialog { kind: "picker-dialog", onclose: move |_| app.dispatch(Msg::PickerClose), icon: Icon::TransferIn, title: "Resume a conversation",
            div { class: "filter-field",
                QueryField { query: list.list.query.clone(), placeholder: "Conversations of {scope}" }
                Button {
                    onclick: move |_| app.dispatch(Msg::PickerKey(Key { ctrl: true, ..Key::new("g") })),
                    "{group.label()}"
                }
                if list.dir.is_some() {
                    Button {
                        onclick: move |_| app.dispatch(Msg::PickerEvery),
                        if list.every { "Every directory" } else { "{scope}" }
                    }
                }
            }
            div { class: "list-rows",
                match rows {
                    None => rsx! { ListRow { kind: "dim", "Reading the conversations…" } },
                    Some(rows) if rows.is_empty() => rsx! {
                        ListRow { kind: "dim",
                            if list.list.query.is_empty() { "No conversation was kept here" } else { "Nothing matches" }
                        }
                    },
                    Some(rows) => rsx! {
                        for (i, row) in rows.into_iter().enumerate() {
                            Fragment { key: "{row.past.agent:?}:{row.past.id}",
                            if let Some(heading) = row.heading.clone() {
                                div { class: "picker-heading", "{heading}" }
                            }
                            ListRow {
                                selected: i == list.list.cursor,
                                onclick: move |_| app.dispatch(Msg::PickerChoose(i)),
                                PlaceMark { icon: if row.session.is_some() { Icon::Message } else { Icon::TransferIn } }
                                span { class: "list-row-name",
                                    "{row.past.title}"
                                    span { class: "picker-place", "{short(&row.past.cwd, &home)}" }
                                }
                                span { class: "list-row-meta",
                                    if row.session.is_some() { "open · " }
                                    "{tsumugi_core::profiles::resumed_as(&settings, &row.past)} · {ago(now.saturating_sub(row.past.at_ms))}"
                                }
                            }
                            }
                        }
                    },
                }
            }
            div { class: "resume-preview",
                match &*preview.read() {
                    Some(Some(preview)) => rsx! { Said { preview: preview.clone() } },
                    Some(None) => rsx! { div { class: "picker-place", "Nothing to show of this conversation" } },
                    None => rsx! {},
                }
            }
            PickerKeys {}
        }
    }
}

/// What a conversation says: its first prompt whole, and its latest turns.
#[component]
fn Said(preview: Preview) -> Element {
    rsx! {
        if !preview.first.is_empty() {
            div { class: "picker-heading", "First prompt" }
            pre { class: "command-text", "{preview.first}" }
        }
        if !preview.recent.is_empty() {
            div { class: "picker-heading", "Latest" }
            for (i, turn) in preview.recent.iter().enumerate() {
                div { key: "{i}", class: "resume-turn",
                    span { class: "picker-place",
                        match turn.speaker { Speaker::User => "You", Speaker::Agent => "Agent" }
                    }
                    "{turn.text}"
                }
            }
        }
    }
}
