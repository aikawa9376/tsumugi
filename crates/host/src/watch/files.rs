//! The files of the worktrees the picker searches while a window shows it,
//! each read once while the picker stays open.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use tsumugi_protocol::{PickChange, Sight};
use tsumugi_vcs::git;

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    let mut read: BTreeSet<PathBuf> = BTreeSet::new();
    loop {
        let now = asked(watched.seen().iter().map(|s| &s.sight));
        // The picker opened anew reads what changed since, so the files of a
        // worktree are not read again while it stays open.
        read.retain(|worktree| now.contains(worktree));
        for worktree in now.difference(&read).cloned().collect::<Vec<_>>() {
            read.insert(worktree.clone());
            match git::files(&worktree).await {
                Ok(files) => watched.record([PickChange::Files { worktree, files }]),
                // A directory outside git has no files to search.
                Err(error) => tracing::debug!(%error, "git listed no files"),
            }
        }
        watched
            .wait(Duration::from_secs(300), |seen| {
                asked(seen.iter().map(|s| &s.sight))
            })
            .await;
    }
}

/// The worktrees whose files any window's picker searches.
fn asked<'a>(sights: impl Iterator<Item = &'a Sight>) -> BTreeSet<PathBuf> {
    sights
        .filter_map(|s| s.pick.as_ref())
        .flat_map(|pick| pick.files.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use tsumugi_protocol::PickSight;

    use super::*;

    #[test]
    fn the_files_every_picker_searches_are_asked_once() {
        let searching = |worktrees: &[&str]| Sight {
            pick: Some(PickSight {
                files: worktrees.iter().map(PathBuf::from).collect(),
                ..PickSight::default()
            }),
            ..Sight::default()
        };
        let sights = [
            searching(&["/a"]),
            searching(&["/a", "/b"]),
            Sight::default(),
        ];
        assert_eq!(
            asked(sights.iter()),
            BTreeSet::from([PathBuf::from("/a"), PathBuf::from("/b")])
        );
        assert!(asked([Sight::default()].iter()).is_empty());
    }
}
