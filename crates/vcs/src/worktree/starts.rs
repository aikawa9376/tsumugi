//! The places a new worktree may start: the checkout's own commit and the
//! branches the repository lists.

use std::path::Path;

use tsumugi_model::{StartChoice, StartPoint};

use super::{Result, command};

/// Where a worktree of the repository holding `source` may start, as the
/// repository lists them: its checkout's own commit first, then its local
/// branches and then the branches of its remotes, each group by the newest
/// commit.
pub async fn starts(source: &Path) -> Result<Vec<StartChoice>> {
    let head = command(
        source,
        "git",
        &["log".as_ref(), "-1".as_ref(), "--format=%s".as_ref()],
    )
    .await
    .unwrap_or_default();
    let listed = command(
        source,
        "git",
        &[
            "for-each-ref".as_ref(),
            "--sort=-committerdate".as_ref(),
            "--format=%(refname)\t%(contents:subject)".as_ref(),
            "refs/heads".as_ref(),
            "refs/remotes".as_ref(),
        ],
    )
    .await?;
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for line in listed.lines() {
        let (name, subject) = line.split_once('\t').unwrap_or((line, ""));
        let choice = |point| StartChoice {
            point,
            subject: subject.to_string(),
        };
        if let Some(branch) = name.strip_prefix("refs/heads/") {
            local.push(choice(StartPoint::Local {
                branch: branch.into(),
            }));
        } else if let Some((remote_name, branch)) = name
            .strip_prefix("refs/remotes/")
            .and_then(|rest| rest.split_once('/'))
            // `origin/HEAD` only says which branch is the default.
            .filter(|(_, branch)| *branch != "HEAD")
        {
            remote.push(choice(StartPoint::Remote {
                remote: remote_name.into(),
                branch: branch.into(),
            }));
        }
    }
    Ok([StartChoice {
        point: StartPoint::Head,
        subject: head.trim().to_string(),
    }]
    .into_iter()
    .chain(local)
    .chain(remote)
    .collect())
}
