//! The branches of the repositories the picker offers while a window shows
//! it, each read once while the picker stays open.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use tsumugi_protocol::{PickChange, Sight};

use super::Watched;

pub(super) async fn watch(mut watched: Watched) {
    let mut read: BTreeSet<PathBuf> = BTreeSet::new();
    loop {
        let now = asked(watched.seen().iter().map(|s| &s.sight));
        // The picker opened anew reads them again, as a branch may have been
        // made or checked out since.
        read.retain(|worktree| now.contains(worktree));
        for worktree in now.difference(&read).cloned().collect::<Vec<_>>() {
            read.insert(worktree.clone());
            match tsumugi_vcs::worktree::starts(&worktree).await {
                Ok(starts) => {
                    // The checkout's own commit is no branch to go to.
                    let branches = starts
                        .into_iter()
                        .filter(|c| c.point != tsumugi_model::StartPoint::Head)
                        .collect();
                    watched.record([PickChange::Branches { worktree, branches }]);
                }
                // A directory outside git has no branches.
                Err(error) => tracing::debug!(%error, "git listed no branches"),
            }
        }
        watched
            .wait(Duration::from_secs(300), |seen| {
                asked(seen.iter().map(|s| &s.sight))
            })
            .await;
    }
}

/// The worktrees whose repository's branches any window's picker offers.
fn asked<'a>(sights: impl Iterator<Item = &'a Sight>) -> BTreeSet<PathBuf> {
    sights
        .filter_map(|s| s.pick.as_ref())
        .flat_map(|pick| pick.branches.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use tsumugi_protocol::PickSight;

    use super::*;

    #[test]
    fn the_branches_every_picker_offers_are_asked_once() {
        let offering = |worktrees: &[&str]| Sight {
            pick: Some(PickSight {
                branches: worktrees.iter().map(PathBuf::from).collect(),
                ..PickSight::default()
            }),
            ..Sight::default()
        };
        let sights = [offering(&["/a"]), offering(&["/a", "/b"]), Sight::default()];
        assert_eq!(
            asked(sights.iter()),
            BTreeSet::from([PathBuf::from("/a"), PathBuf::from("/b")])
        );
        assert!(asked([Sight::default()].iter()).is_empty());
    }
}
