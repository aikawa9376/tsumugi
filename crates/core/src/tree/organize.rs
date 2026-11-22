//! How the user arranges the tree: the worktrees and workspaces pinned to
//! the top, the order the rest keep, and the worktrees left out of it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tsumugi_model::{EnvironmentId, Session, State, Tree, WorktreeNode};

use super::nav::Item;
use crate::chrome::status_line;
use crate::content::Tab;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree;

/// The order worktrees keep within their workspace, below the pinned ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    /// The order the work host lists them in.
    #[default]
    Default,
    /// The one most recently written to or started first, those with no
    /// session last.
    Activity,
    /// By branch, ignoring case.
    Name,
}

impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::Activity,
            Self::Activity => Self::Name,
            Self::Name => Self::Default,
        }
    }

    /// What it is called to the person.
    pub fn word(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Activity => "activity",
            Self::Name => "name",
        }
    }
}

/// Which worktrees are left out of the tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hide {
    /// Worktrees whose sessions all sleep.
    pub sleeping: bool,
    /// Worktrees where no session works, waits or starts and no terminal runs.
    pub idle: bool,
    /// The main checkout of a workspace that lists other worktrees.
    pub main: bool,
}

/// What the user arranged of the tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Organize {
    pinned: BTreeSet<(EnvironmentId, PathBuf)>,
    pub sort: Sort,
    pub hide: Hide,
}

/// How [`Organize`] is written: environments by their ids.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Written {
    pinned: BTreeMap<String, BTreeSet<PathBuf>>,
    sort: Sort,
    hide: Hide,
}

/// When `session` was last written to, or else started, in milliseconds.
fn touched(session: &Session) -> u64 {
    session.last_input_ms.max(session.started_ms).unwrap_or(0)
}

fn activity(node: &WorktreeNode) -> u64 {
    node.sessions.iter().map(touched).max().unwrap_or(0)
}

/// What a worktree is called to sort it by.
fn name(node: &WorktreeNode) -> String {
    node.worktree
        .branch
        .clone()
        .or_else(|| {
            node.worktree
                .path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
        .to_lowercase()
}

/// Whether a session works, waits on the user or is starting.
fn active(session: &Session) -> bool {
    session.rest.is_none()
        && matches!(
            session.state,
            State::Working | State::Waiting(_) | State::Starting | State::Background
        )
}

impl Organize {
    pub fn is_pinned(&self, environment: &EnvironmentId, path: &Path) -> bool {
        self.pinned
            .contains(&(environment.clone(), path.to_path_buf()))
    }

    /// How many are pinned.
    pub fn pinned_count(&self) -> usize {
        self.pinned.len()
    }

    /// Pin the worktree or workspace at `path`, or unpin it; whether it is
    /// pinned now.
    pub fn toggle_pin(&mut self, environment: &EnvironmentId, path: &Path) -> bool {
        let key = (environment.clone(), path.to_path_buf());
        if self.pinned.remove(&key) {
            false
        } else {
            self.pinned.insert(key);
            true
        }
    }

    /// Put `tree`, listed by `environment`'s host, in the order arranged:
    /// pinned workspaces first, scratch last, and in each workspace the
    /// pinned worktrees first, then the rest as the sort says. Sorting keeps
    /// the order among those that rank alike.
    pub fn order(&self, environment: &EnvironmentId, tree: &mut Tree) {
        for ws in &mut tree.workspaces {
            match self.sort {
                Sort::Default => {}
                Sort::Activity => ws.worktrees.sort_by_key(|w| std::cmp::Reverse(activity(w))),
                Sort::Name => ws.worktrees.sort_by_key(name),
            }
            ws.worktrees
                .sort_by_key(|w| !self.is_pinned(environment, &w.worktree.path));
        }
        tree.workspaces
            .sort_by_key(|ws| (ws.scratch, !self.is_pinned(environment, &ws.root)));
    }

    /// The worktrees of `tree` the hide filters leave out. A pinned worktree
    /// is never left out, nor is anything of a workspace that shows no
    /// worktree rows of its own.
    pub fn hidden(&self, environment: &EnvironmentId, tree: &Tree) -> BTreeSet<PathBuf> {
        let Hide {
            sleeping,
            idle,
            main,
        } = self.hide;
        if !(sleeping || idle || main) {
            return BTreeSet::new();
        }
        let mut out = BTreeSet::new();
        for ws in tree.workspaces.iter().filter(|ws| !ws.flat()) {
            for w in &ws.worktrees {
                let hide = (sleeping && w.sleeping())
                    || (idle && w.terminals.is_empty() && !w.sessions.iter().any(active))
                    || (main && w.worktree.is_main() && ws.worktrees.len() > 1);
                if hide && !self.is_pinned(environment, &w.worktree.path) {
                    out.insert(w.worktree.path.clone());
                }
            }
        }
        out
    }

    pub fn to_json(&self) -> String {
        let mut pinned: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
        for (environment, path) in &self.pinned {
            pinned
                .entry(environment.to_string())
                .or_default()
                .insert(path.clone());
        }
        serde_json::to_string(&Written {
            pinned,
            sort: self.sort,
            hide: self.hide,
        })
        .expect("the arrangement is plain data")
    }

    /// From what [`Organize::to_json`] wrote.
    pub fn from_json(text: &[u8]) -> Result<Self, serde_json::Error> {
        let written: Written = serde_json::from_slice(text)?;
        Ok(Self {
            pinned: written
                .pinned
                .into_iter()
                .flat_map(|(e, paths)| {
                    paths
                        .into_iter()
                        .map(move |path| (EnvironmentId::new(&e), path))
                })
                .collect(),
            sort: written.sort,
            hide: written.hide,
        })
    }
}

/// What the arrangement now says, kept for the next run.
fn kept(state: &ViewState) -> Vec<Effect> {
    vec![Effect::KeepOrganize(state.tree.organize.clone())]
}

/// Pin the workspace or worktree of the tree's row `row`, or unpin it. Any
/// other row cannot be pinned.
pub(crate) fn pin(state: &mut ViewState, row: &Item) -> Vec<Effect> {
    let (environment, path) = match row.own() {
        Item::Workspace(environment, root) => (environment, root),
        Item::Worktree(environment, path) => (environment, path),
        _ => return Vec::new(),
    };
    let pinned = state.tree.organize.toggle_pin(environment, path);
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |f| f.to_string_lossy().into_owned(),
    );
    let mut effects = kept(state);
    effects.extend(status_line::notice(
        &mut state.chrome.status,
        if pinned {
            format!("Pinned {name}")
        } else {
            format!("Unpinned {name}")
        },
    ));
    effects
}

/// Step to the next order the worktrees are kept in, and say which it is.
pub(crate) fn sort(state: &mut ViewState) -> Vec<Effect> {
    let organize = &mut state.tree.organize;
    organize.sort = organize.sort.next();
    let word = organize.sort.word();
    let mut effects = kept(state);
    effects.extend(status_line::notice(
        &mut state.chrome.status,
        format!("Worktrees are ordered by {word}"),
    ));
    effects
}

/// Which worktrees a filter leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    Sleeping,
    Idle,
    Main,
}

/// Turn the filter on, or off again.
pub(crate) fn hide(state: &mut ViewState, filter: Filter) -> Vec<Effect> {
    let hide = &mut state.tree.organize.hide;
    let (flag, what) = match filter {
        Filter::Sleeping => (&mut hide.sleeping, "sleeping worktrees"),
        Filter::Idle => (&mut hide.idle, "idle worktrees"),
        Filter::Main => (&mut hide.main, "main checkouts"),
    };
    *flag = !*flag;
    let told = if *flag {
        format!("Hiding {what}")
    } else {
        format!("Showing {what}")
    };
    let mut effects = kept(state);
    effects.extend(status_line::notice(&mut state.chrome.status, told));
    effects
}

/// What the window arranged before it closed comes back, ahead of what it
/// arranges since: what was pinned since stays, and so does an order or a
/// filter set since.
pub(crate) fn restore(view: &mut super::TreeView, kept: Organize) {
    let now = std::mem::take(&mut view.organize);
    let mut merged = kept;
    merged.pinned.extend(now.pinned);
    if now.sort != Sort::Default {
        merged.sort = now.sort;
    }
    merged.hide = Hide {
        sleeping: merged.hide.sleeping || now.hide.sleeping,
        idle: merged.hide.idle || now.hide.idle,
        main: merged.hide.main || now.hide.main,
    };
    view.organize = merged;
}

/// Select the row of what the focused window shows, unfolding the way to it
/// and showing the worktree again where a filter left it out. Anything
/// else, and nothing, has no row to go to, and it is said.
pub(crate) fn reveal_current(state: &mut ViewState) -> Vec<Effect> {
    let nothing = |state: &mut ViewState| {
        status_line::notice(
            &mut state.chrome.status,
            "Nothing shown in the tree to go to".into(),
        )
    };
    let Some(tab) = state.content.active().cloned() else {
        return nothing(state);
    };
    let environment = tab.environment().clone();
    let tree = listed(state, &environment);
    let (shown, worktree) = match &tab {
        Tab::Session(_, id) => (
            tree::reveal_session(&mut state.tree, &environment, &tree, id),
            tree.find(id).map(|(_, w, _)| w.worktree.path.clone()),
        ),
        Tab::Terminal(_, id) => (
            tree::reveal_terminal(&mut state.tree, &environment, &tree, *id),
            tree.terminal(*id).map(|(_, w, _)| w.worktree.path.clone()),
        ),
        _ => (false, None),
    };
    if !shown {
        return nothing(state);
    }
    let mut effects = Vec::new();
    if let Some(path) = worktree
        && state
            .tree
            .organize
            .hidden(&environment, &tree)
            .contains(&path)
    {
        state.tree.organize.hide = Hide::default();
        effects.extend(kept(state));
    }
    effects.extend(crate::sidebar::switch(state, crate::sidebar::Panel::Tree));
    effects
}

/// The tree `environment`'s host lists.
fn listed(state: &ViewState, environment: &EnvironmentId) -> Tree {
    super::listed(state.hosts.work(environment)).clone()
}

#[cfg(test)]
mod acting;
#[cfg(test)]
mod tests;
