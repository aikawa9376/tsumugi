//! The Tree section's rows, adding an environment and the sessions waiting
//! on the user. Its Find field belongs to the shared sidebar frame.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::tree;

use super::naming_field::NamingField;
use super::row::{RowContext, RowEvents, TreeRow, WaitingRow};
use crate::bridge::AppCtx;
use crate::components::{Hint, Keys, ListRow};
use crate::icons::{Icon, IconView};
use crate::nav::Item;

/// `waiting_keys` go to the previous and the next session waiting.
#[component]
pub(crate) fn TreePanel(
    context: RowContext,
    on: RowEvents,
    waiting_keys: (Option<String>, Option<String>),
) -> Element {
    let app = use_context::<AppCtx>();
    let (rows, waiting) = {
        let view = app.tree.read();
        (view.rows().to_vec(), view.narrowed_waiting().to_vec())
    };
    // A new entry's name is typed first among the entries of its directory.
    let naming = app.overlays.read().naming.clone().map(|naming| {
        let under = naming.under();
        let depth = app.tree.read().depth(&under) + 1;
        (under, naming, depth)
    });
    let empty = {
        let hosts = app.hosts.read();
        hosts
            .each()
            .all(|(_, host)| tree::listed(host.work.as_ref()).workspaces.is_empty())
    };
    let (waiting_prev, waiting_next) = waiting_keys;
    rsx! {
        div { class: "sidebar-tree-body",
            div { class: "list-rows", style: "flex:1",
                if empty {
                    ListRow { kind: "dim", "No sessions yet" }
                }
                for item in rows.into_iter().filter(|i| !matches!(i, Item::Waiting(_, _))) {
                    TreeRow { key: "{item:?}", item: item.clone(), context, on }
                    if let Some((under, naming, depth)) = naming.clone().filter(|(under, ..)| *under == item) {
                        NamingField { key: "naming-{under:?}", naming, depth }
                    }
                }
                button {
                    r#type: "button",
                    class: "list-row add-environment",
                    title: "Add an environment, on this computer or on a work machine",
                    onclick: move |_| app.dispatch(Msg::AddingOpen),
                    IconView { icon: Icon::Plus, size: 13 }
                    span { class: "list-row-name", "Add environment…" }
                }
            }
            if !waiting.is_empty() {
                div { class: "sidebar-divider" }
                div { class: "sidebar-title",
                    span { "Waiting on you" }
                    Hint {
                        if let Some(k) = waiting_prev { Keys { keys: k } }
                        if let Some(k) = waiting_next { Keys { keys: k } }
                    }
                }
                div { class: "list-rows sidebar-inbox",
                    for (environment, it) in waiting {
                        WaitingRow { key: "w-{environment}-{it.session.id}", environment, waiting: it, on }
                    }
                }
            }
        }
    }
}
