//! How each worktree the tree lists stands against its branch's base, read
//! again every few seconds so its row follows the disk and the base's last
//! fetch.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use tsumugi_protocol::TreeChange;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    loop {
        let mut standings = BTreeMap::new();
        for worktree in listed(&watched) {
            if let Some(standing) = tsumugi_vcs::branch::standing(&worktree).await {
                standings.insert(worktree, standing);
            }
        }
        watched.record([TreeChange::Standings { standings }]);
        watched.wait(Duration::from_secs(10), |_| ()).await;
    }
}

/// Every worktree of a repository the tree lists.
fn listed(watched: &Watched) -> Vec<PathBuf> {
    let host = watched.host();
    let Some(listed) = host.work().tree().listed.as_ref() else {
        return Vec::new();
    };
    let mut worktrees: Vec<PathBuf> = listed
        .tree
        .worktrees()
        .map(|w| w.worktree.path.clone())
        .collect();
    worktrees.sort();
    worktrees.dedup();
    worktrees
}
