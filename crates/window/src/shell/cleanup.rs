//! The list of worktrees to clean up: each with what is weighed before it is
//! deleted, and a box to choose it by. It only draws: the view state holds
//! it, and its keys go there.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::overlay::hints;
use tsumugi_core::worktree::cleanup_size;

use crate::bridge::AppCtx;
use crate::components::{Dialog, KeyHint, ListRow, PlaceMark};
use crate::icons::Icon;

#[component]
pub(crate) fn CleanupDialog() -> Element {
    let app = use_context::<AppCtx>();
    let Some(cleanup) = app.overlays.read().cleanup.clone() else {
        return rsx! {};
    };
    let hints = hints::cleanup(&app.chrome.read().keymap);
    let now = tsumugi_model::time::now_ms() / 1000;
    let chosen = cleanup.chosen();
    let reclaimed = cleanup_size(cleanup.reclaimed());
    let rows = cleanup.rows().to_vec();
    rsx! {
        Dialog {
            kind: "picker-dialog cleanup-dialog",
            onclose: move |_| app.dispatch(Msg::CleanupKey(tsumugi_model::Key::new("Escape"))),
            icon: Icon::Trash,
            title: "Clean up the worktrees of {cleanup.project}",
            label: "Clean up worktrees",
            div { class: "list-rows",
                if let Some(why) = cleanup.failed() {
                    ListRow { kind: "dim", "The worktrees could not be listed: {why}" }
                }
                if cleanup.reading() {
                    ListRow { kind: "dim", "Reading the worktrees…" }
                }
                if !cleanup.reading() && cleanup.failed().is_none() && rows.is_empty() {
                    ListRow { kind: "dim", "This repository has no worktree but its main checkout." }
                }
                for (i, row) in rows.into_iter().enumerate() {
                    {
                        let name = row.entry.standing.branch.clone().unwrap_or_else(|| {
                            row.entry.path.file_name().map_or_else(
                                || row.entry.path.display().to_string(),
                                |f| f.to_string_lossy().into_owned(),
                            )
                        });
                        let facts = row.facts(now).join(" · ");
                        let mark = if row.selected { "[x]" } else if row.choosable() { "[ ]" } else { "[-]" };
                        rsx! {
                            ListRow {
                                key: "{row.entry.path.display()}",
                                selected: i == cleanup.cursor,
                                onclick: move |_| app.dispatch(Msg::CleanupToggle(i)),
                                span { class: "cleanup-box", "{mark}" }
                                PlaceMark { icon: Icon::GitBranch }
                                span { class: "list-row-name",
                                    "{name}"
                                    span { class: "picker-place", "{row.entry.path.display()}" }
                                }
                                span { class: "list-row-meta", "{facts}" }
                            }
                        }
                    }
                }
            }
            div { class: "composer-label", "{chosen} chosen, {reclaimed} to be freed" }
            div { class: "picker-keys",
                for hint in hints {
                    KeyHint { key: "{hint.keys}", keys: hint.keys.clone(), does: hint.does }
                }
            }
        }
    }
}
