//! The workspaces and worktrees the user silenced: what runs there is not
//! told of outside the window, and its rows stand as they always do.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tsumugi_model::{EnvironmentId, Tree};

use crate::effect::Effect;
use crate::state::ViewState;

/// The directories, by the environment holding them, whose notifications
/// are off: a workspace's root silences all it holds, a worktree's path
/// that worktree alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mutes(BTreeSet<(EnvironmentId, PathBuf)>);

/// How [`Mutes`] are written: environments by their ids.
#[derive(Serialize, Deserialize)]
struct Written(BTreeMap<String, BTreeSet<PathBuf>>);

impl Mutes {
    pub fn is_muted(&self, environment: &EnvironmentId, path: &Path) -> bool {
        self.0.contains(&(environment.clone(), path.to_path_buf()))
    }

    /// Whether the notifications of `session` are off, by its worktree or
    /// its workspace.
    pub fn silences(&self, environment: &EnvironmentId, tree: &Tree, session: &str) -> bool {
        tree.find(session).is_some_and(|(workspace, worktree, _)| {
            self.is_muted(environment, &workspace.root)
                || self.is_muted(environment, &worktree.worktree.path)
        })
    }

    /// Turn the notifications of `path` off, or on again; whether they are
    /// off now.
    pub(crate) fn toggle(&mut self, environment: &EnvironmentId, path: &Path) -> bool {
        let key = (environment.clone(), path.to_path_buf());
        if self.0.remove(&key) {
            false
        } else {
            self.0.insert(key);
            true
        }
    }

    pub fn to_json(&self) -> String {
        let mut written: BTreeMap<String, BTreeSet<PathBuf>> = BTreeMap::new();
        for (environment, path) in &self.0 {
            written
                .entry(environment.to_string())
                .or_default()
                .insert(path.clone());
        }
        serde_json::to_string(&Written(written)).expect("mutes are plain data")
    }

    /// From what [`Mutes::to_json`] wrote.
    pub fn from_json(text: &[u8]) -> Result<Self, serde_json::Error> {
        let written: Written = serde_json::from_slice(text)?;
        Ok(Self(
            written
                .0
                .into_iter()
                .flat_map(|(e, paths)| {
                    paths
                        .into_iter()
                        .map(move |path| (EnvironmentId::new(&e), path))
                })
                .collect(),
        ))
    }
}

/// The notifications of `path` in `environment` turned off, or on again.
pub(crate) fn toggle(
    state: &mut ViewState,
    environment: &EnvironmentId,
    path: &Path,
) -> Vec<Effect> {
    state.notices.mutes.toggle(environment, path);
    vec![Effect::KeepMutes(state.notices.mutes.clone())]
}

/// What the window muted before it closed comes back, ahead of what it
/// mutes since.
pub(crate) fn restore(state: &mut ViewState, mutes: Mutes) -> Vec<Effect> {
    state.notices.mutes.0.extend(mutes.0);
    Vec::new()
}
