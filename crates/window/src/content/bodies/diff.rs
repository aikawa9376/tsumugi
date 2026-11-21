//! Repository reviews use the same host reads as the details panel.

use crate::bridge::AppCtx;
use crate::details::changes::{DiffRows, ReviewActions};
use crate::review::{ReviewRows, commentable};
use dioxus::prelude::*;
use std::path::PathBuf;
use tsumugi_core::content::{Area, Opening, Tab, WindowId};
use tsumugi_core::msg::Msg;
use tsumugi_core::viewers::ViewerAction;
use tsumugi_model::{
    EnvironmentId,
    repository::{Listing, Rev, Spot},
};

#[component]
pub(crate) fn DiffWindow(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    spot: Spot,
    commands: ReadSignal<Option<(WindowId, crate::viewers::Command)>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let id = format!("diff-{window}");
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    let listing = app
        .hosts
        .read()
        .work(&environment)
        .and_then(|w| w.details.repository(&worktree, &spot))
        .map(|r| r.listing.clone());
    let rev = match &spot {
        Spot::Files(rev) | Spot::Diff(rev, _) | Spot::Lines(rev, _) => rev.clone(),
        Spot::Log | Spot::Branch => Rev::WorkingCopy,
    };
    let navigate = {
        let (environment, worktree) = (environment.clone(), worktree.clone());
        move |spot| {
            app.dispatch(Msg::Show {
                thing: Tab::Diff {
                    environment: environment.clone(),
                    worktree: worktree.clone(),
                    spot: Box::new(spot),
                },
                place: None,
                how: Some(Opening::InWindow(window)),
            })
        }
    };
    let back = rev.clone();
    rsx! {
        div { class: "diff-review",
            onmousedown: move |_| app.dispatch(Msg::Focus(Area::Console)),
            div { class: "diff-review-toolbar",
                if matches!(spot, Spot::Diff(..) | Spot::Lines(..)) {
                    button { onclick: { let navigate = navigate.clone(); move |_| navigate(Spot::Files(back.clone())) }, "Back to files" }
                }
                ReviewActions { environment: environment.clone(), worktree: worktree.clone(), spot: spot.clone(), tab: false }
            }
            div { id, class: "diff-review-scroll",
                match listing {
                    None => rsx! { p { "Reading…" } },
                    Some(Err(error)) => rsx! { Scrolling { window, id: id.clone(), command: ReadSignal::from(command) } div { class: "git-error", "{error}" } },
                    Some(Ok(Listing::Diff(lines))) => match &spot {
                        Spot::Diff(rev, change) | Spot::Lines(rev, change) if commentable(&lines) => rsx! {
                            ReviewRows {
                                window,
                                environment: environment.clone(),
                                worktree: worktree.clone(),
                                path: change.path.clone(),
                                rev: rev.clone(),
                                lines,
                                command: ReadSignal::from(command),
                            }
                        },
                        _ => rsx! {
                            Scrolling { window, id: id.clone(), command: ReadSignal::from(command) }
                            if let Spot::Diff(rev, change) = &spot {
                                button { class: "diff-review-file",
                                    onclick: { let (navigate, rev, change) = (navigate.clone(), rev.clone(), change.clone()); move |_| navigate(Spot::Lines(rev.clone(), change.clone())) },
                                    "Comment on the lines of this change"
                                }
                            }
                            DiffRows { lines }
                        },
                    },
                    Some(Ok(Listing::Files(files))) if files.is_empty() => rsx! { Scrolling { window, id: id.clone(), command: ReadSignal::from(command) } p { "No changes" } },
                    Some(Ok(Listing::Files(files))) => rsx! {
                        Scrolling { window, id: id.clone(), command: ReadSignal::from(command) }
                        for file in files {
                            button { class: "diff-review-file",
                                onclick: { let (navigate, rev, file) = (navigate.clone(), rev.clone(), file.clone()); move |_| navigate(Spot::Diff(rev.clone(), file.clone())) },
                                span { class: "details-file-status", "{file.status}" }
                                "{file.shown()}"
                            }
                        }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
}

/// What the keys scroll in a diff window that has no cursor of its own.
#[component]
fn Scrolling(
    window: WindowId,
    id: String,
    command: ReadSignal<Option<crate::viewers::Command>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    use_effect(move || {
        let Some(c) = command() else { return };
        if last.replace(c.seq) == c.seq {
            return;
        }
        let motion = match c.action {
            ViewerAction::Close => {
                app.dispatch(Msg::Window(tsumugi_core::actions::Action::WindowQuit));
                return;
            }
            ViewerAction::ScrollDown => "e.scrollBy(0,24)",
            ViewerAction::ScrollUp => "e.scrollBy(0,-24)",
            ViewerAction::ScrollHalfDown => "e.scrollBy(0,e.clientHeight/2)",
            ViewerAction::ScrollHalfUp => "e.scrollBy(0,-e.clientHeight/2)",
            ViewerAction::ScrollTop => "e.scrollTo(0,0)",
            ViewerAction::ScrollBottom => "e.scrollTo(0,e.scrollHeight)",
            _ => return,
        };
        document::eval(&format!(
            "{{const e=document.getElementById('{id}');if(e){{{motion}}}}}"
        ));
    });
    rsx! {}
}
