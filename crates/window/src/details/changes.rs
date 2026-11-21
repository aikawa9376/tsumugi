//! What the changes and the log read of the worktree shown: the rows opened
//! as crumbs, the revision's description, and the files a change touches or
//! one file's diff.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_model::repository::DiffKind;

use super::log::LogRows;
use crate::bridge::AppCtx;
use crate::components::{Breadcrumbs, ListRow};
use crate::details::{Drill, Listing, View};

fn diff_class(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Difft => "diff-structural",
        DiffKind::File => "diff-file",
        DiffKind::Hunk => "diff-hunk",
        DiffKind::Added => "diff-add",
        DiffKind::Removed => "diff-del",
        DiffKind::Context => "diff-ctx",
    }
}

/// A file's status as two spans: the index's letter, which says what is
/// staged, and the worktree's, drawn apart so the two read apart.
#[component]
pub(crate) fn FileStatus(status: String, conflicted: bool) -> Element {
    let mut letters = status.chars();
    let index = letters.next().map(String::from).unwrap_or_default();
    let worktree = letters.next().map(String::from).unwrap_or_default();
    if worktree.is_empty() {
        return rsx! { span { class: "details-file-status", "{index}" } };
    }
    let index_class = if conflicted { "conflicted" } else { "staged" };
    rsx! {
        span { class: "details-file-status",
            span { class: "{index_class}", "{index}" }
            span { class: if conflicted { "conflicted" } else { "" }, "{worktree}" }
        }
    }
}

/// What a worktree is: none shown, outside Git, or a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Worktree {
    None,
    OutsideGit,
    Repository,
}

/// The spot of the changes or the log the panel is at, `view` what was read
/// for it.
#[component]
pub(super) fn Repository(
    drill: Drill,
    view: Option<View>,
    worktree: Worktree,
    back_keys: String,
) -> Element {
    let app = use_context::<AppCtx>();
    let crumbs = drill.crumbs();
    let revision = view.as_ref().and_then(|v| v.revision.clone());
    let listing = view.map(|v| v.listing);
    let selected = listing.as_ref().and_then(|l| drill.selected(l));
    rsx! {
        div { class: "list-rows details-git",
            if !crumbs.is_empty() {
                Breadcrumbs {
                    title: "Back to the list ({back_keys})",
                    crumbs,
                    onback: move |_| app.dispatch(Msg::DetailsBack),
                }
            }
            if let Some(revision) = &revision {
                div { class: "revision-description",
                    div { class: "revision-heading",
                        span { class: "details-label", "Description" }
                        span { class: "commit-hash", title: "{revision.id}", "{revision.short}" }
                    }
                    if revision.description.is_empty() {
                        div { class: "commit-none", "(no description)" }
                    } else {
                        pre { "{revision.description}" }
                    }
                }
            }
            match (worktree, listing) {
                (Worktree::None, _) => rsx! { ListRow { kind: "dim", "Open a worktree to see it here" } },
                (Worktree::OutsideGit, _) => rsx! { ListRow { kind: "dim", "Not a Git repository" } },
                (_, None) => rsx! { ListRow { kind: "dim", "Reading…" } },
                (_, Some(Listing::Error(e))) => rsx! { div { class: "git-error", "{e}" } },
                (_, Some(Listing::Files(files))) if files.is_empty() => rsx! { ListRow { kind: "dim", "No changes" } },
                (_, Some(Listing::Files(files))) => rsx! {
                    for (i , f) in files.iter().enumerate() {
                        ListRow { key: "{f.path}",
                            kind: "mono",
                            selected: selected == Some(i),
                            onclick: move |_| app.dispatch(Msg::DetailsOpenRow(i)),
                            FileStatus { status: f.status.clone(), conflicted: f.is_conflicted() }
                            span { class: "list-row-name", "{f.shown()}" }
                        }
                    }
                },
                (_, Some(Listing::Log(commits))) => rsx! { LogRows { commits, selected, base: None } },
                (_, Some(Listing::Diff(lines))) if lines.is_empty() => rsx! { ListRow { kind: "dim", "No changes" } },
                (_, Some(Listing::Diff(lines))) => rsx! {
                    DiffRows { lines }
                },
                (_, Some(Listing::Entries(_))) => rsx! {},
            }
        }
    }
}

#[component]
pub(crate) fn DiffRows(lines: Vec<tsumugi_model::repository::DiffLine>) -> Element {
    rsx! { div { class: "diff",
        if lines.is_empty() { div { "No changes" } }
        for (i, line) in lines.iter().enumerate() {
            div { key: "{i}", class: "{diff_class(line.kind)}",
                if line.kind == DiffKind::Difft {
                    for (style, text) in super::diff_text::spans(&line.text) { span { style, "{text}" } }
                } else { "{line.text}" }
            }
        }
    } }
}

#[component]
pub(crate) fn ReviewActions(
    environment: tsumugi_model::EnvironmentId,
    worktree: std::path::PathBuf,
    spot: tsumugi_model::repository::Spot,
    #[props(default = true)] tab: bool,
) -> Element {
    let app = use_context::<AppCtx>();
    let open = move |neovim| {
        app.dispatch(Msg::OpenDiff {
            environment: environment.clone(),
            worktree: worktree.clone(),
            spot: spot.clone(),
            neovim,
        })
    };
    let open_tab = open.clone();
    rsx! { div { class: "review-actions",
        button { onclick: { let open = open.clone(); move |_| open(true) }, "Open in Neovim" }
        if tab { button { onclick: move |_| open_tab(false), "Open in Tab" } }
    } }
}
