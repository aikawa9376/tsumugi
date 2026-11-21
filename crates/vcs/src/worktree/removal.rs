//! Deleting a worktree: what it would lose, and taking it away without
//! losing what the person did not agree to lose.

use std::path::{Path, PathBuf};

use tsumugi_model::{BranchFate, KeptBranch, WorktreeRemoved, WorktreeStanding};

use super::{Result, canonical, command, output};

/// One worktree of `git worktree list --porcelain`.
struct Listed {
    path: PathBuf,
    branch: Option<String>,
    locked: bool,
}

/// Every worktree of the repository `at` belongs to, the main checkout first.
async fn listed(at: &Path) -> Result<Vec<Listed>> {
    let out = command(
        at,
        "git",
        &["worktree".as_ref(), "list".as_ref(), "--porcelain".as_ref()],
    )
    .await?;
    Ok(out
        .split("\n\n")
        .filter_map(|entry| {
            let mut found = Listed {
                path: PathBuf::new(),
                branch: None,
                locked: false,
            };
            for line in entry.lines() {
                if let Some(path) = line.strip_prefix("worktree ") {
                    found.path = path.into();
                } else if let Some(branch) = line.strip_prefix("branch ") {
                    found.branch =
                        Some(branch.strip_prefix("refs/heads/").unwrap_or(branch).into());
                } else if line == "locked" || line.starts_with("locked ") {
                    found.locked = true;
                }
            }
            (!found.path.as_os_str().is_empty()).then_some(found)
        })
        .collect())
}

/// The worktree of `path` among the repository's, and the main checkout.
async fn find(path: &Path) -> Result<(Listed, PathBuf)> {
    let path = canonical(path)?;
    let all = listed(&path).await?;
    let main = all
        .first()
        .map(|w| canonical(&w.path).unwrap_or_else(|_| w.path.clone()))
        .ok_or("git lists no worktree here")?;
    // `path` may be a directory inside a worktree; only a worktree's own
    // directory stands for it.
    let found = all
        .into_iter()
        .find(|w| canonical(&w.path).is_ok_and(|p| p == path))
        .ok_or_else(|| format!("{} is not a worktree", path.display()))?;
    Ok((found, main))
}

async fn count(dir: &Path, args: &[&str]) -> usize {
    let args: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_ref()).collect();
    command(dir, "git", &args)
        .await
        .ok()
        .and_then(|out| out.trim().parse().ok())
        .unwrap_or(0)
}

/// How the worktree at `path` stands, as far as deleting it can lose work.
pub async fn standing(path: &Path) -> Result<WorktreeStanding> {
    let (found, main) = find(path).await?;
    let is_main = canonical(&found.path).is_ok_and(|p| p == main);
    let changed = crate::git::changes(&found.path)
        .await
        .map_err(|error| error.to_string())?
        .len();
    let mut standing = WorktreeStanding {
        branch: found.branch.clone(),
        changed,
        locked: found.locked,
        main: is_main,
        ..WorktreeStanding::default()
    };
    let Some(branch) = &found.branch else {
        return Ok(standing);
    };
    let upstream = format!("{branch}@{{upstream}}");
    let tracks = command(
        &found.path,
        "git",
        &[
            "rev-parse".as_ref(),
            "--verify".as_ref(),
            "--quiet".as_ref(),
            upstream.as_ref(),
        ],
    )
    .await
    .is_ok();
    let remotes = !command(&found.path, "git", &["remote".as_ref()])
        .await
        .unwrap_or_default()
        .trim()
        .is_empty();
    let range = format!("{upstream}..{branch}");
    standing.unpushed = if tracks {
        count(&found.path, &["rev-list", "--count", &range]).await
    } else if remotes {
        count(
            &found.path,
            &["rev-list", "--count", branch, "--not", "--remotes"],
        )
        .await
    } else {
        0
    };
    // What `git branch -d` weighs: the branch's upstream when it has one,
    // else the commit the main checkout is at.
    standing.unmerged = if is_main {
        0
    } else if tracks {
        count(&found.path, &["rev-list", "--count", &range]).await
    } else {
        count(&main, &["rev-list", "--count", branch, "--not", "HEAD"]).await
    };
    Ok(standing)
}

/// Deletes the worktree at `path`: its directory, and its branch as `fate`
/// says. A worktree with changes is deleted only when `force` is set, which
/// the caller sets once the person has been told what it loses; the main
/// checkout and a locked worktree are never deleted. A branch git will not
/// delete is left, and said.
pub async fn delete(path: &Path, force: bool, fate: BranchFate) -> Result<WorktreeRemoved> {
    let (found, main) = find(path).await?;
    if canonical(&found.path).is_ok_and(|p| p == main) {
        return Err("the main checkout is not a worktree to delete".into());
    }
    if found.locked {
        return Err(format!(
            "{} is locked; unlock it with git worktree unlock first",
            found.path.display()
        ));
    }
    let mut args: Vec<&std::ffi::OsStr> = vec!["worktree".as_ref(), "remove".as_ref()];
    if force {
        args.push("--force".as_ref());
    }
    args.push(found.path.as_os_str());
    command(&main, "git", &args).await?;
    let Some(branch) = found.branch.filter(|_| fate != BranchFate::Keep) else {
        return Ok(WorktreeRemoved::default());
    };
    let flag = if fate == BranchFate::DeleteUnmerged {
        "-D"
    } else {
        "-d"
    };
    let tried = output(
        &main,
        "git",
        &["branch".as_ref(), flag.as_ref(), branch.as_ref()],
    )
    .await?;
    Ok(WorktreeRemoved {
        kept: (!tried.status.success()).then(|| KeptBranch {
            name: branch,
            why: String::from_utf8_lossy(&tried.stderr).trim().to_string(),
        }),
    })
}
