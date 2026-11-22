//! The tab switcher: the content's tabs listed over the window while the
//! modifier keys that opened it are held. It only draws; the view state
//! holds which tab is moved to.

use dioxus::prelude::*;
use tsumugi_core::content::{Tab, TabKind, thing_name};
use tsumugi_core::tree as tree_view;

use super::ui::Ui;

/// The switcher open, or nothing.
#[component]
pub(crate) fn TabSwitcher() -> Element {
    let ui = use_context::<Ui>();
    let Some(open) = ui.app.overlays.read().switcher.clone() else {
        return rsx! {};
    };
    let hosts = ui.app.hosts.read();
    let content = ui.app.content.read();
    let rows: Vec<(bool, String, &'static str)> = open
        .tabs
        .iter()
        .enumerate()
        .filter_map(|(at, key)| {
            let tab = content.windows.tabs().iter().find(|t| t.key() == *key)?;
            let shows: &Tab = &tab.focused().shows;
            let work = hosts.work(shows.environment());
            let name = tree_view::github_title(work, shows)
                .or_else(|| crate::browser::title(shows))
                .unwrap_or_else(|| {
                    thing_name(tree_view::listed(work), tree_view::terminals(work), shows)
                });
            let kind = match shows.kind() {
                TabKind::Session => "Session",
                TabKind::Terminal => "Terminal",
                TabKind::File => "File",
                TabKind::Browser => "Browser",
                TabKind::Other => "",
            };
            Some((at == open.at, name, kind))
        })
        .collect();
    rsx! {
        div { class: "switcher",
            div { class: "switcher-frame", role: "listbox", aria_label: "Switch tabs",
                div { class: "switcher-title", "Switch Tab" }
                for (selected , name , kind) in rows {
                    div {
                        class: if selected { "switcher-row selected" } else { "switcher-row" },
                        role: "option",
                        aria_selected: selected,
                        span { class: "switcher-name", "{name}" }
                        span { class: "switcher-kind", "{kind}" }
                    }
                }
            }
        }
    }
}
