//! Repository reviews use the same host reads as the details panel.

use crate::bridge::AppCtx;
use crate::components::InlineField;
use crate::details::changes::{DiffRows, ReviewActions};
use crate::review::ReviewRows;
use dioxus::prelude::*;
use std::path::PathBuf;
use tsumugi_core::content::{Area, Opening, Tab, WindowId};
use tsumugi_core::diff_view::{self, Choice, Pictures, hunk_from, hunk_starts};
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

/// `lines`, a stream, with each file of `unfolded` whose whole diff has been
/// read put in place of its hunks.
fn spliced(
    app: AppCtx,
    environment: &EnvironmentId,
    worktree: &std::path::Path,
    rev: &Rev,
    files: &[Change],
    unfolded: &[String],
    lines: Vec<DiffLine>,
) -> Vec<DiffLine> {
    let mut lines = lines;
    for path in unfolded {
        let Some(change) = files.iter().find(|f| &f.path == path) else {
            continue;
        };
        let whole = app
            .hosts
            .read()
            .work(environment)
            .and_then(|w| {
                w.details
                    .repository(worktree, &Spot::Whole(rev.clone(), change.clone()))
            })
            .and_then(|r| match &r.listing {
                Ok(Listing::Diff(whole)) => Some(whole.clone()),
                _ => None,
            });
        let Some(whole) = whole else { continue };
        let heading = |l: &DiffLine| l.kind == tsumugi_model::repository::DiffKind::File;
        let Some(start) = lines.iter().position(|l| heading(l) && &l.text == path) else {
            continue;
        };
        let end = lines[start + 1..]
            .iter()
            .position(heading)
            .map_or(lines.len(), |k| start + 1 + k);
        lines.splice(start..end, whole);
    }
    lines
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
    let rev = match &spot {
        Spot::Files(rev)
        | Spot::Diff(rev, _)
        | Spot::Lines(rev, _)
        | Spot::Stream(rev)
        | Spot::Whole(rev, _) => rev.clone(),
        Spot::Log | Spot::Branch => Rev::WorkingCopy,
    };
    // The file the tab was opened on, which the stream is brought to.
    let opened = match &spot {
        Spot::Diff(_, change) | Spot::Lines(_, change) => Some(change.path.clone()),
        _ => None,
    };
    let stream = app
        .hosts
        .read()
        .work(&environment)
        .and_then(|w| w.details.repository(&worktree, &Spot::Stream(rev.clone())))
        .map(|r| r.listing.clone());
    let view = app.content.read().diff_views.of(&environment, &worktree);
    let unfolded = app
        .content
        .read()
        .diff_views
        .unfolded(&environment, &worktree);
    // Difftastic's view of the file opened, while it is asked for and read.
    let structural = match (&spot, view.structural) {
        (Spot::Diff(rev, change) | Spot::Lines(rev, change), true) => app
            .hosts
            .read()
            .work(&environment)
            .and_then(|w| {
                w.details
                    .repository(&worktree, &Spot::Diff(rev.clone(), change.clone()))
            })
            .and_then(|r| match &r.listing {
                Ok(Listing::Diff(lines)) => Some(lines.clone()),
                _ => None,
            }),
        _ => None,
    };
    let files = files_of(app, &environment, &worktree, &rev);
    // A file shown whole takes the place of its hunks in the stream.
    let stream = stream.map(|read| {
        read.map(|listing| match listing {
            Listing::Diff(lines) => Listing::Diff(spliced(
                app,
                &environment,
                &worktree,
                &rev,
                &files,
                &unfolded,
                lines,
            )),
            other => other,
        })
    });
    // The files marked viewed, as the work host keeps them.
    let mut marks = use_signal(Vec::<tsumugi_model::review::Viewed>::new);
    use_effect(use_reactive(
        (&environment, &worktree),
        move |(environment, worktree)| {
            spawn(async move {
                let ask = tsumugi_protocol::Ask::Viewed { worktree };
                if let tsumugi_protocol::Answer::Viewed { marks: kept } =
                    app.ask_in(&environment, ask).await
                {
                    marks.set(kept);
                }
            });
        },
    ));
    let full = match &stream {
        Some(Ok(Listing::Diff(lines))) => lines.clone(),
        _ => Vec::new(),
    };
    let seen = diff_view::viewed(&full, &rev, &marks.read());
    let stream = stream.map(|read| {
        read.map(|listing| match listing {
            Listing::Diff(lines) => Listing::Diff(diff_view::fold_viewed(&lines, &seen)),
            other => other,
        })
    });
    let mark_viewed = {
        let (environment, worktree, rev, full, seen) = (
            environment.clone(),
            worktree.clone(),
            rev.clone(),
            full.clone(),
            seen.clone(),
        );
        move |path: String| {
            let Some((_, range)) = diff_view::parts(&full)
                .into_iter()
                .find(|(p, _)| *p == path)
            else {
                return;
            };
            let mark = tsumugi_model::review::Viewed {
                rev: rev.clone(),
                path: path.clone(),
                digest: tsumugi_model::review::digest(&full[range]),
            };
            let ask = tsumugi_protocol::Ask::MarkViewed {
                worktree: worktree.clone(),
                mark,
                viewed: !seen.contains(&path),
            };
            let environment = environment.clone();
            spawn(async move {
                if let tsumugi_protocol::Answer::Viewed { marks: kept } =
                    app.ask_in(&environment, ask).await
                {
                    marks.set(kept);
                }
            });
        }
    };
    let merge_base = app
        .hosts
        .read()
        .work(&environment)
        .and_then(|w| w.details.repository(&worktree, &Spot::Branch))
        .and_then(|r| match &r.listing {
            Ok(Listing::Branch(b)) => b.comparison.merge_base.clone(),
            _ => None,
        });
    let mut choices = diff_view::changes(merge_base.as_deref());
    if !choices.contains(&rev) {
        choices.push(rev.clone());
    }
    let mut comparing = use_signal(|| None::<String>);
    let mut width = use_signal(|| 0.0_f64);
    let mut notice = use_signal(|| None::<String>);
    // The file the cursor is in, which the list marks.
    let current = use_signal(|| opened.clone());
    // The file last opened from the list, and how many times one was, so
    // opening the same file again brings the stream back to it.
    let mut jump = use_signal(|| (None::<String>, 0_u64));
    let next = {
        let at = choices.iter().position(|c| *c == rev).unwrap_or(0);
        let to = choices[(at + 1) % choices.len()].clone();
        let here = current.peek().clone();
        let file = files.iter().find(|f| Some(&f.path) == here.as_ref());
        (to != rev).then(|| match file {
            Some(file) => Spot::Lines(to, file.clone()),
            None => Spot::Files(to),
        })
    };
    // Switching to another change keeps the file in sight where the other
    // change has it too.
    let switch = {
        let (environment, worktree, files) = (environment.clone(), worktree.clone(), files.clone());
        move |to: Rev| {
            let here = current.peek().clone();
            let file = files.iter().find(|f| Some(&f.path) == here.as_ref());
            let spot = match file {
                Some(file) => Spot::Lines(to, file.clone()),
                None => Spot::Files(to),
            };
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
    let compare = {
        let (environment, worktree, switch) =
            (environment.clone(), worktree.clone(), switch.clone());
        move |text: String| {
            let Some((from, to)) = diff_view::compared(&text) else {
                notice.set(Some("Write two revisions, as main..HEAD".into()));
                return;
            };
            let (environment, worktree, switch) =
                (environment.clone(), worktree.clone(), switch.clone());
            spawn(async move {
                let ask = tsumugi_protocol::Ask::Compare { worktree, from, to };
                match app.ask_in(&environment, ask).await {
                    tsumugi_protocol::Answer::Compared { from, to } => {
                        comparing.set(None);
                        notice.set(None);
                        switch(Rev::Range(from, to));
                    }
                    tsumugi_protocol::Answer::Failed { text } => notice.set(Some(text)),
                    _ => notice.set(Some("The work host did not answer".into())),
                }
            });
        }
    };
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
    let picture = files
        .iter()
        .any(|f| current.read().as_deref() == Some(f.path.as_str()) && is_picture(f));
    let pictures = files
        .iter()
        .filter(|f| is_picture(f))
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        div { class,
            onmousedown: move |_| app.dispatch(Msg::Focus(Area::Console)),
            DiffKeys {
                window,
                environment: environment.clone(),
                worktree: worktree.clone(),
                command: ReadSignal::from(command),
                next,
                comparing,
            }
            div { class: "diff-review-toolbar",
                div { class: "diff-changes",
                    for choice in choices {
                        button { key: "{diff_view::called(&choice)}",
                            class: if choice == rev { "on" } else { "" },
                            title: match &choice {
                                Rev::Base(_) => "The worktree against the branch's base, committed or not",
                                Rev::WorkingCopy => "What is not committed",
                                Rev::Branch(_) => "What the branch committed",
                                _ => "",
                            },
                            onclick: { let (switch, choice) = (switch.clone(), choice.clone()); move |_| switch(choice.clone()) },
                            "{diff_view::called(&choice)}"
                        }
                    }
                    if let Some(text) = comparing() {
                        InlineField {
                            class: "diff-compare",
                            value: text,
                            placeholder: "main..HEAD — Enter compares, Escape gives up".to_string(),
                            focus: true,
                            contained: true,
                            oninput: move |text: String| comparing.set(Some(text)),
                            onenter: { let mut compare = compare.clone(); move |()| compare(comparing.peek().clone().unwrap_or_default()) },
                            onescape: move |()| { comparing.set(None); notice.set(None); },
                        }
                    } else {
                        button { title: "Compare two revisions",
                            onclick: move |_| comparing.set(Some(String::new())), "Compare…" }
                    }
                    if let Some(text) = notice() { span { class: "git-error", "{text}" } }
                }
                button { class: if view.files { "on" } else { "" }, title: "Show or hide the files (f)",
                    onclick: { let choose = choose.clone(); move |_| choose(Choice::Files) }, "Files" }
                button { title: "Draw the lines in two columns where there is room, in one, or in two (|)",
                    onclick: { let choose = choose.clone(); move |_| choose(Choice::Columns) },
                    match view.columns {
                        diff_view::Columns::Auto => "Columns: auto",
                        diff_view::Columns::One => "One column",
                        diff_view::Columns::Two => "Two columns",
                    }
                }
                if opened.is_some() {
                    button { class: if view.structural { "on" } else { "" }, title: "Show this file as difftastic reads it (D)",
                        onclick: { let choose = choose.clone(); move |_| choose(Choice::Structural) }, "Difftastic" }
                }
                button { class: if view.wrap { "on" } else { "" }, title: "Wrap long lines (w)",
                    onclick: { let choose = choose.clone(); move |_| choose(Choice::Wrap) }, "Wrap" }
                if picture {
                    button { title: "Compare the pictures another way (m)",
                        onclick: { let choose = choose.clone(); move |_| choose(Choice::Pictures) },
                        match view.pictures {
                            Pictures::SideBySide => "Side by side",
                            Pictures::Swipe => "Swipe",
                            Pictures::Onion => "Onion skin",
                        }
                    }
                }
                ReviewActions { environment: environment.clone(), worktree: worktree.clone(), spot: spot.clone(), tab: false }
            }
            div { class: "diff-body",
                if view.files {
                    FileList {
                        current: current.read().clone(),
                        files: files.clone(),
                        viewed: seen.iter().cloned().collect::<Vec<_>>(),
                        onopen: move |path: String| {
                            let times = jump.peek().1 + 1;
                            jump.set((Some(path), times));
                        },
                    }
                }
                div { id, class: "diff-review-scroll",
                    onresize: move |e: Event<ResizeData>| {
                        e.stop_propagation();
                        if let Ok(size) = e.data().get_content_box_size() {
                            width.set(size.width);
                        }
                    },
                    if let Some(lines) = structural {
                        Scrolling { window, id: id.clone(), command: ReadSignal::from(command), lines: lines.clone() }
                        DiffRows { lines }
                    } else {
                    match stream {
                        None => rsx! { p { "Reading…" } },
                        Some(Err(error)) => rsx! { Scrolling { window, id: id.clone(), command: ReadSignal::from(command) } div { class: "git-error", "{error}" } },
                        Some(Ok(Listing::Diff(lines))) if lines.is_empty() => rsx! { Scrolling { window, id: id.clone(), command: ReadSignal::from(command) } p { "No changes" } },
                        Some(Ok(Listing::Diff(lines))) => rsx! {
                            ReviewRows {
                                window,
                                environment: environment.clone(),
                                worktree: worktree.clone(),
                                path: opened.clone().unwrap_or_default(),
                                rev: rev.clone(),
                                lines,
                                command: ReadSignal::from(command),
                                start: jump().0.or(opened.clone()),
                                jumps: jump().1,
                                current,
                                pictures,
                                how: view.pictures,
                                two: view.columns.two_at(width()),
                                viewed: seen.iter().cloned().collect::<Vec<_>>(),
                                onviewed: mark_viewed.clone(),
                            }
                        },
                        _ => rsx! {},
                    }
                    }
                }
            }
        }
    }
}

/// The files of the change, beside the stream of them; opening one brings
/// the stream to it in the same tab rather than opening another.
#[component]
fn FileList(
    current: Option<String>,
    files: Vec<Change>,
    viewed: Vec<String>,
    onopen: EventHandler<String>,
) -> Element {
    let counted = files.iter().filter(|f| viewed.contains(&f.path)).count();
    rsx! {
        div { class: "diff-files",
            if files.is_empty() { div { class: "diff-files-empty", "No changes" } }
            else { div { class: "diff-files-count", "{counted} of {files.len()} viewed" } }
            for file in files {
                button {
                    key: "{file.path}",
                    class: if current.as_deref() == Some(file.path.as_str()) { "diff-file-row selected" } else { "diff-file-row" },
                    title: "{file.shown()}",
                    onclick: { let path = file.path.clone(); move |_| onopen.call(path.clone()) },
                    span { class: "details-file-status", "{file.status}" }
                    span { class: "diff-file-name", "{file.shown()}" }
                    if viewed.contains(&file.path) { span { class: "diff-file-viewed", title: "Viewed", "✓" } }
                }
            }
        }
    }
}

/// What the keys that choose how the diff is drawn do; the stream's own
/// keys move over it.
#[component]
fn DiffKeys(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    command: ReadSignal<Option<crate::viewers::Command>>,
    /// Where the key that shows the next change goes.
    next: Option<Spot>,
    comparing: Signal<Option<String>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    use_effect(use_reactive((&next,), move |(next,)| {
        let Some(c) = command() else { return };
        if last.replace(c.seq) == c.seq {
            return;
        }
        match c.action {
            ViewerAction::Diff(Diff::Choose(choice)) => app.dispatch(Msg::DiffView {
                environment: environment.clone(),
                worktree: worktree.clone(),
                choice,
            }),
            ViewerAction::Diff(Diff::Change) => {
                if let Some(spot) = next {
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
            }
            ViewerAction::Diff(Diff::Compare) => {
                let mut comparing = comparing;
                comparing.set(Some(String::new()));
            }
            _ => {}
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
pub(crate) fn PictureDiff(
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
