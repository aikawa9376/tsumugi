//! Where a worktree's branch stands: what HEAD is, what Git began in it and
//! has not finished, and how the branch compares with its base, the branch
//! it will be merged into.

use std::path::Path;

use tsumugi_model::repository::{Branch, Compared, Comparison, Head, Operation, Standing};

use crate::Error;
use crate::git::{UNQUOTED, git, parse_name_status};

/// The refs tried as the base, in order, when none was chosen and the remote
/// does not say which branch is its default.
const DEFAULT_BASES: [&str; 4] = ["origin/main", "origin/master", "main", "master"];

/// What HEAD is: the branch checked out, or else the commit.
pub async fn head(path: &Path) -> Result<Head, Error> {
    if let Ok(branch) = git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await {
        return Ok(Head::Branch(branch.trim().to_string()));
    }
    let short = git(path, &["rev-parse", "--short", "HEAD"]).await?;
    Ok(Head::Detached(short.trim().to_string()))
}

/// What Git began in the worktree and has not finished. The directories of a
/// rebase outlive the moments its `REBASE_HEAD` is there, so they tell it.
pub async fn operation(path: &Path) -> Option<Operation> {
    let dir = git(path, &["rev-parse", "--absolute-git-dir"]).await.ok()?;
    let dir = Path::new(dir.trim());
    let there = |name: &str| dir.join(name).exists();
    if there("MERGE_HEAD") {
        Some(Operation::Merge)
    } else if there("rebase-merge") || there("rebase-apply") {
        Some(Operation::Rebase)
    } else if there("CHERRY_PICK_HEAD") {
        Some(Operation::CherryPick)
    } else if there("REVERT_HEAD") {
        Some(Operation::Revert)
    } else {
        None
    }
}

/// The base the user chose for `branch`.
pub async fn chosen(path: &Path, branch: &str) -> Option<String> {
    let key = base_key(branch);
    git(path, &["config", "--get", &key])
        .await
        .ok()
        .map(|base| base.trim().to_string())
        .filter(|base| !base.is_empty())
}

/// The configuration key that keeps the base chosen for `branch`.
pub(crate) fn base_key(branch: &str) -> String {
    format!("branch.{branch}.tsumugiBase")
}

/// Whether `rev` names a commit of the repository.
pub(crate) async fn resolves(path: &Path, rev: &str) -> bool {
    if rev.starts_with('-') {
        return false;
    }
    let commit = format!("{rev}^{{commit}}");
    git(path, &["rev-parse", "--verify", "--quiet", &commit])
        .await
        .is_ok()
}

/// The base to compare with when none was chosen: the remote's default
/// branch, or the first of the usual names that exists.
async fn found_base(path: &Path) -> Option<String> {
    if let Ok(head) = git(
        path,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .await
    {
        let head = head.trim();
        if resolves(path, head).await {
            return Some(head.to_string());
        }
    }
    for name in DEFAULT_BASES {
        if resolves(path, name).await {
            return Some(name.to_string());
        }
    }
    None
}

fn empty(status: Compared, base: Option<String>) -> Comparison {
    Comparison {
        status,
        base,
        merge_base: None,
        ahead: 0,
        behind: 0,
        added: 0,
        removed: 0,
        files: Vec::new(),
    }
}

/// The lines added and taken out, summed over `git diff --numstat`; a binary
/// file, printed with dashes, counts none.
fn totals(numstat: &str) -> (usize, usize) {
    numstat.lines().fold((0, 0), |(added, removed), line| {
        let mut fields = line.split('\t');
        let number = |field: Option<&str>| field.and_then(|f| f.parse::<usize>().ok()).unwrap_or(0);
        (
            added + number(fields.next()),
            removed + number(fields.next()),
        )
    })
}

/// How the branch checked out compares with `base`, or with the base found
/// when none is given: what it committed since it parted from it, and how many
/// commits each has that the other lacks. Nothing in the worktree is read,
/// so what is only edited is not counted.
pub async fn compare(path: &Path, base: Option<&str>) -> Comparison {
    let base = match base {
        Some(base) => Some(base.to_string()),
        None => found_base(path).await,
    };
    let Some(base) = base else {
        return empty(
            Compared::NoBase(
                "no base branch was found: none is chosen, and neither origin's default branch nor main nor master exists".into(),
            ),
            None,
        );
    };
    let named = Some(base.clone());
    let base_resolves = resolves(path, &base).await;
    if git(path, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .await
        .is_err()
    {
        // A branch with no commit has nothing to compare, but its base,
        // which is there, is not to blame.
        return if base_resolves {
            empty(Compared::Ready, named)
        } else {
            empty(Compared::Unborn, named)
        };
    }
    if !base_resolves {
        return empty(
            Compared::NoBase(format!("{base} does not name a commit in this repository")),
            named,
        );
    }
    let merge_base = match git(path, &["merge-base", &base, "HEAD"]).await {
        Ok(found) => found.trim().to_string(),
        Err(_) => {
            let shallow = git(path, &["rev-parse", "--is-shallow-repository"])
                .await
                .is_ok_and(|out| out.trim() == "true");
            let why = if shallow {
                format!(
                    "this branch and {base} share no commit in this shallow clone; fetch more history to compare them"
                )
            } else {
                format!("this branch and {base} share no commit")
            };
            return empty(Compared::Unrelated(why), named);
        }
    };
    match read(path, &base, &merge_base).await {
        Ok(read) => Comparison {
            status: Compared::Ready,
            base: named,
            merge_base: Some(merge_base),
            ..read
        },
        Err(e) => empty(Compared::Failed(e.to_string()), named),
    }
}

async fn read(path: &Path, base: &str, merge_base: &str) -> Result<Comparison, Error> {
    let range = format!("{base}...HEAD");
    let counts = git(path, &["rev-list", "--left-right", "--count", &range]).await?;
    let mut counts = counts.split_whitespace().map(|n| n.parse().unwrap_or(0));
    let (behind, ahead) = (counts.next().unwrap_or(0), counts.next().unwrap_or(0));
    let files = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            merge_base,
            "HEAD",
            "--",
        ],
    )
    .await?;
    let numstat = git(
        path,
        &[
            "diff",
            "--numstat",
            "-M",
            "--no-ext-diff",
            merge_base,
            "HEAD",
            "--",
        ],
    )
    .await?;
    let (added, removed) = totals(&numstat);
    Ok(Comparison {
        status: Compared::Ready,
        base: None,
        merge_base: None,
        ahead,
        behind,
        added,
        removed,
        files: parse_name_status(&files),
    })
}

/// How the worktree stands against its branch's base, or nothing where the
/// branch cannot be compared.
pub async fn standing(path: &Path) -> Option<Standing> {
    let branch = branch(path).await.ok()?;
    let comparison = branch.comparison;
    let merge_base = comparison.merge_base?;
    let numstat = git(
        path,
        &[
            "diff",
            "--numstat",
            "-M",
            "--no-ext-diff",
            &merge_base,
            "--",
        ],
    )
    .await
    .ok()?;
    let (added, removed) = totals(&numstat);
    Some(Standing {
        merge_base,
        ahead: comparison.ahead,
        behind: comparison.behind,
        added,
        removed,
    })
}

/// Where the branch of the worktree stands.
pub async fn branch(path: &Path) -> Result<Branch, Error> {
    let head = head(path).await?;
    let chosen = match &head {
        Head::Branch(name) => chosen(path, name).await,
        Head::Detached(_) => None,
    };
    let comparison = compare(path, chosen.as_deref()).await;
    Ok(Branch {
        head,
        operation: operation(path).await,
        chosen,
        comparison,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::repository::Change;

    fn git_in(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q", "-b", "main"]);
        dir
    }

    fn commit(dir: &Path, name: &str, text: &str) {
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
        git_in(dir, &["add", "--all"]);
        git_in(dir, &["commit", "-q", "-m", name]);
    }

    /// `main` with a commit of its own after `feature` branched, and
    /// `feature` two commits on, checked out.
    fn diverged() -> tempfile::TempDir {
        let dir = repo();
        commit(dir.path(), "shared.txt", "one\n");
        git_in(dir.path(), &["switch", "-q", "-c", "feature"]);
        commit(dir.path(), "a.txt", "1\n2\n3\n");
        commit(dir.path(), "shared.txt", "one\ntwo\n");
        git_in(dir.path(), &["switch", "-q", "main"]);
        commit(dir.path(), "main-only.txt", "m\n");
        git_in(dir.path(), &["switch", "-q", "feature"]);
        dir
    }

    fn change(status: &str, path: &str) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: None,
        }
    }

    #[tokio::test]
    async fn the_branch_is_compared_from_where_it_parted_from_its_base() {
        let dir = diverged();
        let found = compare(dir.path(), None).await;
        assert_eq!(found.status, Compared::Ready);
        assert_eq!(found.base.as_deref(), Some("main"));
        assert_eq!((found.ahead, found.behind), (2, 1));
        assert_eq!((found.added, found.removed), (4, 0));
        let mut files = found.files;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(files, [change("A", "a.txt"), change("M", "shared.txt")]);
        let main = git(dir.path(), &["rev-parse", "main~1"]).await.unwrap();
        assert_eq!(found.merge_base.as_deref(), Some(main.trim()));
    }

    #[tokio::test]
    async fn what_is_only_edited_in_the_worktree_is_not_counted() {
        let dir = diverged();
        std::fs::write(dir.path().join("a.txt"), "changed\n").unwrap();
        std::fs::write(dir.path().join("loose.txt"), "x\n").unwrap();
        let found = compare(dir.path(), None).await;
        assert_eq!(found.files.len(), 2);
        assert_eq!(found.added, 4);
    }

    #[tokio::test]
    async fn a_worktree_stands_against_its_base_with_what_is_only_edited_counted() {
        let dir = diverged();
        std::fs::write(dir.path().join("a.txt"), "1\n2\n3\n4\n").unwrap();
        let stood = standing(dir.path()).await.unwrap();
        let parted = git(dir.path(), &["rev-parse", "main~1"]).await.unwrap();
        assert_eq!(
            stood,
            Standing {
                merge_base: parted.trim().to_string(),
                ahead: 2,
                behind: 1,
                added: 5,
                removed: 0
            }
        );
        let lone = repo();
        commit(lone.path(), "a.txt", "x\n");
        git_in(lone.path(), &["switch", "-q", "--orphan", "other"]);
        commit(lone.path(), "b.txt", "y\n");
        assert_eq!(standing(lone.path()).await, None, "no commit shared");
    }

    #[tokio::test]
    async fn a_chosen_base_is_compared_with_instead_of_the_one_found() {
        let dir = diverged();
        git_in(dir.path(), &["branch", "release", "main~1"]);
        let found = compare(dir.path(), Some("release")).await;
        assert_eq!(found.base.as_deref(), Some("release"));
        assert_eq!((found.ahead, found.behind), (2, 0));
    }

    #[tokio::test]
    async fn the_remotes_default_branch_is_the_base_found_before_the_usual_names() {
        let origin = tempfile::tempdir().unwrap();
        git_in(origin.path(), &["init", "-q", "-b", "trunk"]);
        commit(origin.path(), "a.txt", "1\n");
        let dir = tempfile::tempdir().unwrap();
        let url = origin.path().to_str().unwrap();
        git_in(dir.path(), &["clone", "-q", url, "."]);
        git_in(dir.path(), &["switch", "-q", "-c", "work"]);
        let found = compare(dir.path(), None).await;
        assert_eq!(found.base.as_deref(), Some("origin/trunk"));
        assert_eq!(found.status, Compared::Ready);
    }

    #[tokio::test]
    async fn a_base_that_does_not_exist_is_said_not_to() {
        let dir = diverged();
        let found = compare(dir.path(), Some("nowhere")).await;
        assert!(matches!(found.status, Compared::NoBase(ref why) if why.contains("nowhere")));
        assert_eq!(found.base.as_deref(), Some("nowhere"));
        assert!(found.files.is_empty());
        let flag = compare(dir.path(), Some("--output=x")).await;
        assert!(matches!(flag.status, Compared::NoBase(_)));
        assert!(!dir.path().join("x").exists());
    }

    #[tokio::test]
    async fn a_repository_with_no_base_to_find_says_so() {
        let dir = repo();
        commit(dir.path(), "a.txt", "1\n");
        git_in(dir.path(), &["switch", "-q", "-c", "topic"]);
        git_in(dir.path(), &["branch", "-q", "-m", "main", "other"]);
        let found = compare(dir.path(), None).await;
        assert!(matches!(found.status, Compared::NoBase(_)), "{found:?}");
        assert_eq!(found.base, None);
    }

    #[tokio::test]
    async fn a_branch_with_no_commit_has_nothing_to_compare() {
        let dir = repo();
        let unborn = compare(dir.path(), Some("main")).await;
        assert_eq!(unborn.status, Compared::Unborn);
        let also = compare(dir.path(), None).await;
        assert!(matches!(also.status, Compared::NoBase(_)));
    }

    #[tokio::test]
    async fn a_shallow_clone_that_cannot_reach_the_merge_base_says_it_is_shallow() {
        let origin = repo();
        commit(origin.path(), "a.txt", "1\n");
        git_in(origin.path(), &["switch", "-q", "-c", "feature"]);
        commit(origin.path(), "b.txt", "2\n");
        git_in(origin.path(), &["switch", "-q", "main"]);
        commit(origin.path(), "c.txt", "3\n");
        let dir = tempfile::tempdir().unwrap();
        let url = format!("file://{}", origin.path().display());
        git_in(
            dir.path(),
            &[
                "clone",
                "-q",
                "--depth",
                "1",
                "--no-single-branch",
                &url,
                ".",
            ],
        );
        git_in(dir.path(), &["switch", "-q", "feature"]);
        let found = compare(dir.path(), Some("origin/main")).await;
        let Compared::Unrelated(why) = found.status else {
            panic!("{:?}", found.status);
        };
        assert!(why.contains("shallow"), "{why}");
        assert!(found.files.is_empty());
    }

    #[tokio::test]
    async fn head_is_the_branch_or_else_the_commit() {
        let dir = diverged();
        assert_eq!(
            head(dir.path()).await.unwrap(),
            Head::Branch("feature".into())
        );
        git_in(dir.path(), &["checkout", "-q", "--detach"]);
        let Head::Detached(short) = head(dir.path()).await.unwrap() else {
            panic!("on a branch");
        };
        let expected = git(dir.path(), &["rev-parse", "--short", "HEAD"])
            .await
            .unwrap();
        assert_eq!(short, expected.trim());
    }

    fn conflicting() -> tempfile::TempDir {
        let dir = repo();
        commit(dir.path(), "f.txt", "base\n");
        git_in(dir.path(), &["switch", "-q", "-c", "feature"]);
        commit(dir.path(), "f.txt", "feature\n");
        git_in(dir.path(), &["switch", "-q", "main"]);
        commit(dir.path(), "f.txt", "main\n");
        dir
    }

    #[tokio::test]
    async fn an_unfinished_operation_is_told_by_what_git_leaves() {
        let dir = conflicting();
        assert_eq!(operation(dir.path()).await, None);
        let merge = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=a",
                "-c",
                "user.email=a@b",
                "merge",
                "feature",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(!merge.status.success());
        assert_eq!(operation(dir.path()).await, Some(Operation::Merge));
        git_in(dir.path(), &["merge", "--abort"]);

        git_in(dir.path(), &["switch", "-q", "feature"]);
        let rebase = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=a",
                "-c",
                "user.email=a@b",
                "rebase",
                "main",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(!rebase.status.success());
        assert_eq!(operation(dir.path()).await, Some(Operation::Rebase));
        git_in(dir.path(), &["rebase", "--abort"]);

        let pick = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=a",
                "-c",
                "user.email=a@b",
                "cherry-pick",
                "main",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(!pick.status.success());
        assert_eq!(operation(dir.path()).await, Some(Operation::CherryPick));
    }

    #[tokio::test]
    async fn the_branch_gathers_head_operation_chosen_base_and_comparison() {
        let dir = diverged();
        git_in(dir.path(), &["config", &base_key("feature"), "main"]);
        let found = branch(dir.path()).await.unwrap();
        assert_eq!(found.head, Head::Branch("feature".into()));
        assert_eq!(found.operation, None);
        assert_eq!(found.chosen.as_deref(), Some("main"));
        assert_eq!(found.comparison.ahead, 2);
    }
}
