//! A diff window's review of one file's line diff: the lines numbered, a
//! cursor on them, the comments under the lines they are on, the one being
//! written, and the prompt that carries them to an agent.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use dioxus::prelude::*;
use tsumugi_core::content::{Area, WindowId};
use tsumugi_core::diff_view::{Row, folds, pieces, rows, word_marks};
use tsumugi_core::msg::Msg;
use tsumugi_core::review::{Go, Mode, Pane, Reading, Target};
use tsumugi_core::viewers::{Diff, Review, ViewerAction};
use tsumugi_model::github::LineThread;
use tsumugi_model::repository::{DiffKind, DiffLine, Rev};
use tsumugi_model::review::{self as notes, Change, Judged};
use tsumugi_model::{Agent, EnvironmentId};
use tsumugi_protocol::{Answer, Ask};

use crate::bridge::AppCtx;
use crate::components::InlineField;
use crate::viewers::Command;

/// A session a review can be sent to.
#[derive(Debug, Clone, PartialEq)]
struct Destination {
    id: String,
    name: String,
    agent: Agent,
    /// It waits on the user, whose keys a prompt would answer.
    waiting: bool,
}

/// The sessions of `worktree`, which the review is most likely for.
fn destinations(
    app: AppCtx,
    environment: &EnvironmentId,
    worktree: &std::path::Path,
) -> Vec<Destination> {
    let hosts = app.hosts.peek();
    let tree = tsumugi_core::tree::listed(hosts.work(environment));
    tree.worktrees()
        .filter(|w| w.worktree.path == worktree)
        .flat_map(|w| w.sessions.iter())
        .map(|s| Destination {
            id: s.id.clone(),
            name: s.name.clone(),
            agent: s.agent,
            waiting: s.state.takes_every_key(),
        })
        .collect()
}

/// What a diff window needs to review one file.
#[derive(Clone)]
struct Review_ {
    app: AppCtx,
    environment: EnvironmentId,
    worktree: PathBuf,
    path: String,
    rev: Rev,
    lines: Vec<DiffLine>,
    pane: Signal<Pane>,
    comments: Signal<Vec<Judged>>,
    /// The review threads of the branch's pull request on GitHub.
    threads: Signal<Vec<LineThread>>,
    /// The discard key was pressed once, and waits to be pressed again.
    armed: Signal<bool>,
    /// What is being written to search for, while it is.
    searching: Signal<Option<String>>,
    /// The lines of the worktree's files its sessions' agents wrote.
    written: Signal<Vec<notes::Written>>,
    /// What was last searched for.
    sought: Signal<String>,
    onviewed: Option<EventHandler<String>>,
    onneovim: Option<EventHandler<(Option<String>, Option<u32>)>>,
}

/// What is done with the hunk or the lines chosen.
#[derive(Clone, Copy, PartialEq)]
enum Staging {
    Stage,
    Unstage,
    Discard,
}

impl Review_ {
    /// Bring the cursor to the line `way` of it holding what was sought.
    fn find(&self, way: tsumugi_core::diff_view::Way) {
        let mut pane = self.pane;
        let comments = self.all();
        let reading = self.reading(&comments);
        let here = pane
            .peek()
            .cursor(&reading)
            .min(self.lines.len().saturating_sub(1));
        let sought = self.sought.peek().clone();
        match tsumugi_core::diff_view::find(&self.lines, here, &sought, way) {
            Some(at) => pane.write().place(at),
            None if sought.is_empty() => pane.write().notice = Some("Search with /".into()),
            None => pane.write().notice = Some(format!("Not found: {sought}")),
        }
    }

    /// Mark the file the cursor is in viewed, or not.
    fn viewed(&self) {
        let comments = self.all();
        let reading = self.reading(&comments);
        let at = self.pane.peek().cursor(&reading);
        if at < self.lines.len()
            && let Some(onviewed) = self.onviewed
        {
            onviewed.call(reading.path_at(at).to_string());
        }
    }

    /// Open the file the cursor is in in the user's Neovim, on the line the
    /// cursor is on.
    fn neovim(&self) {
        let Some(onneovim) = self.onneovim else {
            return;
        };
        let comments = self.all();
        let reading = self.reading(&comments);
        let at = self.pane.peek().cursor(&reading);
        if at < self.lines.len() {
            let line = tsumugi_core::diff_view::new_line_at(&self.lines, at);
            onneovim.call((Some(reading.path_at(at).to_string()), line));
        } else {
            onneovim.call((Some(self.path.clone()).filter(|p| !p.is_empty()), None));
        }
    }

    /// Show the file the cursor is in whole, or as its hunks again.
    fn unfold(&self) {
        let comments = self.all();
        let reading = self.reading(&comments);
        let at = self.pane.peek().cursor(&reading);
        if at >= self.lines.len() {
            return;
        }
        self.app.dispatch(Msg::DiffUnfold {
            environment: self.environment.clone(),
            worktree: self.worktree.clone(),
            path: reading.path_at(at).to_string(),
        });
    }

    /// Stage, unstage or discard the hunk or the lines chosen, discarding
    /// only once its key is pressed twice.
    fn stage(&self, how: Staging) {
        let mut pane = self.pane;
        let mut armed = self.armed;
        if self.rev != Rev::WorkingCopy {
            pane.write().notice = Some("Stage from what is not committed: t shows it".into());
            return;
        }
        let comments = self.all();
        let reading = self.reading(&comments);
        let (first, last) = pane.peek().span(&reading);
        if pane.peek().cursor(&reading) >= self.lines.len() {
            pane.write().notice = Some("Choose lines of the diff".into());
            return;
        }
        if how == Staging::Discard && !*armed.peek() {
            armed.set(true);
            pane.write().notice = Some(
                "Press X again to discard these lines from the worktree; it cannot be undone"
                    .into(),
            );
            return;
        }
        armed.set(false);
        let reverse = how != Staging::Stage;
        let patch = match tsumugi_core::diff_view::patch(&self.lines, first, last, reverse) {
            Ok(patch) => patch,
            Err(why) => {
                pane.write().notice = Some(why.words().into());
                return;
            }
        };
        let this = self.clone();
        spawn(async move {
            let ask = Ask::Git {
                worktree: this.worktree.clone(),
                op: tsumugi_protocol::GitOp::Apply {
                    patch,
                    index: how != Staging::Discard,
                    reverse,
                },
            };
            let said = match this.app.ask_in(&this.environment, ask).await {
                Answer::Done => match how {
                    Staging::Stage => "Staged".to_string(),
                    Staging::Unstage => "Unstaged".to_string(),
                    Staging::Discard => "Discarded".to_string(),
                },
                Answer::Failed { text } => text,
                _ => "The work host did not answer".to_string(),
            };
            pane.write().notice = Some(said);
        });
    }

    /// The comments kept and the threads of GitHub's, as the diff draws them.
    fn all(&self) -> Vec<Judged> {
        let mut all = self.comments.peek().clone();
        all.extend(tsumugi_core::review::github_threads(
            &self.lines,
            &self.path,
            &self.rev,
            &self.threads.peek(),
        ));
        all
    }

    /// Read the threads of the branch's pull request from GitHub, in a diff
    /// against the base alone; what cannot be read draws none.
    fn read_threads(&self) {
        let (this, mut threads) = (self.clone(), self.threads);
        if !matches!(self.rev, Rev::Branch(_) | Rev::Base(_)) {
            if !threads.peek().is_empty() {
                threads.set(Vec::new());
            }
            return;
        }
        spawn(async move {
            let ask = Ask::LineThreads {
                worktree: this.worktree.clone(),
            };
            let now = match this.app.ask_in(&this.environment, ask).await {
                Answer::LineThreads { threads } => threads,
                _ => Vec::new(),
            };
            if *threads.peek() != now {
                threads.set(now);
            }
        });
    }

    fn reading<'a>(&'a self, comments: &'a [Judged]) -> Reading<'a> {
        Reading::new(&self.lines, &self.path, &self.rev, comments)
    }

    /// Read what the work host keeps again, and say why when it cannot.
    fn reload(&self) {
        let (this, mut pane, mut comments) = (self.clone(), self.pane, self.comments);
        let mut written = self.written;
        spawn(async move {
            // Only the worktree's files as they read now carry the marks.
            let now = if matches!(this.rev, Rev::WorkingCopy | Rev::Base(_)) {
                let ask = Ask::Written {
                    worktree: this.worktree.clone(),
                };
                match this.app.ask_in(&this.environment, ask).await {
                    Answer::Written { files } => files,
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            if *written.peek() != now {
                written.set(now);
            }
            let ask = Ask::Review {
                worktree: this.worktree.clone(),
            };
            match this.app.ask_in(&this.environment, ask).await {
                Answer::Review { comments: now } => comments.set(now),
                Answer::Failed { text } => pane.write().notice = Some(text),
                _ => {}
            }
        });
    }

    /// Have the work host make `change`, and show what it keeps after.
    fn apply(&self, change: Change) {
        let (this, mut pane, mut comments) = (self.clone(), self.pane, self.comments);
        spawn(async move {
            let ask = Ask::ChangeReview {
                worktree: this.worktree.clone(),
                change: Box::new(change),
            };
            match this.app.ask_in(&this.environment, ask).await {
                Answer::Review { comments: now } => comments.set(now),
                Answer::Failed { text } => pane.write().notice = Some(text),
                _ => pane.write().notice = Some("The work host did not answer".into()),
            }
        });
    }

    /// Send the prompt previewed to the destination chosen, keeping every
    /// comment whatever comes of it.
    fn send(&self) {
        let Mode::Previewing(preview) = self.pane.peek().mode.clone() else {
            return;
        };
        if preview.busy {
            return;
        }
        let (this, mut pane) = (self.clone(), self.pane);
        let choices = destinations(this.app, &this.environment, &this.worktree);
        let Some(to) = choices
            .get(preview.choice.min(choices.len().saturating_sub(1)))
            .cloned()
        else {
            pane.write().sending(
                false,
                Some("Start a session in this worktree, or press S to prepare a new one".into()),
            );
            return;
        };
        if to.waiting {
            pane.write().sending(
                false,
                Some("Answer the session first; the review is kept".into()),
            );
            return;
        }
        pane.write().sending(true, None);
        spawn(async move {
            let ask = Ask::SubmitPrompt {
                session: to.id.clone(),
                agent: to.agent,
                text: preview.text.clone(),
                images: Vec::new(),
                wait: None,
            };
            match this.app.ask_in(&this.environment, ask).await {
                Answer::Done => {
                    // Only what was sent, as it was, is marked.
                    let ids = preview.sent_ids(&this.comments.peek());
                    this.apply(Change::Sent { ids });
                    let mut pane = pane.write();
                    pane.cancel();
                    pane.notice = Some(format!("Sent to {}", to.name));
                }
                Answer::Unconfirmed { text } => pane.write().sending(
                    false,
                    Some(format!(
                        "Could not confirm it went; the review is kept: {text}"
                    )),
                ),
                Answer::Failed { text } => pane.write().sending(false, Some(text)),
                _ => pane
                    .write()
                    .sending(false, Some("The work host did not answer".into())),
            }
        });
    }

    /// Prepare a new session's draft with the prompt written in it.
    fn draft(&self) {
        let text = match &self.pane.peek().mode {
            Mode::Previewing(preview) => preview.text.clone(),
            _ => notes::prompt(&self.comments.peek()),
        };
        let mut pane = self.pane;
        if text.is_empty() {
            pane.write().notice = Some("No comments to send".into());
            return;
        }
        pane.write().cancel();
        self.app.dispatch(Msg::DraftWritten {
            environment: self.environment.clone(),
            worktree: self.worktree.clone(),
            prompt: text,
        });
    }

    /// Do what the key `action` asks.
    fn run(&self, action: ViewerAction, window: WindowId) {
        if action != ViewerAction::Diff(Diff::Discard) {
            let mut armed = self.armed;
            armed.set(false);
        }
        let comments = self.all();
        let reading = self.reading(&comments);
        let mut pane = self.pane;
        let mode = pane.peek().mode.clone();
        let choices = if matches!(mode, Mode::Previewing(_)) {
            destinations(self.app, &self.environment, &self.worktree).len()
        } else {
            0
        };
        let mut go = |to| pane.write().go(&reading, to);
        match (&mode, action) {
            (Mode::Writing { .. }, _) => {}
            (Mode::Previewing(_), ViewerAction::Close) => pane.write().cancel(),
            (Mode::Previewing(_), ViewerAction::ScrollDown | ViewerAction::ScrollHalfDown) => {
                pane.write().choose(1, choices)
            }
            (Mode::Previewing(_), ViewerAction::ScrollUp | ViewerAction::ScrollHalfUp) => {
                pane.write().choose(-1, choices)
            }
            (Mode::Previewing(_), ViewerAction::Review(Review::Send)) => self.send(),
            (Mode::Previewing(_), ViewerAction::Review(Review::Draft)) => self.draft(),
            (Mode::Previewing(_), _) => {}
            (_, ViewerAction::Close) => self
                .app
                .dispatch(Msg::Window(tsumugi_core::actions::Action::WindowQuit)),
            (_, ViewerAction::ScrollDown) => go(Go::By(1)),
            (_, ViewerAction::ScrollUp) => go(Go::By(-1)),
            (_, ViewerAction::ScrollHalfDown) => go(Go::By(12)),
            (_, ViewerAction::ScrollHalfUp) => go(Go::By(-12)),
            (_, ViewerAction::Diff(Diff::Hunk(way))) => go(Go::Hunk(way)),
            (_, ViewerAction::Diff(Diff::File(way))) => go(Go::File(way)),
            (_, ViewerAction::Diff(Diff::Stage)) => self.stage(Staging::Stage),
            (_, ViewerAction::Diff(Diff::Search)) => {
                let mut searching = self.searching;
                searching.set(Some(String::new()));
            }
            (_, ViewerAction::Diff(Diff::Found(way))) => self.find(way),
            (_, ViewerAction::Diff(Diff::Unfold)) => self.unfold(),
            (_, ViewerAction::Diff(Diff::Viewed)) => self.viewed(),
            (_, ViewerAction::Diff(Diff::Neovim)) => self.neovim(),
            (_, ViewerAction::Diff(Diff::Note(way))) => go(Go::Note(way)),
            (_, ViewerAction::Diff(Diff::Unstage)) => self.stage(Staging::Unstage),
            (_, ViewerAction::Diff(Diff::Discard)) => self.stage(Staging::Discard),
            (_, ViewerAction::ScrollTop) => go(Go::Top),
            (_, ViewerAction::ScrollBottom) => go(Go::Bottom),
            (_, ViewerAction::Review(Review::Select)) => pane.write().select(&reading),
            (_, ViewerAction::Review(Review::Comment)) => pane.write().comment(&reading),
            (_, ViewerAction::Review(Review::Edit)) => pane.write().edit(&reading),
            (_, ViewerAction::Review(Review::Delete)) => {
                if let Some(change) = pane.write().delete(&reading) {
                    self.apply(change);
                }
            }
            (_, ViewerAction::Review(Review::Resolve)) => {
                if let Some(change) = pane.write().resolve(&reading) {
                    self.apply(change);
                }
            }
            (_, ViewerAction::Review(Review::Send)) => pane.write().preview(&comments),
            (_, ViewerAction::Review(Review::Draft)) => self.draft(),
            (_, ViewerAction::Review(Review::Take)) => {
                let taken = pane.write().take(&reading);
                if let Some(change) = taken {
                    self.apply(change);
                    pane.write().notice = Some("Taken into your review".into());
                }
            }
            _ => {}
        }
        let _ = window;
    }
}

fn row_class(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Difft => "diff-structural",
        DiffKind::File => "diff-file",
        DiffKind::Hunk => "diff-hunk",
        DiffKind::Added => "diff-add",
        DiffKind::Removed => "diff-del",
        DiffKind::Context => "diff-ctx",
    }
}

fn line_class(kind: DiffKind, cursor: bool, marked: bool, commented: bool) -> String {
    let mut class = format!("rv-line {}", row_class(kind));
    for (on, name) in [
        (cursor, " rv-cursor"),
        (marked, " rv-marked"),
        (commented, " rv-commented"),
    ] {
        if on {
            class.push_str(name);
        }
    }
    class
}

/// How the lines of `anchor` are said.
fn said(anchor: &notes::Anchor) -> String {
    let side = match anchor.side {
        notes::Side::Old => "before",
        notes::Side::New => "after",
    };
    if anchor.start == anchor.end {
        format!("{side}, line {}", anchor.start)
    } else {
        format!("{side}, lines {}-{}", anchor.start, anchor.end)
    }
}

/// The lines a row draws.
fn row_lines(row: Row) -> Vec<usize> {
    match row {
        Row::Whole(i) => vec![i],
        Row::Pair(old, new) => old.into_iter().chain(new).collect(),
    }
}

/// A line's text with the words it changed marked.
#[component]
fn Words(text: String, marks: Vec<(usize, usize)>) -> Element {
    rsx! {
        span { class: "rv-text-line",
            for (k , (piece , marked)) in pieces(&text, &marks).into_iter().enumerate() {
                if marked { span { key: "{k}", class: "rv-word", "{piece}" } } else { "{piece}" }
            }
        }
    }
}

/// One side of a row of two columns: a line with its number on that side.
#[component]
fn LineCell(
    i: usize,
    line: DiffLine,
    number: Option<u32>,
    /// The session that wrote the line, on the new side.
    #[props(default)]
    by: Option<String>,
    marks: Vec<(usize, usize)>,
    class: String,
    pane: Signal<Pane>,
) -> Element {
    let app = use_context::<AppCtx>();
    rsx! {
        div { class: "{class} rv-cell",
            onclick: move |_| {
                app.dispatch(Msg::Focus(Area::Console));
                let mut pane = pane;
                pane.write().place(i);
            },
            // A right click puts the cursor on the line, whose menu then opens.
            oncontextmenu: move |_| {
                let mut pane = pane;
                pane.write().place(i);
            },
            NewNumber { number, by: by.clone() }
            Words { text: line.text.clone(), marks }
            if class.contains("rv-cursor") && let Some(by) = by { span { class: "rv-by", "written by {by}" } }
        }
    }
}

/// A line's number on the new side, marked in the gutter when a session's
/// agent wrote the line.
#[component]
fn NewNumber(number: Option<u32>, by: Option<String>) -> Element {
    let shown = number.map(|n| n.to_string()).unwrap_or_default();
    match by {
        Some(by) => rsx! { span { class: "rv-no rv-ai", title: "Written by {by}", "{shown}" } },
        None => rsx! { span { class: "rv-no", "{shown}" } },
    }
}

#[component]
fn Thread(judged: Judged, at: bool, outdated: bool) -> Element {
    let comment = judged.comment;
    let class = format!(
        "rv-thread{}{}{}{}",
        if comment.is_note() {
            " rv-note"
        } else if comment.is_github() {
            " rv-github"
        } else {
            ""
        },
        if comment.resolved { " resolved" } else { "" },
        if outdated { " outdated" } else { "" },
        if at { " rv-cursor" } else { "" },
    );
    rsx! { div { class,
        div { class: "rv-head",
            if let Some(url) = &comment.github { span { class: "rv-badge rv-author", title: "A review thread of the pull request on GitHub, {url}: T takes it into your review", "GitHub" } }
            else if let Some(author) = &comment.author { span { class: "rv-badge rv-author", title: "A note the session's agent left", "note · {author}" } }
            span { class: "rv-range", "{said(&comment.anchor)}" }
            if outdated { span { class: "rv-badge", title: "The lines changed since it was written; it is not moved to others", "outdated" } }
            if comment.resolved { span { class: "rv-badge", "resolved" } }
            else if comment.sent { span { class: "rv-badge", "sent" } }
        }
        if outdated {
            for quoted in comment.quoted.iter() { div { class: "rv-quoted", "{quoted}" } }
        }
        if !comment.resolved { pre { class: "rv-text", "{comment.text}" } }
        else { div { class: "rv-text rv-dim", "{comment.text}" } }
    } }
}

/// A change's line diff, every file under its heading, reviewed: `command`
/// is what the keys ask of it. The cursor is brought to the file `start`
/// whenever it changes, and `current` is told the file the cursor is in.
/// The files of `pictures` are drawn as pictures under their headings.
#[component]
pub(crate) fn ReviewRows(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    path: String,
    rev: Rev,
    lines: Vec<DiffLine>,
    command: ReadSignal<Option<Command>>,
    #[props(default)] start: Option<String>,
    /// How many times a file was opened from the list, so opening `start`
    /// again brings the cursor back to it.
    #[props(default)]
    jumps: u64,
    current: Option<Signal<Option<String>>>,
    #[props(default)] pictures: Vec<tsumugi_model::repository::Change>,
    #[props(default)] how: tsumugi_core::diff_view::Pictures,
    /// The old is drawn beside the new.
    #[props(default)]
    two: bool,
    /// The files marked viewed, folded to their headings.
    #[props(default)]
    viewed: Vec<String>,
    /// Marks the file at the path given viewed, or not.
    onviewed: Option<EventHandler<String>>,
    /// Opens the file at the path given in the user's Neovim, on the line
    /// given of its new side.
    onneovim: Option<EventHandler<(Option<String>, Option<u32>)>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let pane = use_signal(Pane::default);
    let comments = use_signal(Vec::<Judged>::new);
    let threads = use_signal(Vec::<LineThread>::new);
    let armed = use_signal(|| false);
    let written = use_signal(Vec::<notes::Written>::new);
    let mut searching = use_signal(|| None::<String>);
    let mut hide_notes = use_signal(|| false);
    let mut sought = use_signal(String::new);
    let this = Review_ {
        app,
        environment: environment.clone(),
        worktree: worktree.clone(),
        path: path.clone(),
        rev: rev.clone(),
        lines: lines.clone(),
        pane,
        comments,
        threads,
        armed,
        searching,
        written,
        sought,
        onviewed,
        onneovim,
    };
    // What the work host keeps is read when the diff is, so a change to the
    // code shows at once which comments no longer stand.
    use_effect(use_reactive((&lines, &worktree, &rev), {
        let this = this.clone();
        move |_| this.reload()
    }));
    // GitHub is asked only as the diff is opened, never as its code changes.
    use_effect(use_reactive((&worktree, &rev), {
        let this = this.clone();
        move |(worktree, rev)| {
            Review_ {
                worktree,
                rev,
                ..this.clone()
            }
            .read_threads()
        }
    }));
    let last = use_hook(|| Rc::new(Cell::new(command.peek().map_or(0, |c| c.seq))));
    use_effect(use_reactive((&lines, &path, &rev, &worktree), {
        let this = this.clone();
        move |(lines, path, rev, worktree)| {
            let Some(c) = command() else { return };
            if last.replace(c.seq) == c.seq {
                return;
            }
            let this = Review_ {
                lines,
                path,
                rev,
                worktree,
                ..this.clone()
            };
            this.run(c.action, window);
        }
    }));
    // Opening a file of the list, or the tab on a file, brings the cursor
    // to its heading.
    let id = format!("review-{window}");
    use_effect(use_reactive((&start, &jumps, &lines), {
        let id = id.clone();
        move |(start, jumps, lines)| {
            let Some(start) = start else { return };
            if let Some(at) = lines
                .iter()
                .position(|l| l.kind == DiffKind::File && l.text == start)
            {
                let mut pane = pane;
                pane.write().place(at);
                // Opened from the list, the file's heading goes to the top,
                // as a move to the nearest edge would leave it at the bottom.
                if jumps > 0 {
                    document::eval(&format!(
                        "requestAnimationFrame(()=>requestAnimationFrame(()=>document.querySelector('#{id} .rv-cursor')?.scrollIntoView({{block:'start'}})))"
                    ));
                }
            }
        }
    }));
    use_effect(use_reactive((&lines, &path), move |(lines, path)| {
        let Some(mut current) = current else { return };
        let comments = comments.read();
        let reading = Reading::new(&lines, &path, &Rev::WorkingCopy, &comments);
        let at = pane.read().cursor(&reading);
        let here = (at < lines.len()).then(|| reading.path_at(at).to_string());
        if here.is_some() && *current.peek() != here {
            current.set(here);
        }
    }));
    use_effect({
        let id = id.clone();
        move || {
            let _ = pane.read().clone();
            document::eval(&format!(
                "requestAnimationFrame(()=>document.querySelector('#{id} .rv-cursor')?.scrollIntoView({{block:'nearest'}}))"
            ));
        }
    });
    let mut all = comments.read().clone();
    all.extend(tsumugi_core::review::github_threads(
        &lines,
        &path,
        &rev,
        &threads.read(),
    ));
    let reading = this.reading(&all);
    let state = pane.read().clone();
    let cursor = state.cursor(&reading);
    let (first, last_marked) = state.span(&reading);
    let marking = state.marking();
    let numbers = notes::numbered(&lines);
    let outdated = reading.outdated();
    let kept_count = all.iter().filter(|j| !j.comment.is_github()).count();
    let open = all
        .iter()
        .filter(|j| !j.comment.resolved && !j.comment.is_github())
        .count();
    let from_github = all.len() - kept_count;
    let choices = destinations(app, &environment, &worktree);
    // In one column every line is a row; a removed and an added line are
    // still compared, to mark the words each does not share.
    let drawn = if two {
        rows(&lines)
    } else {
        (0..lines.len()).map(Row::Whole).collect()
    };
    let marks = word_marks(&lines);
    let folded = folds(&lines);
    let written_now = written.read().clone();
    let by: Vec<Option<String>> = reading
        .written_by(&written_now)
        .into_iter()
        .map(|by| by.map(str::to_string))
        .collect();
    let kept = reading.written(&written_now);
    rsx! {
        div { id: "{id}", class: "review",
            div { class: if two { "rv-lines two" } else { "rv-lines" },
                for (k , row) in drawn.iter().enumerate() {
                    if let Row::Whole(i) = *row
                        && let Some(&(_, hidden)) = folded.iter().find(|(h, _)| *h == i)
                    {
                        div { key: "fold-{i}", class: "rv-fold",
                            title: "Show the whole file (z a)",
                            onclick: { let this = this.clone(); move |_| {
                                this.app.dispatch(Msg::DiffUnfold {
                                    environment: this.environment.clone(),
                                    worktree: this.worktree.clone(),
                                    path: Reading::new(&this.lines, &this.path, &this.rev, &[]).path_at(i).to_string(),
                                });
                            } },
                            "⋯ {hidden} unchanged lines"
                        }
                    }
                    match *row {
                        Row::Whole(i) if two && lines[i].kind == DiffKind::Context && numbers[i].old.is_some() => rsx! {
                            div { key: "{k}", class: "rv-row2",
                                LineCell { i, line: lines[i].clone(), number: numbers[i].old, marks: Vec::new(), class: line_class(lines[i].kind, i == cursor, marking && (first..=last_marked).contains(&i), !reading.standing_on(i).is_empty()), pane }
                                LineCell { i, line: lines[i].clone(), number: numbers[i].new, by: by[i].clone(), marks: Vec::new(), class: line_class(lines[i].kind, i == cursor, marking && (first..=last_marked).contains(&i), !reading.standing_on(i).is_empty()), pane }
                            }
                        },
                        Row::Whole(i) => rsx! {
                            div { key: "{k}",
                                class: line_class(lines[i].kind, i == cursor, marking && (first..=last_marked).contains(&i), !reading.standing_on(i).is_empty()),
                                onclick: move |_| {
                                    app.dispatch(Msg::Focus(Area::Console));
                                    let mut pane = pane;
                                    pane.write().place(i);
                                },
                                // A right click puts the cursor on the line, whose menu then opens.
                                oncontextmenu: move |_| {
                                    let mut pane = pane;
                                    pane.write().place(i);
                                },
                                span { class: "rv-no", "{numbers[i].old.map(|n| n.to_string()).unwrap_or_default()}" }
                                NewNumber { number: numbers[i].new, by: by[i].clone() }
                                if lines[i].kind == DiffKind::Difft { "{lines[i].text}" } else { Words { text: lines[i].text.clone(), marks: marks[i].clone() } }
                                if i == cursor && let Some(by) = &by[i] { span { class: "rv-by", "written by {by}" } }
                                if lines[i].kind == DiffKind::File && viewed.contains(&lines[i].text) {
                                    span { class: "rv-viewed", title: "Viewed: folded until its diff changes (V)", "✓ viewed" }
                                }
                            }
                        },
                        Row::Pair(old, new) if two => rsx! {
                            div { key: "{k}", class: "rv-row2",
                                if let Some(i) = old {
                                    LineCell { i, line: lines[i].clone(), number: numbers[i].old, marks: marks[i].clone(), class: line_class(lines[i].kind, i == cursor, marking && (first..=last_marked).contains(&i), !reading.standing_on(i).is_empty()), pane }
                                } else { div { class: "rv-line rv-empty" } }
                                if let Some(i) = new {
                                    LineCell { i, line: lines[i].clone(), number: numbers[i].new, by: by[i].clone(), marks: marks[i].clone(), class: line_class(lines[i].kind, i == cursor, marking && (first..=last_marked).contains(&i), !reading.standing_on(i).is_empty()), pane }
                                } else { div { class: "rv-line rv-empty" } }
                            }
                        },
                        Row::Pair(old, new) => rsx! {
                            for i in old.into_iter().chain(new) {
                                div { key: "{k}-{i}",
                                    class: line_class(lines[i].kind, i == cursor, marking && (first..=last_marked).contains(&i), !reading.standing_on(i).is_empty()),
                                    onclick: move |_| {
                                        app.dispatch(Msg::Focus(Area::Console));
                                        let mut pane = pane;
                                        pane.write().place(i);
                                    },
                                    // A right click puts the cursor on the line, whose menu then opens.
                                    oncontextmenu: move |_| {
                                        let mut pane = pane;
                                        pane.write().place(i);
                                    },
                                    span { class: "rv-no", "{numbers[i].old.map(|n| n.to_string()).unwrap_or_default()}" }
                                    NewNumber { number: numbers[i].new, by: by[i].clone() }
                                    Words { text: lines[i].text.clone(), marks: marks[i].clone() }
                                    if i == cursor && let Some(by) = &by[i] { span { class: "rv-by", "written by {by}" } }
                                }
                            }
                        },
                    }
                    for i in row_lines(*row) {
                        if lines[i].kind == DiffKind::File {
                            if let Some(change) = pictures.iter().find(|c| c.path == lines[i].text) {
                                crate::content::bodies::PictureDiff {
                                    environment: environment.clone(),
                                    worktree: worktree.clone(),
                                    rev: rev.clone(),
                                    change: change.clone(),
                                    how,
                                }
                            }
                        }
                        for judged in reading.ending_at(i).into_iter().filter(|j| !(hide_notes() && j.comment.is_note())) {
                            Thread { key: "{judged.comment.id}", judged: judged.clone(), at: reading.comment_at(cursor).is_some_and(|c| c.comment.id == judged.comment.id), outdated: false }
                        }
                    }
                }
                if !outdated.is_empty() {
                    div { class: "rv-outdated-title", "Outdated: the lines these were written on changed" }
                    for (k , judged) in outdated.iter().enumerate() {
                        Thread { key: "{judged.comment.id}", judged: (*judged).clone(), at: cursor == lines.len() + k, outdated: true }
                    }
                }
            }
            // One foot, so what is written, previewed or sought stacks above
            // the bar instead of each sticking to the same bottom edge.
            div { class: "rv-foot",
                if let Mode::Writing { target , text } = &state.mode {
                    div { class: "rv-composer",
                        span { class: "rv-label",
                            match target {
                                Target::Add { anchor , .. } => format!("Comment, {}", said(anchor)),
                                Target::Edit { .. } => "Edit the comment".to_string(),
                            }
                        }
                        InlineField {
                            class: "rv-input",
                            value: text.clone(),
                            placeholder: "What should change? Enter keeps it, Escape gives it up".to_string(),
                            focus: true,
                            contained: true,
                            oninput: { let mut pane = pane; move |text: String| pane.write().typed(text) },
                            onenter: { let this = this.clone(); let mut pane = pane; move |()| {
                                if let Some(change) = pane.write().finish(&this.rev) { this.apply(change); }
                            } },
                            onescape: { let mut pane = pane; move |()| pane.write().cancel() },
                        }
                    }
                }
                if let Mode::Previewing(preview) = &state.mode {
                    div { class: "rv-preview",
                        div { class: "rv-label", "Send to" }
                        if choices.is_empty() {
                            div { class: "rv-dim", "No session runs in this worktree: S prepares a new one" }
                        }
                        for (k , choice) in choices.iter().enumerate() {
                            div { key: "{choice.id}", class: if k == preview.choice.min(choices.len() - 1) { "rv-choice selected" } else { "rv-choice" },
                                "{choice.name}"
                                if choice.waiting { span { class: "rv-badge", "waiting on you" } }
                            }
                        }
                        pre { class: "rv-prompt", "{preview.text}" }
                        if let Some(error) = &preview.error { div { class: "git-error", "{error}" } }
                        div { class: "rv-hint",
                            if preview.busy { "Sending…" } else { "s send · S new session · j/k choose · q close" }
                        }
                    }
                }
                if let Some(text) = searching() {
                    div { class: "rv-composer",
                        span { class: "rv-label", "/" }
                        InlineField {
                            class: "rv-input",
                            value: text,
                            placeholder: "Find in the diff; Enter goes to it, Escape gives up".to_string(),
                            focus: true,
                            contained: true,
                            oninput: move |text: String| searching.set(Some(text)),
                            onenter: { let this = this.clone(); move |()| {
                                sought.set(searching.peek().clone().unwrap_or_default());
                                searching.set(None);
                                this.find(tsumugi_core::diff_view::Way::Next);
                            } },
                            onescape: move |()| searching.set(None),
                        }
                    }
                }
                div { class: "rv-bar",
                    if let Some(notice) = &state.notice { span { class: "rv-notice", "{notice}" } }
                    else { span { class: "rv-dim", "c comment · v mark lines · Enter edit · dd delete · r resolve · s send" } }
                    if !kept.is_empty() {
                        button { class: "rv-export",
                            title: "Copy, as JSON, which lines of these files agents wrote and who wrote each",
                            onclick: { let worktree = worktree.clone(); let mut pane = pane; move |_| {
                                let text = notes::exported(&worktree.to_string_lossy(), &kept);
                                pane.write().notice = Some(match crate::clipboard::copy_to_clipboard(&text) {
                                    Ok(()) => "Copied the lines agents wrote".into(),
                                    Err(e) => format!("cannot copy: {e}"),
                                });
                            } },
                            "Copy who wrote"
                        }
                    }
                    if all.iter().any(|j| j.comment.is_note()) {
                        button { class: if hide_notes() { "rv-notes" } else { "rv-notes on" },
                            title: "Show or hide the notes agents left",
                            onclick: move |_| { let now = !hide_notes(); hide_notes.set(now); },
                            if hide_notes() { "Notes hidden" } else { "Notes shown" }
                        }
                    }
                    if from_github > 0 {
                        span { class: "rv-dim", title: "Review threads of the pull request on GitHub; T takes the one under the cursor into your review", "{from_github} on GitHub" }
                    }
                    span { class: "rv-count", "{open} open · {kept_count - open} resolved" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_said_with_its_side() {
        let anchor = |start, end| notes::Anchor {
            path: "a".into(),
            side: notes::Side::Old,
            start,
            end,
        };
        assert_eq!(said(&anchor(3, 3)), "before, line 3");
        assert_eq!(said(&anchor(3, 5)), "before, lines 3-5");
    }
}
