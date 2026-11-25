//! What is picked from a list drawn over the window: a session to go to or
//! a directory to start one in, and a past conversation to resume. One is
//! open at a time, and holds the keys while it is.

use std::path::{Path, PathBuf};

use tsumugi_model::{EnvironmentId, GitHubTyped, Key, TerminalProgram};
use tsumugi_protocol::{PastSight, PickSight, Request, WorkSnapshot};

use crate::actions::Action;
use crate::content::{self, Opening, TerminalAsk};
use crate::effect::Effect;
use crate::host::Hosts;
use crate::keymap::{Context, Step};
use crate::msg::Msg;
use crate::settings::PickerSource;
use crate::state::ViewState;
use crate::tree::Place;
use crate::tree::nav;
use crate::tree::{self, TreeView};
use crate::update::step;

mod choices;
pub mod commands;
pub mod filer;
pub mod fuzzy;
mod go;
pub mod interrupted;
mod list;
pub mod resume;
pub mod scopes;
pub mod sources;
#[cfg(test)]
mod tests;

pub use choices::*;
use commands::Commands;
use filer::{Filer, Row};
pub use go::*;
use interrupted::Waiting;
pub use list::*;
use resume::Resume;
pub use scopes::Aim;
pub use sources::{
    ActionOffer, BranchOffer, FileOffer, NewWorktreeOffer, ResourceOffer, WorktreeOffer,
};

/// The most sessions, and the most directories, the picker going to a
/// session lists at once.
const PICKS: usize = 40;

/// The most rows a scope of one source lists.
const ALONE: usize = 100;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PickerView {
    /// The home directory, which the paths drawn write as `~`.
    pub home: PathBuf,
    pub open: Option<Picking>,
}

/// The picker open.
#[derive(Debug, Clone, PartialEq)]
pub enum Picking {
    /// Go to a session, or start one.
    Go(Box<GoPicker>),
    /// Resume a conversation an agent kept.
    Resume(Resume),
    /// Bring back what the terminal host took with it.
    Interrupted(Waiting),
    /// Run, write, edit or delete a command the project keeps.
    Commands(Commands),
    /// Go through what needed the user.
    Notices(crate::notices::Listing),
}

impl Picking {
    /// What is typed to narrow the list, and the row the cursor is on.
    pub fn list(&self) -> &ListState {
        match self {
            Picking::Go(p) => &p.list,
            Picking::Resume(r) => &r.list,
            Picking::Interrupted(w) => &w.list,
            Picking::Commands(c) => &c.list,
            Picking::Notices(n) => &n.list,
        }
    }

    fn list_mut(&mut self) -> &mut ListState {
        match self {
            Picking::Go(p) => &mut p.list,
            Picking::Resume(r) => &mut r.list,
            Picking::Interrupted(w) => &mut w.list,
            Picking::Commands(c) => &mut c.list,
            Picking::Notices(n) => &mut n.list,
        }
    }

    pub fn cursor(&self) -> usize {
        self.list().cursor
    }
}

impl PickerView {
    /// What the host keeps listed for the picker going to a session.
    pub fn sight(&self) -> Option<PickSight> {
        match &self.open {
            Some(Picking::Go(p)) => Some(PickSight {
                listed: p.filer.expanded().map(Path::to_path_buf).collect(),
                ..PickSight::default()
            }),
            _ => None,
        }
    }

    /// What the host of `environment` keeps listed for the picker going to
    /// a session: that of the environment the directory tree browses lists
    /// its directories too, every other only what it offers; one that
    /// clones lists what is typed asks of GitHub.
    pub fn sight_in(&self, environment: &EnvironmentId) -> Option<PickSight> {
        let Some(Picking::Go(p)) = &self.open else {
            return None;
        };
        let mut sight = self.sight()?;
        if *environment != p.environment {
            sight = PickSight::default();
        }
        let aim = p.aim();
        let clones = p.offered.get(environment).is_some_and(|o| o.clones)
            && aim.sources.contains(&PickerSource::Directories);
        if let Some(place) = p.place.as_ref().filter(|pl| pl.environment == *environment) {
            if aim.sources.contains(&PickerSource::Files) {
                sight.files.insert(place.worktree.clone());
            }
            if aim.sources.contains(&PickerSource::Branches) {
                sight.branches.insert(place.worktree.clone());
            }
        }
        match GitHubTyped::parse(aim.query).filter(|_| clones) {
            Some(GitHubTyped::Owner(_)) => sight.accounts = true,
            Some(GitHubTyped::Repository { owner, .. }) => {
                sight.owners.insert(owner);
            }
            None => {}
        }
        Some(sight)
    }

    /// Which conversations the host of `environment` lists while they are
    /// picked from: only that of the environment the list is of does.
    pub fn past_sight(&self, environment: &EnvironmentId) -> Option<PastSight> {
        match &self.open {
            Some(Picking::Resume(r)) if r.environment == *environment => Some(r.sight()),
            _ => None,
        }
    }
}

/// The child directories of `dir` as the host listed them; none while it
/// has not.
fn listed_dirs(work: Option<&WorkSnapshot>, dir: &Path) -> Option<Vec<PathBuf>> {
    let entries = work?.tree.entries.get(dir)?;
    Some(
        entries
            .iter()
            .filter(|e| e.dir)
            .map(|e| e.path.clone())
            .collect(),
    )
}

/// What the picker going to a session lists from every environment
/// `hosts` holds, best first for what is typed, each session standing as
/// `tree` shows it; the directories alone while it adds to the tree. Each
/// source of the scope searched lists its rows in turn.
pub fn go_rows(picker: &GoPicker, home: &Path, hosts: &Hosts, tree: &TreeView) -> Vec<Choice> {
    let aim = picker.aim();
    let query = aim.query;
    let directories = aim.sources.contains(&PickerSource::Directories);
    if directories {
        let github = hosts.each().filter_map(|(environment, host)| {
            Some(GitHubOffers {
                environment,
                offers: picker.offered.get(environment)?,
                listed: &host.work.as_ref()?.pick,
            })
        });
        if let Some(rows) = github_choices(query, github, PICKS) {
            return rows;
        }
    }
    // A scope of one source has the room to itself.
    let cap = match aim.sources.len() {
        1 => ALONE,
        _ => PICKS,
    };
    let mut rows = Vec::new();
    for source in aim.sources {
        match source {
            PickerSource::Sessions => {
                let mut offers = Vec::new();
                for (environment, host) in hosts.each() {
                    offers.extend(sessions(
                        environment,
                        tree::listed(host.work.as_ref()),
                        &tree.completions,
                        &tree.changes,
                    ));
                }
                rows.extend(session_rows(&offers, query, cap));
            }
            PickerSource::Directories => {
                let mut found = Vec::new();
                for (environment, _) in hosts.each() {
                    let offered = picker.offered.get(environment).cloned().unwrap_or_default();
                    let running = picker
                        .running
                        .get(environment)
                        .map_or(&[][..], Vec::as_slice);
                    found.extend(candidates(
                        environment,
                        running,
                        &offered.recent,
                        &offered.ghq,
                        &offered.listed,
                    ));
                }
                rows.extend(directory_rows(&found, query, home, cap));
                let clones = hosts.each().filter_map(|(environment, _)| {
                    clone_offer(environment, picker.offered.get(environment)?, query)
                });
                let help =
                    github_help(query, picker.offered.values()).then_some(Choice::GitHubHelp);
                rows.extend(clones.map(Choice::Clone).chain(help));
            }
            PickerSource::Worktrees => {
                rows.extend(sources::worktrees(hosts, picker.place.as_ref(), query, cap))
            }
            PickerSource::Resources => {
                rows.extend(sources::resources(hosts, picker.place.as_ref(), query, cap));
            }
            PickerSource::Files => {
                rows.extend(sources::files(hosts, picker.place.as_ref(), query, cap));
            }
            PickerSource::Actions => rows.extend(sources::actions(query, cap)),
            PickerSource::Branches => {
                rows.extend(sources::branches(hosts, picker.place.as_ref(), query, cap));
            }
        }
    }
    match picker.purpose {
        Purpose::Go => rows,
        Purpose::Add => rows
            .into_iter()
            .filter(|c| !matches!(c, Choice::Scratch | Choice::Session(_)))
            .collect(),
    }
}

/// The rows of [`go_rows`] as drawn: with more than one environment shown,
/// each session and directory names the environment it is in.
pub fn go_lines(picker: &GoPicker, home: &Path, hosts: &Hosts, tree: &TreeView) -> Vec<Line> {
    let several = hosts.each().nth(1).is_some();
    labelled_lines(&go_rows(picker, home, hosts, tree), home, |environment| {
        several.then(|| hosts.name(environment))
    })
}

/// The rows of the picker's directory tree, each open directory's as the
/// host listed them.
pub fn tree_rows(picker: &GoPicker, work: Option<&WorkSnapshot>) -> Vec<Row> {
    picker
        .filer
        .rows(|dir| listed_dirs(work, dir).unwrap_or_default())
}

/// Open the picker for `purpose` on the sessions and the directories the
/// tree lists; what else is offered follows once the host lists it. The
/// home directory starts open: nearly every pick in the tree begins inside
/// it.
pub(crate) fn open(state: &mut ViewState, purpose: Purpose) -> Vec<Effect> {
    let running = state
        .hosts
        .each()
        .map(|(environment, host)| {
            let running = tree::listed(host.work.as_ref())
                .worktrees()
                .map(|w| w.worktree.path.clone())
                .collect();
            (environment.clone(), running)
        })
        .collect();
    let environment = content::focused_environment(state);
    let work = state.hosts.work(&environment);
    let home = home_of(state, &environment);
    let mut picker = GoPicker {
        running,
        purpose,
        environment,
        home: home.clone(),
        scopes: state.config.settings.picker.scopes.clone(),
        place: state.content.place.clone(),
        ..GoPicker::new(Filer::new([home]))
    };
    enter(&mut picker, work);
    state.picker.open = Some(Picking::Go(Box::new(picker)));
    Vec::new()
}

/// Open the list of past conversations.
pub(crate) fn open_resume(state: &mut ViewState) -> Vec<Effect> {
    state.picker.open = Some(Picking::Resume(resume::open(state)));
    Vec::new()
}

/// Open the list of what the terminal host of `environment` took with it.
pub(crate) fn open_interrupted(state: &mut ViewState, environment: EnvironmentId) -> Vec<Effect> {
    state.picker.open = Some(Picking::Interrupted(Waiting {
        environment,
        ..Waiting::default()
    }));
    Vec::new()
}

pub(crate) fn close(view: &mut PickerView) -> Vec<Effect> {
    view.open = None;
    Vec::new()
}

/// Narrow the list by `text` from now on, from the first row when it
/// changed.
pub(crate) fn query(view: &mut PickerView, text: String) -> Vec<Effect> {
    if let Some(open) = &mut view.open {
        open.list_mut().set_query(text);
    }
    Vec::new()
}

/// Search the scope `at` of the picker going to a session, what is typed
/// staying.
pub(crate) fn scope(view: &mut PickerView, at: usize) -> Vec<Effect> {
    if let Some(Picking::Go(p)) = &mut view.open {
        p.set_scope(at);
    }
    Vec::new()
}

/// List every directory's conversations, or the directory's again.
pub(crate) fn every(view: &mut PickerView) -> Vec<Effect> {
    if let Some(Picking::Resume(r)) = &mut view.open {
        r.toggle_every();
    }
    Vec::new()
}

/// A key the picker's query field does not type with, or any key in its
/// directory tree.
pub(crate) fn press(state: &mut ViewState, key: &Key) -> Vec<Effect> {
    if matches!(state.picker.open, Some(Picking::Commands(_))) {
        return commands::press(state, key);
    }
    let ctrl = key.ctrl && !key.alt && !key.meta;
    if ctrl
        && matches!(state.picker.open, Some(Picking::Resume(_)))
        && let Some(effects) = resume::act(state, &key.key)
    {
        return effects;
    }
    let browsed = browsed(state);
    let work = state.hosts.work(&browsed);
    let tree = matches!(&state.picker.open, Some(Picking::Go(p)) if p.mode == Mode::Tree);
    let contexts: &[Context] = match tree {
        true => &[Context::Filer, Context::Overlay],
        false => &[Context::Overlay],
    };
    let action = match state.chrome.keymap.resolve(contexts, &[], key) {
        Step::Run(action) => Some(action),
        Step::Wait | Step::Nothing => None,
    };
    if action == Some(Action::OverlayClose) {
        return close(&mut state.picker);
    }
    if let Some(then) = Then::of(key)
        && matches!(state.picker.open, Some(Picking::Go(_)))
    {
        if let Some(offer) = clone_under_cursor(state) {
            return clone(state, offer, then);
        }
        return match under_cursor(state) {
            Some((environment, dir)) => then_in(state, then, environment, dir),
            None => Vec::new(),
        };
    }
    let Some(open) = &mut state.picker.open else {
        return Vec::new();
    };
    match open {
        Picking::Go(p) if ctrl && key.key == "f" => {
            p.toggle_mode();
            return Vec::new();
        }
        Picking::Go(p) if ctrl && key.key == "o" && p.mode == Mode::List => {
            let under = go_rows(p, &state.picker.home, &state.hosts, &state.tree)
                .into_iter()
                .nth(p.list.cursor);
            return match under {
                Some(Choice::Branch(b)) => {
                    close(&mut state.picker);
                    vec![Effect::SwitchBranch {
                        environment: b.environment,
                        worktree: b.worktree,
                        to: b.point,
                    }]
                }
                _ => Vec::new(),
            };
        }
        Picking::Go(p) if ctrl && key.key.eq_ignore_ascii_case("g") && p.mode == Mode::List => {
            p.step_scope(if key.shift { -1 } else { 1 });
            return Vec::new();
        }
        Picking::Go(p) if p.mode == Mode::Tree => {
            let Some(action) = action else {
                return Vec::new();
            };
            return match tree_key(p, &action, work) {
                Some(dir) => start_in_environment(state, browsed, dir),
                None => Vec::new(),
            };
        }
        Picking::Resume(r) if ctrl && key.key == "a" => {
            r.toggle_every();
            return Vec::new();
        }
        Picking::Resume(r) if ctrl && key.key == "g" => {
            r.cycle_group();
            return Vec::new();
        }
        Picking::Notices(_) if ctrl && key.key == "u" => return crate::notices::toggle(state),
        Picking::Notices(_) if ctrl && key.key == "a" => return crate::notices::read_all(state),
        _ => {}
    }
    let delta = match action {
        Some(Action::OverlayChoose) => {
            let at = open.cursor();
            return choose(state, at);
        }
        Some(Action::OverlayDown) => 1,
        Some(Action::OverlayUp) => -1,
        _ => return Vec::new(),
    };
    let len = rows_len(&state.picker, &state.hosts, &state.tree, &state.notices);
    if let Some(open) = &mut state.picker.open {
        open.list_mut().step(delta, len);
    }
    Vec::new()
}

fn rows_len(
    view: &PickerView,
    hosts: &Hosts,
    tree: &TreeView,
    notices: &crate::notices::NoticeView,
) -> usize {
    match &view.open {
        Some(Picking::Go(p)) => go_rows(p, &view.home, hosts, tree).len(),
        Some(Picking::Resume(r)) => {
            resume::rows(r, &view.home, hosts.work(&r.environment)).map_or(0, |r| r.len())
        }
        Some(Picking::Interrupted(w)) => interrupted::rows(w, &view.home, hosts).len(),
        Some(Picking::Commands(c)) => commands::rows(c, hosts).len(),
        Some(Picking::Notices(n)) => crate::notices::rows(notices, hosts, &n.list.query).len(),
        None => 0,
    }
}

/// The filer's action in the directory tree; the directory chosen, when
/// one is.
fn tree_key(
    picker: &mut GoPicker,
    action: &Action,
    work: Option<&WorkSnapshot>,
) -> Option<PathBuf> {
    let rows = tree_rows(picker, work);
    let filer = &mut picker.filer;
    match action {
        Action::FilerDown => filer.down(&rows),
        Action::FilerUp => filer.up(&rows),
        Action::FilerInto => enter(picker, work),
        Action::FilerOut => filer.close(&rows),
        Action::FilerHidden => filer.toggle_hidden(),
        Action::FilerOpen => return filer.chosen(),
        _ => {}
    }
    None
}

/// Open the directory under the tree's cursor and move onto its first
/// child; into one the host has not listed yet, once it has.
fn enter(picker: &mut GoPicker, work: Option<&WorkSnapshot>) {
    let Some(at) = picker.filer.cursor().cloned() else {
        return;
    };
    let listed = listed_dirs(work, &at.path);
    picker.filer.open(|_| listed.clone().unwrap_or_default());
    picker.entering = listed.is_none().then_some(at);
}

/// The row `at` chosen, by its place among those drawn.
pub(crate) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let chosen = match &state.picker.open {
        Some(Picking::Go(p)) => go_rows(p, &state.picker.home, &state.hosts, &state.tree)
            .into_iter()
            .nth(at),
        Some(Picking::Resume(_)) => return resume::choose(state, at),
        Some(Picking::Interrupted(_)) => return interrupted::choose(state, at),
        Some(Picking::Commands(_)) => return commands::choose(state, at),
        Some(Picking::Notices(_)) => return crate::notices::choose(state, at),
        None => None,
    };
    match chosen {
        Some(Choice::Scratch) => {
            let environment = browsed(state);
            let scratch = scratch_of(state, &environment);
            start_in_environment(state, environment, scratch)
        }
        Some(Choice::Directory(c)) => start_in_environment(state, c.environment, c.path),
        Some(Choice::Clone(offer)) => {
            let then = match &state.picker.open {
                Some(Picking::Go(p)) if p.purpose == Purpose::Add => Then::Keep,
                _ => Then::Start,
            };
            clone(state, offer, then)
        }
        Some(Choice::Session(s)) => {
            close(&mut state.picker);
            content::show_session(state, s.environment, s.id)
        }
        Some(Choice::Worktree(w)) => {
            close(&mut state.picker);
            go_to_worktree(state, w)
        }
        Some(Choice::NewWorktree(n)) => {
            close(&mut state.picker);
            let row = nav::Item::Worktree(n.environment, n.from);
            let effects = crate::worktree::composer::open(state, &row);
            if let Some(composer) = state.overlays.composer.as_mut() {
                composer.name = n.name;
            }
            effects
        }
        Some(Choice::Resource(r)) => {
            close(&mut state.picker);
            vec![open_resource(&r)]
        }
        Some(Choice::File(f)) => {
            close(&mut state.picker);
            vec![Effect::OpenFile {
                environment: f.environment.clone(),
                path: f.absolute(),
                how: Opening::Existing,
            }]
        }
        Some(Choice::Action(a)) => {
            close(&mut state.picker);
            match a.action() {
                Some(action) => step(
                    state,
                    Msg::Perform {
                        action,
                        key: None,
                        probe: Box::default(),
                    },
                ),
                None => Vec::new(),
            }
        }
        Some(Choice::Branch(b)) => {
            close(&mut state.picker);
            go_to_branch(state, b)
        }
        Some(Choice::Owner(owner)) => query(&mut state.picker, GitHubTyped::owner_query(&owner)),
        Some(Choice::GitHubHelp) => query(&mut state.picker, GitHubTyped::PREFIX.to_string()),
        None => Vec::new(),
    }
}

/// Go to the branch `b`: to the worktree it is checked out in, else to a
/// worktree made to check it out.
fn go_to_branch(state: &mut ViewState, b: sources::BranchOffer) -> Vec<Effect> {
    if let Some(path) = b.checked_out.clone() {
        return go_to_worktree(
            state,
            WorktreeOffer {
                environment: b.environment,
                path,
                name: String::new(),
                place: String::new(),
            },
        );
    }
    let new = tsumugi_model::NewWorktree {
        name: b.local(),
        start: b.point,
        branch: None,
        checkout: true,
    };
    crate::worktree::begin_in(state, b.environment, b.worktree, new)
}

/// Open the picker on the branches of the repository of the worktree at
/// `place`: stepped to the first scope that offers branches alone, else to
/// the first that offers them at all.
pub(crate) fn open_branches(state: &mut ViewState, place: Place) -> Vec<Effect> {
    let effects = open(state, Purpose::Go);
    if let Some(Picking::Go(p)) = &mut state.picker.open {
        let alone = p
            .scopes
            .iter()
            .position(|s| s.sources == [PickerSource::Branches]);
        let any = || {
            p.scopes
                .iter()
                .position(|s| s.sources.contains(&PickerSource::Branches))
        };
        if let Some(at) = alone.or_else(any) {
            p.scope = at;
        }
        p.place = Some(place);
    }
    effects
}

/// Go to the worktree `w` in the tree: its workspace opened and its row
/// selected, the keys left where they were.
fn go_to_worktree(state: &mut ViewState, w: WorktreeOffer) -> Vec<Effect> {
    let root = tree::listed(state.hosts.work(&w.environment))
        .workspace_of(&w.path)
        .map(|ws| ws.root.clone());
    let folds = &mut state.tree.folds;
    if let Some(root) = root {
        folds.set(&nav::Item::Workspace(w.environment.clone(), root), true);
    }
    let row = nav::Item::Worktree(w.environment, w.path);
    folds.set(&row, true);
    state.tree.selected = Some(row);
    Vec::new()
}

/// What opening `offer` asks of the window, as opening a pinned row of the
/// details panel does: an address as a page of the browser, a path in its
/// viewer or Neovim.
fn open_resource(offer: &ResourceOffer) -> Effect {
    let resource = &offer.resource;
    let environment = offer.environment.clone();
    let web = resource.target.starts_with("http://") || resource.target.starts_with("https://");
    match resource.reach() {
        tsumugi_model::Reach::Path => Effect::OpenFile {
            environment,
            path: PathBuf::from(&resource.target),
            how: Opening::Existing,
        },
        tsumugi_model::Reach::Address if web => Effect::OpenAddress {
            environment,
            worktree: resource.project.clone(),
            url: resource.target.clone(),
            how: Opening::Existing,
        },
        tsumugi_model::Reach::Address => Effect::Open {
            environment,
            target: resource.target.clone(),
        },
    }
}

/// A directory chosen as the directory tree browses it, in the environment
/// it browses: a new session's draft there, its first prompt to write.
pub(crate) fn start_in(state: &mut ViewState, dir: PathBuf) -> Vec<Effect> {
    let environment = browsed(state);
    start_in_environment(state, environment, dir)
}

/// The environment the directory tree browses, the window's own while the
/// picker is not open on it.
fn browsed(state: &ViewState) -> EnvironmentId {
    match &state.picker.open {
        Some(Picking::Go(p)) => p.environment.clone(),
        _ => state.hosts.environment.clone(),
    }
}

/// Where a scratch session of `environment` runs, as its host says.
fn scratch_of(state: &ViewState, environment: &EnvironmentId) -> PathBuf {
    if *environment == state.hosts.environment {
        return state.content.scratch().to_path_buf();
    }
    state
        .hosts
        .work(environment)
        .and_then(|w| w.config.configured.as_ref())
        .map(|c| c.scratch.clone())
        .unwrap_or_else(|| state.content.scratch().to_path_buf())
}

/// The home directory of `environment`'s host, as it says; the window's own
/// while it has not said, which is the host's when both run on one computer.
fn home_of(state: &ViewState, environment: &EnvironmentId) -> PathBuf {
    said_home(state, environment).unwrap_or_else(|| state.picker.home.clone())
}

/// The home directory of `environment`'s host as it says, or the window's
/// own for the window's own environment; none for another environment
/// whose host has not said, whose home may be on another machine.
pub(crate) fn home_in(state: &ViewState, environment: &EnvironmentId) -> Option<PathBuf> {
    said_home(state, environment)
        .or_else(|| (*environment == state.hosts.environment).then(|| state.picker.home.clone()))
}

fn said_home(state: &ViewState, environment: &EnvironmentId) -> Option<PathBuf> {
    state
        .hosts
        .work(environment)
        .and_then(|w| w.config.configured.as_ref())
        .map(|c| c.home.clone())
        .filter(|home| !home.as_os_str().is_empty())
}

/// A directory of `environment` chosen: a new session's draft there, its
/// first prompt to write, or the directory kept in the tree while the picker
/// adds to it.
fn start_in_environment(
    state: &mut ViewState,
    environment: EnvironmentId,
    dir: PathBuf,
) -> Vec<Effect> {
    if let Some(Picking::Go(p)) = &state.picker.open
        && p.purpose == Purpose::Add
    {
        return then_in(state, Then::Keep, environment, dir);
    }
    close(&mut state.picker);
    content::draft(&mut state.content, environment, dir, true)
}

/// What a row's directory is taken for: a new session's draft there, as
/// choosing it does, a shell or Neovim there, or its workspace kept in the
/// tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Start,
    Shell,
    Neovim,
    Keep,
}

impl Then {
    /// Bound as `Mod+s` and `Mod+e` open a shell and Neovim elsewhere.
    fn of(key: &Key) -> Option<Self> {
        if !key.ctrl || key.alt || key.meta || key.shift {
            return None;
        }
        Some(match key.key.as_str() {
            "s" => Then::Shell,
            "e" => Then::Neovim,
            "t" => Then::Keep,
            _ => return None,
        })
    }
}

/// The directory of `environment` the row under the cursor stands for: in
/// the list, a directory's own, the worktree a session runs in, or the
/// scratch folder; in the directory tree, the directory selected.
fn under_cursor(state: &ViewState) -> Option<(EnvironmentId, PathBuf)> {
    let Some(Picking::Go(p)) = &state.picker.open else {
        return None;
    };
    if p.mode == Mode::Tree {
        return Some((p.environment.clone(), p.filer.cursor()?.path.clone()));
    }
    match go_rows(p, &state.picker.home, &state.hosts, &state.tree)
        .into_iter()
        .nth(p.list.cursor)?
    {
        Choice::Scratch => {
            let environment = browsed(state);
            let scratch = scratch_of(state, &environment);
            Some((environment, scratch))
        }
        Choice::Session(s) => {
            let tree = tree::listed(state.hosts.work(&s.environment));
            let (worktree, _) = tree::nav::find(tree, &s.id)?;
            Some((s.environment.clone(), worktree.to_path_buf()))
        }
        Choice::Directory(c) => Some((c.environment, c.path)),
        Choice::Worktree(w) => Some((w.environment, w.path)),
        Choice::Branch(b) => b.checked_out.map(|path| (b.environment, path)),
        Choice::Clone(_)
        | Choice::Owner(_)
        | Choice::GitHubHelp
        | Choice::NewWorktree(_)
        | Choice::Resource(_)
        | Choice::File(_)
        | Choice::Action(_) => None,
    }
}

/// The repository the row under the cursor offers to clone, if it offers one.
fn clone_under_cursor(state: &ViewState) -> Option<CloneOffer> {
    let Some(Picking::Go(p)) = &state.picker.open else {
        return None;
    };
    if p.mode == Mode::Tree {
        return None;
    }
    match go_rows(p, &state.picker.home, &state.hosts, &state.tree)
        .into_iter()
        .nth(p.list.cursor)?
    {
        Choice::Clone(offer) => Some(offer),
        _ => None,
    }
}

/// Clone the repository `offer` names, the picker closed, its directory to
/// be taken for `then` once it is cloned.
fn clone(state: &mut ViewState, offer: CloneOffer, then: Then) -> Vec<Effect> {
    close(&mut state.picker);
    vec![Effect::Clone {
        environment: offer.environment,
        target: offer.target,
        then,
    }]
}

/// The repository cloned into `dir` of `environment` kept in the tree, then
/// taken for `then`, as its row was chosen, whatever the window shows by now.
pub(crate) fn cloned(
    state: &mut ViewState,
    environment: EnvironmentId,
    dir: PathBuf,
    then: Then,
) -> Vec<Effect> {
    if then == Then::Keep {
        return take(state, then, environment, dir);
    }
    let mut effects = take(state, Then::Keep, environment.clone(), dir.clone());
    effects.extend(take(state, then, environment, dir));
    effects
}

/// `then` done in `dir` of `environment`, the picker closed.
fn then_in(
    state: &mut ViewState,
    then: Then,
    environment: EnvironmentId,
    dir: PathBuf,
) -> Vec<Effect> {
    close(&mut state.picker);
    take(state, then, environment, dir)
}

fn take(
    state: &mut ViewState,
    then: Then,
    environment: EnvironmentId,
    dir: PathBuf,
) -> Vec<Effect> {
    let runs = match then {
        Then::Start => return content::draft(&mut state.content, environment, dir, true),
        Then::Shell => TerminalProgram::Shell,
        Then::Neovim => TerminalProgram::Neovim,
        Then::Keep => {
            return vec![Effect::Host {
                environment,
                request: Request::KeepWorkspace { dir },
            }];
        }
    };
    vec![Effect::StartTerminal(TerminalAsk {
        place: Place::at(&environment, dir),
        runs,
        file: None,
        opening: Opening::Existing,
        focus: true,
    })]
}

/// What each host offered taken into the list, and what the host of the
/// environment the tree browses offered into the tree: the tree lists the
/// roots offered after those it has, what is open in it staying open.
pub(crate) fn take_offers(view: &mut PickerView, hosts: &Hosts) {
    let Some(Picking::Go(picker)) = &mut view.open else {
        return;
    };
    let offered: std::collections::BTreeMap<EnvironmentId, tsumugi_protocol::Offers> = hosts
        .each()
        .filter_map(|(environment, host)| {
            let offers = host.work.as_ref()?.pick.offers.clone()?;
            Some((environment.clone(), offers))
        })
        .collect();
    if picker.offered == offered {
        return;
    }
    let roots = offered.get(&picker.environment).cloned();
    let roots_new = picker.offered.get(&picker.environment) != roots.as_ref();
    picker.offered = offered;
    if let Some(roots) = roots.filter(|_| roots_new) {
        picker.filer.add_roots(roots.roots.iter().cloned());
    }
}

/// The cursor stays on the row it was on while the rows move under it, as
/// sessions change state or a host offers more, not on whatever comes to sit
/// at its place; on the first row once that row is gone. Moving it, or
/// typing, puts it where that says.
pub(crate) fn keep_cursor(view: &mut PickerView, hosts: &Hosts, tree: &TreeView) {
    let PickerView { home, open } = view;
    let Some(Picking::Go(picker)) = open else {
        return;
    };
    let rows = go_rows(picker, home, hosts, tree);
    if let Some((list, on)) = &picker.held
        && *list == picker.list
        && rows.get(picker.list.cursor).is_none_or(|c| !c.same_row(on))
    {
        picker.list.cursor = rows.iter().position(|c| c.same_row(on)).unwrap_or(0);
    }
    picker.held = rows
        .get(picker.list.cursor)
        .map(|c| (picker.list.clone(), c.clone()));
}

/// A directory opened before the host listed it is moved into once it
/// lists it, while the cursor is still on it.
pub(crate) fn take_listed(view: &mut PickerView, hosts: &Hosts) {
    let Some(Picking::Go(picker)) = &mut view.open else {
        return;
    };
    let work = hosts.work(&picker.environment);
    let Some(at) = picker.entering.clone() else {
        return;
    };
    let Some(listed) = listed_dirs(work, &at.path) else {
        return;
    };
    picker.entering = None;
    if picker.filer.cursor() == Some(&at) {
        picker.filer.open(|_| listed.clone());
    }
}
