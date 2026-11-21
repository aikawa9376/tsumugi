//! The log: the history of the worktree shown, drawn with its graph.

use dioxus::prelude::*;
use tsumugi_core::graph;
use tsumugi_model::repository::Commit;

use crate::bridge::AppCtx;
use tsumugi_core::msg::Msg;
use tsumugi_core::overlay::context_menu::{Reach, Target};

/// `commits` with their graph; a click opens one, or, with `base`, the row at
/// its place from `base` of the panel's list.
#[component]
pub(super) fn LogRows(
    commits: Vec<Commit>,
    selected: Option<usize>,
    base: Option<usize>,
) -> Element {
    let app = use_context::<AppCtx>();
    let rows = graph::layout(
        commits
            .iter()
            .map(|c| (c.node.as_str(), c.parents.as_slice())),
    );
    let lanes = rows.iter().map(graph::Row::width).max().unwrap_or(1);
    rsx! {
        for (i , (c , row)) in commits.iter().zip(&rows).enumerate() {
            div { key: "{c.id}",
                class: if selected == Some(i) { "commit selected" } else { "commit" },
                onclick: move |_| app.dispatch(match base {
                    Some(base) => Msg::DetailsClick(base + i),
                    None => Msg::DetailsOpenRow(i),
                }),
                oncontextmenu: move |e: Event<MouseData>| {
                    e.prevent_default();
                    e.stop_propagation();
                    let p = e.client_coordinates();
                    app.dispatch(Msg::MenuOpen {
                        x: p.x,
                        y: p.y,
                        target: Target::Commit { at: base.unwrap_or(0) + i },
                        reach: Reach::Here,
                    });
                },
                div {
                    class: "commit-graph",
                    dangerous_inner_html: graph::svg(row, lanes, graph::Marks { current: c.current, empty: c.empty, conflict: c.conflict }),
                }
                div { class: "commit-text",
                span { class: "commit-hash", "{c.short}" }
                span { class: "commit-subject",
                    if c.current {
                        span { class: "commit-mark current", "@" }
                    }
                    if c.conflict {
                        span { class: "commit-mark conflict", "conflict" }
                    }
                    if c.empty {
                        span { class: "commit-mark", "empty" }
                    }
                    for b in c.bookmarks.iter() {
                        span { class: "commit-mark bookmark", "{b}" }
                    }
                    if c.subject.is_empty() {
                        span { class: "commit-none", "(no description)" }
                    } else {
                        "{c.subject}"
                    }
                }
                span { class: "commit-when", "{c.when}" }
                }
            }
        }
    }
}
