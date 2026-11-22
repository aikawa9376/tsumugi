//! Plain questions about the tree: where a session is, whether it is still
//! listed, the tree narrowed to a name, where a row works.

use std::collections::HashSet;
use std::path::Path;

use tsumugi_model::{Session, Tree};

use super::Item;
use crate::content::Tab;
use crate::picker::fuzzy::Query;
use crate::tree::Place;

/// Find a session by id, with the worktree it runs in.
pub fn find<'a>(tree: &'a Tree, id: &str) -> Option<(&'a Path, &'a Session)> {
    tree.find(id)
        .map(|(_, wt, s)| (wt.worktree.path.as_path(), s))
}

/// Whether the session `id` names is no longer listed.
pub fn is_gone(sessions: &[Session], id: &str) -> bool {
    !sessions.iter().any(|s| s.id == id)
}

/// Tabs kept for what they show, those of sessions no longer listed left out.
pub fn followed(sessions: &[Session], tabs: &HashSet<Tab>) -> HashSet<Tab> {
    tabs.iter()
        .filter(|tab| match tab {
            Tab::Session(_, id) => !is_gone(sessions, id),
            _ => true,
        })
        .cloned()
        .collect()
}

/// `tree` with only the sessions and terminals whose names match `text` as
/// the pickers match, and the worktrees and workspaces that still hold one;
/// a workspace or a worktree whose own name matches keeps everything beneath
/// it. A `text` that asks nothing keeps everything.
pub fn narrowed(tree: &Tree, text: &str) -> Tree {
    narrowed_by(tree, text, &[])
}

/// [`narrowed`], with the words `status:<status>` in `text` keeping only
/// the worktrees whose checkpoint is in one of the statuses named, with all
/// they hold; the rest of `text` narrows as it does alone.
pub fn narrowed_by(tree: &Tree, text: &str, checkpoints: &[tsumugi_model::Checkpoint]) -> Tree {
    let (statuses, rest) = split_statuses(text);
    if statuses.is_empty() {
        return narrowed_to_names(tree, text);
    }
    let mut kept = tree.clone();
    for ws in &mut kept.workspaces {
        ws.worktrees.retain(|w| {
            checkpoints.iter().any(|c| {
                c.worktree == w.worktree.path && c.status.is_some_and(|s| statuses.contains(&s))
            })
        });
    }
    kept.workspaces.retain(|ws| !ws.worktrees.is_empty());
    narrowed_to_names(&kept, &rest)
}

/// `text` with the word `status:<status>` added, or taken away when it is
/// there, as clicking a status chip narrows the tree to a status or widens it.
pub fn status_toggled(text: &str, status: tsumugi_model::WorktreeStatus) -> String {
    let named = |word: &str| {
        word.strip_prefix("status:")
            .and_then(tsumugi_model::WorktreeStatus::parse)
            == Some(status)
    };
    if text.split_whitespace().any(named) {
        return text
            .split_whitespace()
            .filter(|word| !named(word))
            .collect::<Vec<_>>()
            .join(" ");
    }
    let word = format!("status:{}", status.word());
    if text.trim().is_empty() {
        word
    } else {
        format!("{} {word}", text.trim())
    }
}

/// The statuses `text` names by `status:` words, and the words left.
fn split_statuses(text: &str) -> (Vec<tsumugi_model::WorktreeStatus>, String) {
    let mut statuses = Vec::new();
    let mut rest = Vec::new();
    for word in text.split_whitespace() {
        match word
            .strip_prefix("status:")
            .and_then(tsumugi_model::WorktreeStatus::parse)
        {
            Some(status) => statuses.push(status),
            None => rest.push(word),
        }
    }
    (statuses, rest.join(" "))
}

fn narrowed_to_names(tree: &Tree, text: &str) -> Tree {
    let query = Query::new(text);
    if query.is_empty() {
        return tree.clone();
    }
    let named = |name: &str| query.matches(name);
    let mut out = tree.clone();
    for ws in &mut out.workspaces {
        if named(&ws.name) {
            continue;
        }
        ws.worktrees.retain_mut(|w| {
            let own = w.worktree.branch.as_deref().is_some_and(named)
                || w.worktree
                    .path
                    .file_name()
                    .is_some_and(|f| named(&f.to_string_lossy()));
            if !own {
                w.sessions.retain(|s| named(&s.name));
                w.terminals.retain(|s| named(&s.name));
            }
            own || !w.sessions.is_empty() || !w.terminals.is_empty()
        });
    }
    out.workspaces
        .retain(|ws| named(&ws.name) || !ws.worktrees.is_empty());
    out
}

/// The worktree or directory an item works in: a session's own, the one a
/// shell was started in, a worktree itself, a workspace's main checkout or
/// directory.
pub fn place_of(tree: &Tree, item: &Item) -> Option<Place> {
    let at = |worktree: &Path| Place::at(item.environment(), worktree);
    match item {
        Item::Waiting(_, id) | Item::Session(_, id) => find(tree, id).map(|(p, _)| at(p)),
        Item::Terminal(_, id) => tree.terminal(*id).map(|(_, _, s)| at(&s.cwd)),
        Item::LocalIssue(_, dir, _) | Item::Browser(_, dir, _) => Some(at(dir)),
        Item::Worktree(_, path) | Item::Heading(_, path, _) | Item::File { worktree: path, .. } => {
            Some(at(path))
        }
        Item::Gathered(own) => place_of(tree, own),
        Item::Environment(_) => None,
        // Not the root itself: a bare repository, a submodule or a separate
        // git directory is named by its git directory.
        Item::Workspace(_, root)
        | Item::Across(_, root, _)
        | Item::GitHub(_, root, _)
        | Item::GitHubItem(_, root, ..)
        | Item::GitHubFailed(_, root, _)
        | Item::Pending(_, root, _) => {
            let ws = tree.workspaces.iter().find(|ws| &ws.root == root)?;
            let main = ws.worktrees.iter().find(|w| w.worktree.is_main());
            main.or(ws.worktrees.first()).map(|w| at(&w.worktree.path))
        }
    }
}

/// The sessions running in a worktree, for the tabs above the console.
pub fn sessions_in<'a>(tree: &'a Tree, place: &Place) -> Vec<&'a Session> {
    tree.worktrees()
        .filter(|w| w.worktree.path == place.worktree)
        .flat_map(|w| w.sessions.iter())
        .collect()
}
