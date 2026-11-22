//! One row of the tree: its marks, its name and what it says beside it, the
//! tabs that show it, and what a click or a right click on it does.

use std::collections::HashSet;
use std::path::Path;

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::overlay::context_menu::{Reach, Target};
use tsumugi_core::settings::Settings;
use tsumugi_core::tree;
use tsumugi_model::github::{Listing, Standing};
use tsumugi_model::{EnvironmentId, InboxItem, TerminalSession, Tree};

use super::signals::use_row_focus;
use super::summary::{WorkSummary, summarize};
use crate::bridge::AppCtx;
use crate::completion::Completions;
use crate::components::{
    AgentBadge, DraftMark, FileLook, FileRow, IconButton, ListRow, PlaceMark, SessionMark,
    StateGlyph, TerminalMark, glyph, runs_of, session_glyph, session_mark,
};
use crate::connection;
use crate::content::Tab;
use crate::files;
use crate::icons::{Icon, IconView};
use crate::nav::{self, Item};
use crate::rename::{Place as RenamePlace, SessionName};
use crate::shell::overlays::menu_opener;

/// What a sidebar row is, told when the pointer rests on it.
pub(crate) fn row_tip(item: &Item) -> &'static str {
    match item {
        Item::Environment(_) => {
            "An environment, how its connection stands and the work in it. Enter folds it"
        }
        Item::Workspace(_, _) => {
            "A workspace: a git repository or a directory sessions run in. Enter folds it; n starts a session there"
        }
        Item::Worktree(_, _) => {
            "A worktree: a checkout of the repository, on a branch of its own. A click folds or unfolds it; Enter or l unfolds it while folded, else opens its session; n starts a session in it, s a shell"
        }
        Item::Session(_, _) | Item::Waiting(_, _) => "A session. Enter opens it",
        Item::Terminal(_, _) => {
            "A terminal tsumugi runs. Enter shows it; closing its tab leaves it running"
        }
        Item::Browser(..) => "A page of the browser. Enter shows it; closing its tab ends the page",
        Item::Across(..) => {
            "What runs in every worktree of the repository, of one kind. Enter folds it"
        }
        Item::Gathered(own) => row_tip(own),
        Item::Heading(..) => "What runs in the worktree, of one kind. Enter folds it",
        Item::File { dir: true, .. } => {
            "A directory of the worktree. l goes into it, h folds it or the one it is in, Enter folds or unfolds it"
        }
        Item::File { .. } => "A file of the worktree",
        Item::GitHub(_, _, Listing::Issues) => {
            "Local issues and issues open on GitHub. Enter folds it"
        }
        Item::GitHub(..) => {
            "What is open on GitHub of the repository, as gh sees it there. Enter folds it"
        }
        Item::GitHubItem(_, _, Listing::Pulls, _) => {
            "A pull request open on GitHub. Enter shows it with its comments, g x opens it in the browser; n starts a session in a worktree at its head"
        }
        Item::GitHubItem(..) => {
            "An issue open on GitHub. Enter shows it with its comments, g x opens it in the browser; n starts a session on it"
        }
        Item::LocalIssue(..) => "A local issue. Enter shows its description; ▸ starts work on it",
        Item::GitHubFailed(..) => "Why gh could not say what is open on GitHub",
        Item::Pending(..) => {
            "A worktree being made. It goes once a session's draft opens in it; one that failed stays until x puts it away"
        }
    }
}

/// What the row of a worktree being made says: its name, the step it is
/// at or why one failed, and a mark of how it stands.
fn pending_look(
    pending: &tsumugi_core::worktree::Pending,
) -> (
    &'static str,
    String,
    String,
    Option<(&'static str, Icon, &'static str)>,
) {
    use tsumugi_core::worktree::{PIPELINE, Progress};
    let steps = pending.steps();
    let at = steps
        .iter()
        .position(|(_, progress)| matches!(progress, Progress::Running | Progress::Failed))
        .unwrap_or(0);
    let counted = format!("{}/{}", at + 1, PIPELINE.len());
    match &pending.failed {
        Some(why) => (
            "dim pending failed",
            pending.new.name.clone(),
            format!("{} failed · {why}", PIPELINE[at].doing()),
            Some(("state-glyph fail", Icon::X, "It could not be made")),
        ),
        None => (
            "dim pending",
            pending.new.name.clone(),
            format!("{} · {counted}", PIPELINE[at].doing()),
            Some(("state-glyph run", Icon::Loader, "Being made")),
        ),
    }
}

/// The mark of how a pull request's checks stand, drawn as a session's
/// state is.
fn checks_glyph(standing: Standing) -> (&'static str, Icon, &'static str) {
    match standing {
        Standing::Passed => ("state-glyph done", Icon::Check, "Every check passed"),
        Standing::Failed => ("state-glyph fail", Icon::X, "A check failed"),
        Standing::Pending => ("state-glyph run", Icon::Loader, "Checks are running"),
    }
}

/// The label of the agent a session's row runs, and the profile it started
/// from as `settings` name it.
pub(crate) fn row_badge(tree: &Tree, settings: &Settings, item: &Item) -> Option<String> {
    match item {
        Item::Session(_, id) | Item::Waiting(_, id) => nav::find(tree, id).map(|(_, s)| {
            settings.started_as(s.agent, &s.profile, s.home.as_deref(), &s.profile_name)
        }),
        _ => None,
    }
}

/// Whether `shown` shows the file at `path`: in its viewer, or in the Neovim
/// started on it.
pub(crate) fn shows_file(shown: Option<&Tab>, terminals: &[TerminalSession], path: &Path) -> bool {
    match shown {
        Some(Tab::Viewer(_, p)) => p == path,
        Some(Tab::Terminal(_, id)) => terminals
            .iter()
            .any(|t| t.id == *id && t.file.as_deref() == Some(path)),
        _ => false,
    }
}

fn short(path: &Path) -> String {
    tsumugi_core::picker::short(path, &crate::paths::home())
}

/// What rows share: the tree's structure, the sessions' states and the
/// drafts written for them. Each row follows its own selection separately.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RowContext {
    pub(crate) structure: ReadSignal<tree::TreeView>,
    pub(crate) completions: ReadSignal<Completions>,
    pub(crate) drafted: ReadSignal<HashSet<Tab>>,
}

/// What a row asks the window to do.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct RowEvents {
    /// Open the row as a click does.
    pub(crate) onopen: EventHandler<Item>,
    /// Take a workspace out of the tree, or close a worktree.
    pub(crate) onclose: EventHandler<Item>,
}

/// The `+` of an environment's heading: a menu of what starts work there,
/// opened under the pointer.
fn start_menu(app: AppCtx, item: Item) -> EventHandler<MouseEvent> {
    EventHandler::new(move |e: MouseEvent| {
        e.stop_propagation();
        let at = e.client_coordinates();
        app.dispatch(Msg::MenuOpen {
            x: at.x,
            y: at.y,
            target: Target::Start(item.clone()),
            reach: Reach::Here,
        });
    })
}

#[component]
pub(crate) fn TreeRow(item: Item, context: RowContext, on: RowEvents) -> Element {
    let app = use_context::<AppCtx>();
    let focus = use_row_focus(app.tree, item.clone());
    let focus = focus.read();
    let view = context.structure.read();
    let hosts = app.hosts.read();
    let work = hosts.work(item.environment());
    let t = tree::listed(work);
    let terminals = tree::terminals(work);
    let marks = work.map(|w| &w.tree.marks);
    let content = app.content.read();
    let content_now = &content.windows;
    let shows = content_now.focused().cloned();
    let it = item;
    let own = it.own().clone();
    let is_sel = focus.selected;
    let open = view.folds.is_open(&it);
    // The rows of the environments the tree is not at stand back; those
    // waiting on the user, gathered from all of them, do not.
    let elsewhere = !matches!(it, Item::Waiting(..))
        && hosts.each().nth(1).is_some()
        && *it.environment()
            != focus
                .environment
                .clone()
                .unwrap_or_else(|| app.shown_environment());
    if let Item::File {
        environment,
        worktree,
        path,
        dir,
    } = &it
    {
        // Hidden or ignored files stand back, as fern draws them.
        let back = files::is_hidden(path)
            || marks
                .and_then(|m| m.get(worktree))
                .is_some_and(|m| m.ignores(path));
        let look = FileLook {
            dir: *dir,
            back,
            shown: !*dir && shows_file(shows.as_ref(), terminals, path),
            editing: nav::edited(&it, terminals),
            elsewhere,
        };
        let status = marks
            .and_then(|m| m.get(worktree))
            .and_then(|m| m.of(path, *dir));
        // A file stands in its viewer's tabs and its Neovim's.
        let chips = if *dir {
            String::new()
        } else {
            let mut at = content_now.tabs_showing(&Tab::Viewer(environment.clone(), path.clone()));
            for t in terminals.iter().filter(|t| t.file.as_ref() == Some(path)) {
                at.extend(content_now.tabs_showing(&Tab::Terminal(environment.clone(), t.id)));
            }
            at.sort_unstable();
            at.dedup();
            at.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let here = it.clone();
        return rsx! {
            FileRow {
                path: path.clone(),
                depth: view.depth(&it),
                look,
                open,
                selected: is_sel,
                title: row_tip(&it).to_owned(),
                onclick: move |_| on.onopen.call(here.clone()),
                oncontextmenu: menu_opener(app, Target::Row(it.clone())),
                if !chips.is_empty() {
                    span { class: "tab-where", title: "The tabs showing it", "{chips}" }
                }
                if let Some(mark) = status {
                    span { class: if mark == 'A' { "file-status added" } else { "file-status" }, "{mark}" }
                }
            }
        };
    }
    let summary = summarize(
        if matches!(it, Item::Environment(_)) {
            t
        } else {
            view.narrowed(it.environment())
        },
        &context.completions.read(),
        &it,
    );
    let has_summary = summary.as_ref().is_some_and(|s| !s.is_empty());
    if let Item::Environment(environment) = &it {
        let standing = connection::standing_of(&app, environment);
        let kind = match (standing.mark == connection::Mark::Stopped, elsewhere) {
            (true, true) => "head environment dim elsewhere",
            (true, false) => "head environment dim",
            (false, true) => "head environment elsewhere",
            (false, false) => "head environment",
        };
        let here = it.clone();
        let kind = if has_summary {
            format!("{kind} with-summary")
        } else {
            kind.to_owned()
        };
        return rsx! {
            ListRow {
                kind,
                selected: is_sel,
                style: "padding-left:8px",
                title: row_tip(&it).to_owned(),
                onclick: move |_| on.onopen.call(here.clone()),
                oncontextmenu: menu_opener(app, Target::Row(it.clone())),
                PlaceMark { icon: if open { Icon::ChevronDown } else { Icon::ChevronRight } }
                connection::StandingDot { mark: standing.mark }
                span { class: "kind-mark", IconView { icon: standing.kind.icon(), size: 14 } }
                span { class: "list-row-name",
                    SessionName {
                        environment: environment.clone(), id: String::new(), name: app.environment_name(environment), place: RenamePlace::Heading,
                    }
                }
                if let Some(note) = standing.note {
                    connection::StandingPill { note }
                }
                if let Some(summary) = summary {
                    WorkSummary { summary }
                }
                IconButton {
                    icon: Icon::Plus,
                    place: "list-row-start",
                    title: "Start work here: a session, a shell, Neovim, or a directory added to the tree",
                    onclick: start_menu(app, it.clone()),
                }
            }
        };
    }
    let mark = match &own {
        Item::Session(_, id) => nav::find(t, id).and_then(|(_, s)| session_mark(s.reads_only())),
        _ => None,
    };
    let (kind, text, meta, glyph_class) = match &own {
        Item::Workspace(_, root) => {
            let name = t
                .workspace_of(root)
                .map(|w| w.name.clone())
                .unwrap_or_else(|| short(root));
            ("head", name, String::new(), None)
        }
        Item::Worktree(_, path) => {
            let node = t.worktrees().find(|w| &w.worktree.path == path);
            let label = node
                .and_then(|w| w.worktree.branch.clone())
                .or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()))
                .unwrap_or_else(|| short(path));
            let empty = node.is_none_or(|w| w.sessions.is_empty() && w.terminals.is_empty());
            (
                if empty { "dim mono vacant" } else { "dim mono" },
                label,
                String::new(),
                None,
            )
        }
        Item::Heading(_, _, heading) | Item::Across(_, _, heading) => {
            ("group", heading.name().to_string(), String::new(), None)
        }
        Item::Session(_, id) => {
            let s = nav::find(t, id).map(|(_, s)| s.clone());
            (
                "",
                s.as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| id.clone()),
                s.as_ref()
                    .and_then(|s| s.code.as_ref())
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                s.and_then(|s| session_glyph(s.rest, context.completions.read().state(&s))),
            )
        }
        Item::Terminal(_, id) => {
            let terminal = t.terminal(*id).map(|(_, _, s)| s);
            let name = terminal.map(|s| s.name.clone()).unwrap_or_default();
            let state =
                terminal.and_then(|s| context.completions.read().terminal_state(s).and_then(glyph));
            ("", name, String::new(), state)
        }
        Item::GitHub(_, root, listing) => {
            let count = tree::github(work, root)
                .and_then(|r| r.count(*listing))
                .unwrap_or_default()
                + if *listing == Listing::Issues {
                    work.map_or(0, |work| {
                        nav::issues_in(t, root, &work.tree.local_issues).len() as u64
                    })
                } else {
                    0
                };
            let count = if count > 0 {
                count.to_string()
            } else {
                String::new()
            };
            ("group", listing.name().to_string(), count, None)
        }
        Item::GitHubItem(_, root, listing, number) => {
            let found = tree::github(work, root).and_then(|r| r.item(*listing, *number));
            (
                if found.is_some_and(|i| i.draft) {
                    "github-item dim"
                } else {
                    "github-item"
                },
                found.map_or_else(|| format!("#{number}"), |i| i.title.clone()),
                found.map(|i| i.author.clone()).unwrap_or_default(),
                found
                    .and_then(|i| i.pull.as_ref())
                    .and_then(|p| p.checks.standing())
                    .map(checks_glyph),
            )
        }
        Item::LocalIssue(_, _, id) => {
            let issue =
                work.and_then(|work| work.tree.local_issues.iter().find(|issue| &issue.id == id));
            (
                "local-issue-row",
                issue.map_or_else(|| id.clone(), |issue| issue.title.clone()),
                issue.map_or_else(
                    || "Local".into(),
                    |issue| format!("Local · {}", issue.state.word()),
                ),
                None,
            )
        }
        Item::GitHubFailed(_, root, listing) => {
            let why = tree::github(work, root)
                .and_then(|r| r.read(*listing))
                .and_then(|read| read.as_ref().err())
                .cloned()
                .unwrap_or_default();
            ("dim", why, String::new(), None)
        }
        Item::Browser(_, _, id) => {
            let name = crate::browser::title(&Tab::Browser {
                environment: it.environment().clone(),
                worktree: std::path::PathBuf::new(),
                id: *id,
            });
            (
                "",
                name.unwrap_or_else(|| "New page".into()),
                String::new(),
                None,
            )
        }
        Item::Pending(_, _, id) => match view.making.iter().find(|p| p.id == *id) {
            Some(pending) => pending_look(pending),
            None => ("dim", String::new(), String::new(), None),
        },
        Item::Environment(_) | Item::File { .. } | Item::Waiting(_, _) | Item::Gathered(_) => {
            ("", String::new(), String::new(), None)
        }
    };
    let indent = 14 * view.depth(&it);
    // The tabs of the content that show it.
    let chips = match &own {
        Item::Session(environment, id) => {
            content_now.tabs_showing(&Tab::Session(environment.clone(), id.clone()))
        }
        Item::Terminal(environment, id) => {
            content_now.tabs_showing(&Tab::Terminal(environment.clone(), *id))
        }
        Item::Browser(environment, worktree, id) => content_now.tabs_showing(&Tab::Browser {
            environment: environment.clone(),
            worktree: worktree.clone(),
            id: *id,
        }),
        _ => Vec::new(),
    }
    .iter()
    .map(usize::to_string)
    .collect::<Vec<_>>()
    .join(" ");
    let row_title = row_tip(&it).to_owned();
    let terminal = match &own {
        Item::Terminal(_, id) => Some(runs_of(terminals, *id)),
        _ => None,
    };
    let place_icon = match &it {
        Item::Browser(..) => Some(Icon::World),
        Item::Workspace(_, root) if t.workspace_of(root).is_some_and(|w| w.scratch) => {
            Some(Icon::Notes)
        }
        Item::Workspace(_, _) if !open => Some(Icon::Folder),
        Item::Workspace(_, _) => Some(Icon::FolderOpen),
        Item::Worktree(_, _) => Some(Icon::GitBranch),
        Item::Across(..) | Item::Heading(..) | Item::GitHub(..) if open => Some(Icon::ChevronDown),
        Item::Across(..) | Item::Heading(..) | Item::GitHub(..) => Some(Icon::ChevronRight),
        _ => None,
    };
    // What the console shows stands out wherever the selection is.
    let kind = match &own {
        Item::Browser(environment, worktree, id)
            if shows
                == Some(Tab::Browser {
                    environment: environment.clone(),
                    worktree: worktree.clone(),
                    id: *id,
                }) =>
        {
            format!("{kind} shown")
        }
        Item::Session(environment, id)
            if shows == Some(Tab::Session(environment.clone(), id.clone())) =>
        {
            format!("{kind} shown")
        }
        Item::Terminal(environment, id)
            if shows == Some(Tab::Terminal(environment.clone(), *id)) =>
        {
            format!("{kind} shown")
        }
        _ => kind.to_string(),
    };
    let kind = if elsewhere {
        format!("{kind} elsewhere")
    } else {
        kind
    };
    // What finished or was marked unread, or holds what did, is drawn bold
    // until it is seen.
    let unread = {
        let marks = context.completions.read();
        match &own {
            Item::Session(_, id) => nav::find(t, id).is_some_and(|(_, s)| marks.unread(s)),
            Item::Terminal(_, id) => t.terminal(*id).is_some_and(|(_, _, s)| {
                marks.terminal_state(s) == Some(&tsumugi_model::State::Done)
            }),
            Item::Worktree(_, path) => t
                .worktrees()
                .find(|w| &w.worktree.path == path)
                .is_some_and(|w| w.sessions.iter().any(|s| marks.unread(s))),
            Item::Workspace(_, root) => t
                .workspace_of(root)
                .is_some_and(|w| w.sessions().any(|s| marks.unread(s))),
            _ => false,
        }
    };
    let kind = if unread {
        format!("{kind} unread")
    } else {
        kind
    };
    let muted = {
        let notices = app.notices.read();
        match &own {
            Item::Workspace(environment, path) | Item::Worktree(environment, path) => {
                notices.mutes.is_muted(environment, path)
            }
            _ => false,
        }
    };
    let pinned = matches!(&own, Item::Workspace(..) | Item::Worktree(..))
        && match &own {
            Item::Workspace(environment, path) | Item::Worktree(environment, path) => {
                view.organize.is_pinned(environment, path)
            }
            _ => false,
        };
    let sleeping = match &own {
        Item::Worktree(_, path) => t
            .worktrees()
            .find(|w| &w.worktree.path == path)
            .is_some_and(|w| w.sleeping()),
        _ => false,
    };
    let kind = if has_summary {
        format!("{kind} with-summary")
    } else {
        kind
    };
    let target = Target::Row(it.clone());
    // Where a session can start, a button starts one there.
    let starts = matches!(it, Item::Workspace(_, _) | Item::Worktree(_, _));
    // A workspace is taken out of the tree, a worktree closed, while
    // something runs there to hold its row up.
    let closes = match &it {
        Item::Workspace(_, _) => Some("Remove from the tree"),
        Item::Worktree(_, path) => t
            .worktrees()
            .find(|w| &w.worktree.path == path)
            .filter(|w| !w.sessions.is_empty() || !w.terminals.is_empty())
            .map(|_| "Close: end what runs here"),
        Item::Pending(_, _, id) => view
            .making
            .iter()
            .find(|p| p.id == *id && p.failed.is_some())
            .map(|_| "Put this row away"),
        _ => None,
    };
    let has_draft = match &own {
        Item::Session(environment, id) | Item::Waiting(environment, id) => context
            .drafted
            .read()
            .contains(&Tab::Session(environment.clone(), id.clone())),
        _ => false,
    };
    // A session listed twice takes its new name on one row: the selected
    // one, or else its worktree's.
    let renames_here = focus.renames_here;
    let badge = row_badge(t, &app.config.read().settings, &own);
    let here = it.clone();
    let close = it.clone();
    let local_issue = if let Item::LocalIssue(_, _, id) = &it {
        work.and_then(|work| work.tree.local_issues.iter().find(|issue| &issue.id == id))
            .cloned()
    } else {
        None
    };
    rsx! {
        ListRow {
            kind,
            selected: is_sel,
            style: "padding-left:{8 + indent}px",
            title: row_title,
            onclick: move |_| on.onopen.call(here.clone()),
            oncontextmenu: menu_opener(app, target),
            if let Some((g, icon, tip)) = glyph_class {
                StateGlyph { class: g, icon, size: 13, title: tip }
            }
            if let Some(icon) = place_icon {
                PlaceMark { icon }
            }
            if let Some(runs) = terminal { TerminalMark { runs } }
            span { class: "list-row-name",
                if let Item::Session(environment, id) = &own
                    && renames_here
                {
                    SessionName {
                        environment: environment.clone(), id: id.clone(), name: text.clone(), place: RenamePlace::Sidebar,
                    }
                } else { "{text}" }
            }
            SessionMark { mark }
            if pinned {
                span { class: "muted-mark", title: "Pinned: it stays at the top",
                    IconView { icon: Icon::Highlight, size: 12 }
                }
            }
            if sleeping {
                span { class: "muted-mark", title: "Asleep: its sessions' programs are stopped; Wake brings them back",
                    IconView { icon: Icon::SunMoon, size: 12 }
                }
            }
            if muted {
                span { class: "muted-mark", title: "Notifications are off; the row still shows how its work stands",
                    IconView { icon: Icon::BellOff, size: 12 }
                }
            }
            if has_draft {
                DraftMark {}
            }
            if let Some(summary) = summary {
                WorkSummary { summary }
            }
            if !meta.is_empty() {
                span { class: "list-row-meta", "{meta}" }
            }
            if !chips.is_empty() {
                span { class: "tab-where", title: "The tabs showing it", "{chips}" }
            }
            if let Some(label) = badge {
                AgentBadge { label }
            }
            if starts {
                IconButton {
                    icon: Icon::Plus,
                    place: "list-row-start",
                    title: "Start work here: a session, a local issue, a shell, or Neovim",
                    onclick: start_menu(app, it.clone()),
                }
            }
            if let Some(issue) = local_issue {
                crate::local_issues::StartIssue { environment: it.environment().clone(), issue: issue.clone() }
                crate::local_issues::DeleteIssue { environment: it.environment().clone(), issue }
            }
            if let Some(tip) = closes {
                IconButton {
                    icon: Icon::X,
                    place: "list-row-close",
                    title: tip,
                    onclick: move |e: Event<MouseData>| {
                        e.stop_propagation();
                        on.onclose.call(close.clone());
                    },
                }
            }
        }
    }
}

/// A session waiting on the user, listed again below the tree.
#[component]
pub(crate) fn WaitingRow(environment: EnvironmentId, waiting: InboxItem, on: RowEvents) -> Element {
    let app = use_context::<AppCtx>();
    let item = Item::Waiting(environment.clone(), waiting.session.id.clone());
    let selected = use_memo({
        let item = item.clone();
        move || app.tree.read().selected.as_ref() == Some(&item)
    });
    let is_sel = selected();
    let shown = app.content.read().active()
        == Some(&Tab::Session(
            environment.clone(),
            waiting.session.id.clone(),
        ));
    let badge = {
        let hosts = app.hosts.read();
        row_badge(
            tree::listed(hosts.work(&environment)),
            &app.config.read().settings,
            &item,
        )
    };
    let sub = format!("{} · {}", waiting.workspace, waiting.wait);
    let target = Target::Row(item.clone());
    rsx! {
        ListRow {
            kind: if shown { "shown" } else { "" },
            selected: is_sel,
            onclick: move |_| on.onopen.call(item.clone()),
            oncontextmenu: menu_opener(app, target),
            span { class: "state-glyph wait", title: "waiting", style: "margin-top:2px",
                IconView { icon: Icon::Bell, size: 13 }
            }
            div { class: "sidebar-inbox-text",
                span { class: "list-row-name",
                    "{waiting.session.name}"
                    SessionMark { mark: session_mark(waiting.session.reads_only()) }
                }
                span { class: "sidebar-inbox-sub", "{sub}" }
            }
            if let Some(label) = badge {
                AgentBadge { label }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tsumugi_model::EnvironmentId;
    use tsumugi_model::TerminalProgram;
    use tsumugi_model::testing::{session, tree};

    use super::*;
    use crate::shell::status_bar::mode_tip;

    #[test]
    fn a_file_is_shown_by_its_viewer_or_by_the_neovim_started_on_it() {
        let nvim = TerminalSession {
            id: 7,
            program: TerminalProgram::Neovim,
            file: Some("/w/a.rs".into()),
            ..Default::default()
        };
        let terminals = [nvim];
        let path = Path::new("/w/a.rs");
        assert!(shows_file(
            Some(&Tab::Terminal(EnvironmentId::unbound(), 7)),
            &terminals,
            path
        ));
        assert!(!shows_file(
            Some(&Tab::Terminal(EnvironmentId::unbound(), 8)),
            &terminals,
            path
        ));
        assert!(shows_file(
            Some(&Tab::Viewer(EnvironmentId::unbound(), "/w/b.png".into())),
            &terminals,
            Path::new("/w/b.png")
        ));
        assert!(!shows_file(None, &terminals, path));
    }

    #[test]
    fn sidebar_session_badges_identify_the_agent_even_when_names_match() {
        use tsumugi_model::Agent;

        let sessions = [Agent::Claude, Agent::Codex].map(|agent| {
            session(agent.key())
                .agent(agent)
                .name("Review")
                .cwd("/project")
        });
        let tree = tree().sessions(sessions).build();
        let settings = Settings::default();
        for (id, label) in [("claude", "Claude Code"), ("codex", "Codex")] {
            let badge = |item| row_badge(&tree, &settings, &item);
            assert_eq!(
                badge(Item::Session(EnvironmentId::unbound(), id.into())).as_deref(),
                Some(label)
            );
            assert_eq!(
                badge(Item::Waiting(EnvironmentId::unbound(), id.into())).as_deref(),
                Some(label)
            );
        }
        for item in [
            Item::Workspace(EnvironmentId::unbound(), "/project".into()),
            Item::Worktree(EnvironmentId::unbound(), "/project".into()),
            Item::Terminal(EnvironmentId::unbound(), 1),
            Item::Session(EnvironmentId::unbound(), "missing".into()),
        ] {
            assert_eq!(row_badge(&tree, &settings, &item), None);
        }
    }

    #[test]
    fn tips_that_hold_for_any_agent_name_the_session_rather_than_claude_code() {
        for tip in [
            mode_tip("DIRECT", false),
            row_tip(&Item::Session(EnvironmentId::unbound(), "s1".into())),
            row_tip(&Item::Waiting(EnvironmentId::unbound(), "s1".into())),
        ] {
            assert!(!tip.contains("Claude"), "{tip}");
        }
    }
}
