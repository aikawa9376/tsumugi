//! Repository reviews use the same host reads as the details panel.

use crate::bridge::AppCtx;
use crate::details::changes::{DiffRows, ReviewActions};
use crate::review::{ReviewRows, commentable};
use dioxus::prelude::*;
use std::path::PathBuf;
use tsumugi_core::content::{Area, Opening, Tab, WindowId};
use tsumugi_core::diff_view::{Choice, Pictures, Way, file_beside, hunk_from, hunk_starts};
use tsumugi_core::msg::Msg;
use tsumugi_core::viewers::{Diff, FileKind, ViewerAction, is_text};
use tsumugi_model::{
    EnvironmentId,
    repository::{Change, DiffLine, Listing, Rev, Spot},
};

/// The files the diff of `rev` lists, once the work host has read them.
fn files_of(
    app: AppCtx,
    environment: &EnvironmentId,
    worktree: &std::path::Path,
    rev: &Rev,
) -> Vec<Change> {
    app.hosts
        .read()
        .work(environment)
        .and_then(|w| w.details.repository(worktree, &Spot::Files(rev.clone())))
        .and_then(|r| match &r.listing {
            Ok(Listing::Files(files)) => Some(files.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Whether a diff of the file is drawn as two pictures rather than lines.
fn is_picture(change: &Change) -> bool {
    FileKind::of(&change.path) == FileKind::Image && !is_text(&change.path)
}

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
    let view = app.content.read().diff_views.of(&environment, &worktree);
    let files = files_of(app, &environment, &worktree, &rev);
    let on_a_file = matches!(spot, Spot::Diff(..) | Spot::Lines(..));
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
    let choose = {
        let (environment, worktree) = (environment.clone(), worktree.clone());
        move |choice| {
            app.dispatch(Msg::DiffView {
                environment: environment.clone(),
                worktree: worktree.clone(),
                choice,
            })
        }
    };
    let class = if view.wrap {
        "diff-review wrap"
    } else {
        "diff-review"
    };
    let picture = match &spot {
        Spot::Diff(_, change) | Spot::Lines(_, change) => is_picture(change),
        _ => false,
    };
    rsx! {
        div { class,
            onmousedown: move |_| app.dispatch(Msg::Focus(Area::Console)),
            DiffKeys {
                window,
                environment: environment.clone(),
                worktree: worktree.clone(),
                spot: spot.clone(),
                files: files.clone(),
                command: ReadSignal::from(command),
            }
            div { class: "diff-review-toolbar",
                if on_a_file {
                    button { class: if view.files { "on" } else { "" }, title: "Show or hide the files (f)",
                        onclick: { let choose = choose.clone(); move |_| choose(Choice::Files) }, "Files" }
                    button { onclick: { let navigate = navigate.clone(); move |_| navigate(Spot::Files(back.clone())) }, "Back to files" }
                    if picture {
                        button { title: "Compare the pictures another way (m)",
                            onclick: { let choose = choose.clone(); move |_| choose(Choice::Pictures) },
                            match view.pictures {
                                Pictures::SideBySide => "Side by side",
                                Pictures::Swipe => "Swipe",
                                Pictures::Onion => "Onion skin",
                            }
                        }
                    } else {
                        button { class: if view.wrap { "on" } else { "" }, title: "Wrap long lines (w)",
                            onclick: { let choose = choose.clone(); move |_| choose(Choice::Wrap) }, "Wrap" }
                    }
                }
                ReviewActions { environment: environment.clone(), worktree: worktree.clone(), spot: spot.clone(), tab: false }
            }
            div { class: "diff-body",
                if on_a_file && view.files {
                    FileList {
                        environment: environment.clone(),
                        worktree: worktree.clone(),
                        rev: rev.clone(),
                        spot: spot.clone(),
                        files: files.clone(),
                        window,
                    }
                }
                div { id, class: "diff-review-scroll",
                    match listing {
                        None => rsx! { p { "Reading…" } },
                        Some(Err(error)) => rsx! { Scrolling { window, id: id.clone(), command: ReadSignal::from(command) } div { class: "git-error", "{error}" } },
                        Some(Ok(Listing::Diff(lines))) => match &spot {
                            Spot::Diff(rev, change) | Spot::Lines(rev, change) if is_picture(change) => rsx! {
                                Scrolling { window, id: id.clone(), command: ReadSignal::from(command) }
                                PictureDiff {
                                    environment: environment.clone(),
                                    worktree: worktree.clone(),
                                    rev: rev.clone(),
                                    change: change.clone(),
                                    how: view.pictures,
                                }
                            },
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
                                Scrolling { window, id: id.clone(), command: ReadSignal::from(command), lines: lines.clone() }
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
}

/// The files of the change, beside the diff of one of them.
#[component]
fn FileList(
    environment: EnvironmentId,
    worktree: PathBuf,
    rev: Rev,
    spot: Spot,
    files: Vec<Change>,
    window: WindowId,
) -> Element {
    let app = use_context::<AppCtx>();
    let current = match &spot {
        Spot::Diff(_, change) | Spot::Lines(_, change) => Some(change.path.clone()),
        _ => None,
    };
    let lines = matches!(spot, Spot::Lines(..));
    rsx! {
        div { class: "diff-files",
            if files.is_empty() { div { class: "diff-files-empty", "No changes" } }
            for file in files {
                button {
                    key: "{file.path}",
                    class: if current.as_deref() == Some(file.path.as_str()) { "diff-file-row selected" } else { "diff-file-row" },
                    title: "{file.shown()}",
                    onclick: {
                        let (environment, worktree, rev, file) = (environment.clone(), worktree.clone(), rev.clone(), file.clone());
                        move |_| {
                            let spot = if lines { Spot::Lines(rev.clone(), file.clone()) } else { Spot::Diff(rev.clone(), file.clone()) };
                            app.dispatch(Msg::Show {
                                thing: Tab::Diff { environment: environment.clone(), worktree: worktree.clone(), spot: Box::new(spot) },
                                place: None,
                                how: Some(Opening::InWindow(window)),
                            })
                        }
                    },
                    span { class: "details-file-status", "{file.status}" }
                    span { class: "diff-file-name", "{file.shown()}" }
                }
            }
        }
    }
}

/// What the keys that choose how the diff is drawn, or that move between
/// its files, do.
#[component]
fn DiffKeys(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    spot: Spot,
    files: Vec<Change>,
    command: ReadSignal<Option<crate::viewers::Command>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    use_effect(use_reactive((&spot, &files), move |(spot, files)| {
        let Some(c) = command() else { return };
        if last.replace(c.seq) == c.seq {
            return;
        }
        let ViewerAction::Diff(action) = c.action else {
            return;
        };
        match action {
            Diff::Choose(choice) => app.dispatch(Msg::DiffView {
                environment: environment.clone(),
                worktree: worktree.clone(),
                choice,
            }),
            Diff::File(way) => {
                let (rev, to) = match &spot {
                    Spot::Files(rev) => (rev, (way == Way::Next).then(|| files.first()).flatten()),
                    Spot::Diff(rev, change) | Spot::Lines(rev, change) => {
                        (rev, file_beside(&files, &change.path, way))
                    }
                    Spot::Log | Spot::Branch => return,
                };
                let Some(to) = to else { return };
                let spot = match &spot {
                    Spot::Lines(..) => Spot::Lines(rev.clone(), to.clone()),
                    _ => Spot::Diff(rev.clone(), to.clone()),
                };
                app.dispatch(Msg::Show {
                    thing: Tab::Diff {
                        environment: environment.clone(),
                        worktree: worktree.clone(),
                        spot: Box::new(spot),
                    },
                    place: None,
                    how: Some(Opening::InWindow(window)),
                });
            }
            // The lines of the diff take these.
            Diff::Hunk(_) => {}
        }
    }));
    rsx! {}
}

/// What the keys scroll in a diff window that has no cursor of its own, and
/// which of its changes they move over when it shows difftastic's text.
#[component]
fn Scrolling(
    window: WindowId,
    id: String,
    command: ReadSignal<Option<crate::viewers::Command>>,
    #[props(default)] lines: Vec<DiffLine>,
) -> Element {
    let app = use_context::<AppCtx>();
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    let mut at = use_signal(|| None::<usize>);
    use_effect(use_reactive((&lines,), move |(lines,)| {
        let Some(c) = command() else { return };
        if last.replace(c.seq) == c.seq {
            return;
        }
        let motion = match c.action {
            ViewerAction::Close => {
                app.dispatch(Msg::Window(tsumugi_core::actions::Action::WindowQuit));
                return;
            }
            ViewerAction::Diff(Diff::Hunk(way)) => {
                let starts = hunk_starts(&lines);
                let Some(to) = hunk_from(&starts, at.peek().as_ref().copied(), way) else {
                    return;
                };
                at.set(Some(to));
                format!(
                    "(document.querySelectorAll('#{id} .diff > div')[{to}]||e).scrollIntoView({{block:'start'}})"
                )
            }
            ViewerAction::ScrollDown => "e.scrollBy(0,24)".into(),
            ViewerAction::ScrollUp => "e.scrollBy(0,-24)".into(),
            ViewerAction::ScrollHalfDown => "e.scrollBy(0,e.clientHeight/2)".into(),
            ViewerAction::ScrollHalfUp => "e.scrollBy(0,-e.clientHeight/2)".into(),
            ViewerAction::ScrollTop => "e.scrollTo(0,0)".into(),
            ViewerAction::ScrollBottom => "e.scrollTo(0,e.scrollHeight)".into(),
            _ => return,
        };
        document::eval(&format!(
            "{{const e=document.getElementById('{id}');if(e){{{motion}}}}}"
        ));
    }));
    rsx! {}
}

/// The two versions of a picture a change makes, compared.
#[component]
fn PictureDiff(
    environment: EnvironmentId,
    worktree: PathBuf,
    rev: Rev,
    change: Change,
    how: Pictures,
) -> Element {
    let app = use_context::<AppCtx>();
    let (old, new) = tsumugi_core::diff_view::versions(&rev, &change);
    let (had, has) = (old.is_some(), new.is_some());
    // None until the work host has answered; Some(None) when it has none.
    let mut shown = use_signal(|| (None::<Option<String>>, None::<Option<String>>));
    let mut at = use_signal(|| 50.0_f64);
    use_effect(use_reactive(
        (&old, &new, &worktree),
        move |(old, new, worktree)| {
            shown.set((None, None));
            for (side, source) in [(0, old), (1, new)] {
                let environment = environment.clone();
                let worktree = worktree.clone();
                spawn(async move {
                    let url = match source {
                        None => None,
                        Some(source) => match app
                            .ask_in(
                                &environment,
                                tsumugi_protocol::Ask::DiffPicture {
                                    worktree,
                                    rev: source.rev,
                                    path: source.path,
                                },
                            )
                            .await
                        {
                            tsumugi_protocol::Answer::Picture { url } => url,
                            _ => None,
                        },
                    };
                    let mut now = shown.write();
                    if side == 0 {
                        now.0 = Some(url);
                    } else {
                        now.1 = Some(url);
                    }
                });
            }
        },
    ));
    let (before, after) = shown.read().clone();
    let (Some(before), Some(after)) = (before, after) else {
        return rsx! { p { "Reading…" } };
    };
    rsx! {
        div { class: "pic-diff",
            match how {
                Pictures::SideBySide => rsx! {
                    div { class: "pic-pair",
                        figure { class: "pic-side",
                            figcaption { if had { "Before" } else { "Not there before" } }
                            if let Some(url) = &before { img { src: "{url}" } } else if had { div { class: "rv-dim", "It cannot be shown" } }
                        }
                        figure { class: "pic-side",
                            figcaption { if has { "After" } else { "Not there after" } }
                            if let Some(url) = &after { img { src: "{url}" } } else if has { div { class: "rv-dim", "It cannot be shown" } }
                        }
                    }
                },
                Pictures::Swipe => rsx! {
                    div { class: "pic-stack",
                        div { class: "pic-layers",
                            if let Some(url) = &after { img { class: "pic-new", src: "{url}" } }
                            if let Some(url) = &before {
                                img { class: "pic-old", src: "{url}", style: "clip-path: inset(0 {100.0 - at()}% 0 0)" }
                            }
                        }
                        input { r#type: "range", min: "0", max: "100", step: "1", value: "{at()}",
                            oninput: move |e| at.set(e.value().parse().unwrap_or(50.0)) }
                        div { class: "rv-dim", "Before on the left of the divider, after on the right" }
                    }
                },
                Pictures::Onion => rsx! {
                    div { class: "pic-stack",
                        div { class: "pic-layers",
                            if let Some(url) = &after { img { class: "pic-new", src: "{url}" } }
                            if let Some(url) = &before {
                                img { class: "pic-old", src: "{url}", style: "opacity: {at() / 100.0}" }
                            }
                        }
                        input { r#type: "range", min: "0", max: "100", step: "1", value: "{at()}",
                            oninput: move |e| at.set(e.value().parse().unwrap_or(50.0)) }
                        div { class: "rv-dim", "The before fades over the after" }
                    }
                },
            }
        }
    }
}
