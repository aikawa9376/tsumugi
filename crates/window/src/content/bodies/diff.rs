//! Repository reviews use the same host reads as the details panel.

use crate::bridge::AppCtx;
use crate::details::changes::{DiffRows, ReviewActions};
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
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    use_effect({
        let id = id.clone();
        move || {
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
        }
    });
    let listing = app
        .hosts
        .read()
        .work(&environment)
        .and_then(|w| w.details.repository(&worktree, &spot))
        .map(|r| r.listing.clone());
    let rev = match &spot {
        Spot::Files(rev) | Spot::Diff(rev, _) => rev.clone(),
        Spot::Log => Rev::WorkingCopy,
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
                if matches!(spot, Spot::Diff(..)) {
                    button { onclick: { let navigate = navigate.clone(); move |_| navigate(Spot::Files(back.clone())) }, "Back to files" }
                }
                ReviewActions { environment, worktree, spot: spot.clone(), tab: false }
            }
            div { id, class: "diff-review-scroll",
                match listing {
                    None => rsx! { p { "Reading…" } },
                    Some(Err(error)) => rsx! { div { class: "git-error", "{error}" } },
                    Some(Ok(Listing::Diff(lines))) => rsx! { DiffRows { lines } },
                    Some(Ok(Listing::Files(files))) if files.is_empty() => rsx! { p { "No changes" } },
                    Some(Ok(Listing::Files(files))) => rsx! {
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
