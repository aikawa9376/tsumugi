//! Reading what a spot of the details panel shows.

use std::path::Path;

use tsumugi_protocol::RepositoryRead;
use tsumugi_vcs::{Limits, Repo, Spot};

pub use tsumugi_core::details::*;

/// Read what `spot` shows of `worktree`: through jj in a jj repository,
/// through git otherwise or when jj cannot be run.
pub async fn load(spot: Spot, worktree: &Path) -> RepositoryRead {
    let details = crate::settings::current().details;
    let limits = Limits {
        diff_lines: details.diff_lines,
        log_commits: details.log_commits,
    };
    let read = Repo::open(worktree).load(&spot, limits).await;
    RepositoryRead {
        spot,
        listing: read.listing.map_err(|e| e.to_string()),
        revision: read.revision,
    }
}
