//! Cleaning up worktrees: a list of those of a repository with what is
//! weighed before deleting each, from which several are chosen and deleted
//! together. What loses work is deleted only after a question says so.

use std::path::{Path, PathBuf};

use tsumugi_model::{BranchFate, CleanupEntry, EnvironmentId, Key, Session};
use tsumugi_protocol::Request;

use crate::actions::Action;
use crate::chrome::status_line;
use crate::cursor;
use crate::effect::Effect;
use crate::overlay::confirm::Ask;
use crate::overlay::{Holder, action_over};
use crate::state::ViewState;
use crate::tree::{self, nav::Item};

/// One worktree of the list, and whether it is chosen to go.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub entry: CleanupEntry,
    pub selected: bool,
}

impl Row {
    /// Whether it can be chosen at all: git keeps a locked worktree.
    pub fn choosable(&self) -> bool {
        !self.entry.standing.locked
    }

    /// Whether deleting it loses nothing that is on disk alone: no changed
    /// files, no commits no remote has, and a branch to find them by.
    pub fn safe(&self) -> bool {
        let s = &self.entry.standing;
        self.choosable() && s.changed == 0 && s.unpushed == 0 && s.branch.is_some()
    }

    /// What is weighed of it, in words, `now` being the time in seconds since
    /// the epoch: files changed, commits not pushed, whether it is merged,
    /// its pull request, its size and the age of its last commit.
    pub fn facts(&self, now: u64) -> Vec<String> {
        let e = &self.entry;
        let mut facts = Vec::new();
        if e.standing.locked {
            facts.push("locked".into());
        }
        if e.standing.branch.is_none() {
            facts.push("no branch".into());
        }
        if e.standing.changed > 0 {
            facts.push(format!("{} changed", counted(e.standing.changed, "file")));
        }
        if e.standing.unpushed > 0 {
            facts.push(format!(
                "{} not pushed",
                counted(e.standing.unpushed, "commit")
            ));
        }
        if e.merged && e.standing.branch.is_some() {
            facts.push("merged into the base".into());
        }
        if let Some(pull) = &e.pull {
            let state = match pull.state {
                tsumugi_model::PullState::Open => "open",
                tsumugi_model::PullState::Merged => "merged",
                tsumugi_model::PullState::Closed => "closed",
            };
            facts.push(format!("#{} {state}", pull.number));
        }
        facts.push(size(e.bytes));
        if let Some(at) = e.last_commit {
            facts.push(format!("{} ago", span(now.saturating_sub(at))));
        }
        facts
    }

    /// Whether it is chosen beforehand: its work is done with, as its pull
    /// request says, and nothing on disk is lost.
    fn suggested(&self) -> bool {
        self.entry.finished() && self.safe()
    }
}

/// `n` of `what`, said as a count.
fn counted(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// A size in bytes, in the largest unit that keeps it above one.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// A span in seconds, in the largest whole unit.
fn span(seconds: u64) -> String {
    match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86_400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// What the host has said of the worktrees.
#[derive(Debug, Clone, Default, PartialEq)]
enum Rows {
    #[default]
    Reading,
    Read(Vec<Row>),
    Failed(String),
}

/// The list of worktrees to clean up, open over the window.
#[derive(Debug, Clone, PartialEq)]
pub struct Cleanup {
    pub environment: EnvironmentId,
    /// The repository's main checkout, whose worktrees are listed.
    pub repository: PathBuf,
    /// The workspace's name, for the title.
    pub project: String,
    pub cursor: usize,
    rows: Rows,
}

impl Cleanup {
    fn new(environment: EnvironmentId, repository: PathBuf, project: String) -> Self {
        Self {
            environment,
            repository,
            project,
            cursor: 0,
            rows: Rows::Reading,
        }
    }

    pub fn rows(&self) -> &[Row] {
        match &self.rows {
            Rows::Read(rows) => rows,
            Rows::Reading | Rows::Failed(_) => &[],
        }
    }

    fn rows_mut(&mut self) -> &mut [Row] {
        match &mut self.rows {
            Rows::Read(rows) => rows,
            Rows::Reading | Rows::Failed(_) => &mut [],
        }
    }

    /// Whether the host has not answered yet.
    pub fn reading(&self) -> bool {
        matches!(self.rows, Rows::Reading)
    }

    /// Why the host could not list the worktrees.
    pub fn failed(&self) -> Option<&str> {
        match &self.rows {
            Rows::Failed(why) => Some(why),
            _ => None,
        }
    }

    /// How many are chosen.
    pub fn chosen(&self) -> usize {
        self.rows().iter().filter(|r| r.selected).count()
    }

    /// What the chosen ones take of the disk, in bytes.
    pub fn reclaimed(&self) -> u64 {
        self.rows()
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.entry.bytes)
            .sum()
    }
}

/// The rows of the entries the host listed: what goes first, then what has
/// been left alone the longest. The main checkout is never one.
fn rows_of(entries: Vec<CleanupEntry>) -> Vec<Row> {
    let mut rows: Vec<Row> = entries
        .into_iter()
        .filter(|e| !e.standing.main)
        .map(|entry| Row {
            entry,
            selected: false,
        })
        .collect();
    for row in &mut rows {
        row.selected = row.suggested();
    }
    rows.sort_by_key(|r| (!r.selected, r.entry.last_commit.unwrap_or(0)));
    rows
}

/// Open the list from the tree's row `row`, for the repository of the
/// workspace it belongs to, whose worktrees the host is asked to list. A row
/// outside git says so and opens nothing.
pub(crate) fn open(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    let (environment, root) = match row.own() {
        Item::Workspace(environment, root) | Item::Across(environment, root, _) => {
            (environment.clone(), root.clone())
        }
        Item::Worktree(environment, path)
        | Item::Heading(environment, path, _)
        | Item::File {
            environment,
            worktree: path,
            ..
        } => (environment.clone(), path.clone()),
        _ => return Vec::new(),
    };
    let tree = tree::listed(state.hosts.work(&environment));
    let Some(ws) = tree.workspace_of(&root) else {
        return Vec::new();
    };
    if !ws.git {
        return status_line::notice(
            &mut state.chrome.status,
            format!(
                "{} is not in a Git repository, so it has no worktrees",
                ws.name
            ),
        );
    }
    let repository = ws
        .worktrees
        .iter()
        .find(|w| w.worktree.is_main())
        .or(ws.worktrees.first())
        .map_or_else(|| ws.root.clone(), |w| w.worktree.path.clone());
    let project = ws.name.clone();
    state.overlays.cleanup = Some(Cleanup::new(
        environment.clone(),
        repository.clone(),
        project,
    ));
    vec![Effect::CleanupScan {
        environment,
        repository,
    }]
}

/// The worktrees the host listed for the list open, or why it could not.
pub(crate) fn read(
    state: &mut ViewState,
    environment: &EnvironmentId,
    repository: &Path,
    result: Result<Vec<CleanupEntry>, String>,
) -> Vec<Effect> {
    let Some(cleanup) = state
        .overlays
        .cleanup
        .as_mut()
        .filter(|c| c.environment == *environment && c.repository == repository)
    else {
        return Vec::new();
    };
    cleanup.cursor = 0;
    cleanup.rows = match result {
        Ok(entries) => Rows::Read(rows_of(entries)),
        Err(why) => Rows::Failed(why),
    };
    Vec::new()
}

/// What runs in the worktree at `path`: its sessions and the ids of its
/// terminals.
fn running(
    state: &ViewState,
    environment: &EnvironmentId,
    path: &Path,
) -> (Vec<Session>, Vec<u128>) {
    let tree = tree::listed(state.hosts.work(environment));
    tree.worktrees()
        .find(|w| w.worktree.path == path)
        .map(|w| {
            (
                w.sessions.clone(),
                w.terminals.iter().map(|t| t.id).collect(),
            )
        })
        .unwrap_or_default()
}

/// What a worktree chosen to go is deleted with.
fn doomed(state: &ViewState, cleanup: &Cleanup) -> Vec<Doomed> {
    cleanup
        .rows()
        .iter()
        .filter(|r| r.selected)
        .map(|r| {
            let (sessions, terminals) = running(state, &cleanup.environment, &r.entry.path);
            Doomed {
                name: r.entry.standing.branch.clone().unwrap_or_else(|| {
                    r.entry.path.file_name().map_or_else(
                        || r.entry.path.display().to_string(),
                        |f| f.to_string_lossy().into_owned(),
                    )
                }),
                path: r.entry.path.clone(),
                standing: r.entry.standing.clone(),
                sessions,
                terminals,
            }
        })
        .collect()
}

/// A worktree to delete, with what ends with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Doomed {
    pub name: String,
    pub path: PathBuf,
    pub standing: tsumugi_model::WorktreeStanding,
    pub sessions: Vec<Session>,
    pub terminals: Vec<u128>,
}

impl Doomed {
    /// Whether deleting it loses what only it holds: changes, commits no
    /// remote has, a detached head, or work running in it that ends.
    pub fn risky(&self) -> bool {
        self.standing.changed > 0
            || self.standing.unpushed > 0
            || self.standing.branch.is_none()
            || !self.sessions.is_empty()
            || !self.terminals.is_empty()
    }
}

/// What deleting `doomed` asks: what runs in each ends, then the host
/// deletes the worktree, with its changes only where the question said it
/// has some, and its branch unless git finds commits in it nothing else has.
pub(crate) fn deletions(environment: &EnvironmentId, doomed: Vec<Doomed>) -> Vec<Effect> {
    doomed
        .into_iter()
        .flat_map(|d| {
            let stops = d.sessions.into_iter().map({
                let environment = environment.clone();
                move |session| Effect::Host {
                    environment: environment.clone(),
                    request: Request::Stop {
                        session: Box::new(session),
                    },
                }
            });
            let ends = d.terminals.into_iter().map({
                let environment = environment.clone();
                move |id| Effect::EndShell {
                    environment: environment.clone(),
                    id,
                }
            });
            let delete = Effect::WorktreeDelete {
                environment: environment.clone(),
                path: d.path,
                force: d.standing.dirty(),
                branch: if d.standing.branch.is_some() {
                    BranchFate::Delete
                } else {
                    BranchFate::Keep
                },
            };
            stops.chain(ends).chain([delete])
        })
        .collect()
}

/// A key while the list holds the keys: the keys that move through a list
/// move the cursor, `Space` chooses the worktree under it, `a` chooses every
/// worktree with nothing to lose, or none when all of them are chosen, the
/// key that chooses deletes the worktrees chosen and the key that closes puts
/// the list away.
pub(crate) fn key(state: &mut ViewState, key: &Key) -> Vec<Effect> {
    let action = action_over(&state.chrome.keymap, Holder::Cleanup, key);
    let plain = !key.ctrl && !key.alt && !key.meta;
    let Some(cleanup) = state.overlays.cleanup.as_mut() else {
        return Vec::new();
    };
    match action {
        Some(Action::OverlayClose) => {
            state.overlays.cleanup = None;
            Vec::new()
        }
        Some(Action::OverlayDown) => {
            let len = cleanup.rows().len();
            cleanup.cursor = cursor::wrap(cleanup.cursor, 1, len);
            Vec::new()
        }
        Some(Action::OverlayUp) => {
            let len = cleanup.rows().len();
            cleanup.cursor = cursor::wrap(cleanup.cursor, -1, len);
            Vec::new()
        }
        Some(Action::OverlayChoose) => delete_chosen(state),
        _ if key.key == " " && plain => {
            let at = cleanup.cursor;
            toggle(state, at)
        }
        _ if key.key == "a" && plain => {
            let all_chosen = cleanup
                .rows()
                .iter()
                .filter(|r| r.safe())
                .all(|r| r.selected);
            for row in cleanup.rows_mut().iter_mut().filter(|r| r.safe()) {
                row.selected = !all_chosen;
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The worktree in row `at` is chosen, or unchosen when it was, and the cursor
/// goes there. A locked one cannot be chosen, and it is said.
pub(crate) fn toggle(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let Some(cleanup) = state.overlays.cleanup.as_mut() else {
        return Vec::new();
    };
    if at >= cleanup.rows().len() {
        return Vec::new();
    }
    cleanup.cursor = at;
    let row = &mut cleanup.rows_mut()[at];
    if !row.choosable() {
        return status_line::notice(
            &mut state.chrome.status,
            "That worktree is locked: unlock it with git worktree unlock first".into(),
        );
    }
    row.selected = !row.selected;
    Vec::new()
}

/// Delete what is chosen: at once when none of it loses anything, else after
/// a question that names what each loses.
fn delete_chosen(state: &mut ViewState) -> Vec<Effect> {
    let Some(cleanup) = state.overlays.cleanup.as_ref() else {
        return Vec::new();
    };
    let doomed = doomed(state, cleanup);
    if doomed.is_empty() {
        return Vec::new();
    }
    let environment = cleanup.environment.clone();
    state.overlays.cleanup = None;
    if doomed.iter().any(Doomed::risky) {
        state.overlays.confirm = Some(Ask::CleanUpWorktrees {
            environment,
            doomed,
        });
        return Vec::new();
    }
    deletions(&environment, doomed)
}

#[cfg(test)]
mod tests;
