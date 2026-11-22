//! Merging a finished branch into the main checkout without harming it: what
//! the merge would do is read first, the main checkout's own changes are
//! never stashed, committed or overwritten unless the person chose it, and a
//! merge that would conflict is not begun there at all.

use std::path::Path;

use tokio::process::Command;

use tsumugi_model::repository::{Head, Operation};
use tsumugi_model::{MainChanges, MergeStanding, Merged};

use crate::branch;
use crate::git::{changes, git};

type Result<T> = std::result::Result<T, String>;

fn said(error: crate::Error) -> String {
    error.to_string()
}

/// The branch `head` is on.
fn branch_of(head: Head) -> Option<String> {
    match head {
        Head::Branch(name) => Some(name),
        Head::Detached(_) => None,
    }
}

/// The files that merging `branch` into `base` would leave in conflict,
/// found by Git in the object database alone: no ref, index or file moves.
async fn conflicts(primary: &Path, base: &str, branch: &str) -> Result<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(primary)
        .args(["merge-tree", "--write-tree", "--name-only", "--no-messages"])
        .args([base, branch])
        .output()
        .await
        .map_err(|e| format!("cannot run git: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    match out.status.code() {
        Some(0) => Ok(Vec::new()),
        // The tree's id, then the files in conflict.
        Some(1) => Ok(text
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .map(str::to_string)
            .collect()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

/// How merging the branch checked out in the worktree at `worktree` into
/// the main checkout stands. Nothing is changed.
pub async fn standing(worktree: &Path) -> Result<MergeStanding> {
    let primary = crate::worktree::primary(worktree).await?;
    let top = git(
        worktree,
        &["rev-parse", "--path-format=absolute", "--show-toplevel"],
    )
    .await
    .map_err(said)?;
    let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    let mut standing = MergeStanding {
        main: same(Path::new(top.trim()), &primary),
        branch: branch::head(worktree).await.ok().and_then(branch_of),
        base: branch::head(&primary).await.ok().and_then(branch_of),
        changed: changes(worktree).await.map_err(said)?.len(),
        main_changed: changes(&primary).await.map_err(said)?.len(),
        main_operation: branch::operation(&primary).await,
        ..MergeStanding::default()
    };
    if let (Some(branch), Some(base)) = (&standing.branch, &standing.base)
        && !standing.main
    {
        let range = format!("{base}..{branch}");
        standing.ahead = git(&primary, &["rev-list", "--count", &range])
            .await
            .map_err(said)?
            .trim()
            .parse()
            .unwrap_or(0);
        if standing.ahead > 0 {
            standing.conflicts = conflicts(&primary, base, branch).await?;
        }
    }
    Ok(standing)
}

/// What the commit of the main checkout's changes says.
fn saving(branch: &str) -> String {
    format!("Save the changes in the working tree before merging {branch}")
}

/// Merge the branch checked out in the worktree at `worktree` into the
/// branch the main checkout has checked out, with a merge commit of its own,
/// saying `message` when one is given.
///
/// Nothing is done when the merge cannot be made or would conflict: the
/// conflicting files come back as [`Merged::Conflicts`] and neither the main
/// checkout nor the branch is touched. Changes in the main checkout are in
/// the way unless `changes` says what to do with them. A merge Git then
/// fails to finish is aborted, so the main checkout is as it was.
pub async fn merge(worktree: &Path, changes: MainChanges, message: Option<&str>) -> Result<Merged> {
    let standing = standing(worktree).await?;
    if let Some(why) = standing.refusal() {
        return Err(format!("cannot merge: {why}"));
    }
    let (Some(branch), Some(base)) = (standing.branch.clone(), standing.base.clone()) else {
        return Err("cannot merge: no branch".into());
    };
    if !standing.conflicts.is_empty() {
        return Ok(Merged::Conflicts {
            files: standing.conflicts,
        });
    }
    let primary = crate::worktree::primary(worktree).await?;
    let mut stash = None;
    if standing.main_dirty() {
        match changes {
            MainChanges::Refuse => {
                return Err(format!(
                    "the main checkout has {} files with uncommitted changes; stash or commit them first, nothing was changed",
                    standing.main_changed
                ));
            }
            MainChanges::Stash => {
                let name = format!("tsumugi: before merging {branch}");
                git(
                    &primary,
                    &["stash", "push", "--include-untracked", "--message", &name],
                )
                .await
                .map_err(said)?;
                stash = Some(name);
            }
            MainChanges::Commit => {
                git(&primary, &["add", "--all"]).await.map_err(said)?;
                git(&primary, &["commit", "--message", &saving(&branch)])
                    .await
                    .map_err(said)?;
            }
        }
    }
    let mut args = vec!["merge", "--no-ff"];
    match message {
        Some(message) => args.extend(["--message", message]),
        None => args.push("--no-edit"),
    }
    args.extend(["--", &branch]);
    if let Err(error) = git(&primary, &args).await {
        if branch::operation(&primary).await == Some(Operation::Merge) {
            let _ = git(&primary, &["merge", "--abort"]).await;
        }
        let kept = stash
            .map(|name| format!("; your changes are kept in the stash {name:?}"))
            .unwrap_or_default();
        return Err(format!(
            "the merge failed and {base} is as it was: {error}{kept}"
        ));
    }
    let commit = git(&primary, &["rev-parse", "--short", "HEAD"])
        .await
        .map_err(said)?
        .trim()
        .to_string();
    Ok(Merged::Done {
        branch,
        base,
        commit,
        stash,
        closed: None,
    })
}

/// Bring the branch the main checkout has checked out into the branch of the
/// worktree at `worktree`, to resolve there what would conflict in the main
/// checkout. The conflicts are left for the person or an agent to resolve,
/// continue or abort like any merge in progress; the main checkout is not
/// touched. Refused while the worktree has changes or an operation in
/// progress. The files left in conflict come back, none when it merged clean.
pub async fn bring_in_base(worktree: &Path) -> Result<Vec<String>> {
    let standing = standing(worktree).await?;
    let Some(base) = standing.base.clone() else {
        return Err("the main checkout has no branch checked out".into());
    };
    if standing.main {
        return Err("the main checkout is not merged into itself".into());
    }
    if let Some(operation) = branch::operation(worktree).await {
        return Err(format!(
            "a {} is in progress in the worktree; finish or abort it first",
            operation.name()
        ));
    }
    if standing.changed > 0 {
        return Err(format!(
            "the worktree has {} files with uncommitted changes; commit them first",
            standing.changed
        ));
    }
    if git(worktree, &["merge", "--no-edit", "--", &base])
        .await
        .is_ok()
    {
        return Ok(Vec::new());
    }
    let unmerged = git(worktree, &["diff", "--name-only", "--diff-filter=U"])
        .await
        .map_err(said)?;
    let files: Vec<String> = unmerged.lines().map(str::to_string).collect();
    if files.is_empty() {
        // The merge failed for another reason: leave nothing begun.
        if branch::operation(worktree).await == Some(Operation::Merge) {
            let _ = git(worktree, &["merge", "--abort"]).await;
        }
        return Err(format!("merging {base} into the branch failed"));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
    }

    fn commit(dir: &Path, name: &str, text: &str) {
        write(dir, name, text);
        run(dir, &["add", "--all"]);
        run(dir, &["commit", "-q", "-m", &format!("{name}: {text}")]);
    }

    /// A repository on `main` with a file `shared.txt`, and a worktree of
    /// the branch `feature` made from it.
    struct Repo {
        _dir: tempfile::TempDir,
        main: PathBuf,
        feature: PathBuf,
    }

    fn repo() -> Repo {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let main = root.join("api");
        std::fs::create_dir(&main).unwrap();
        run(&main, &["init", "-q", "-b", "main"]);
        run(&main, &["config", "user.name", "a"]);
        run(&main, &["config", "user.email", "a@b"]);
        commit(&main, "shared.txt", "one\n");
        let feature = root.join("api-feature");
        run(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature",
                feature.to_str().unwrap(),
            ],
        );
        Repo {
            _dir: dir,
            main,
            feature,
        }
    }

    fn head(dir: &Path) -> String {
        run(dir, &["rev-parse", "HEAD"]).trim().to_string()
    }

    #[tokio::test]
    async fn a_branch_with_commits_stands_ahead_of_a_clean_main_checkout() {
        let r = repo();
        commit(&r.feature, "new.txt", "new\n");

        let s = standing(&r.feature).await.unwrap();

        assert_eq!(s.branch.as_deref(), Some("feature"));
        assert_eq!(s.base.as_deref(), Some("main"));
        assert_eq!((s.ahead, s.changed, s.main_changed), (1, 0, 0));
        assert!(s.conflicts.is_empty() && !s.main && s.refusal().is_none());
    }

    #[tokio::test]
    async fn the_main_checkout_is_told_apart_from_a_worktree() {
        let r = repo();

        let s = standing(&r.main).await.unwrap();

        assert!(s.main && s.refusal().is_some());
    }

    #[tokio::test]
    async fn conflicting_files_are_found_without_touching_either_checkout() {
        let r = repo();
        commit(&r.main, "shared.txt", "main\n");
        commit(&r.feature, "shared.txt", "feature\n");
        let (main_head, feature_head) = (head(&r.main), head(&r.feature));

        let s = standing(&r.feature).await.unwrap();

        assert_eq!(s.conflicts, ["shared.txt"]);
        assert_eq!((head(&r.main), head(&r.feature)), (main_head, feature_head));
        assert_eq!(run(&r.main, &["status", "--porcelain"]), "");
    }

    #[tokio::test]
    async fn a_branch_is_merged_with_a_merge_commit_of_its_own() {
        let r = repo();
        commit(&r.feature, "new.txt", "new\n");
        let feature_head = head(&r.feature);

        let merged = merge(&r.feature, MainChanges::Refuse, None).await.unwrap();

        let Merged::Done {
            branch,
            base,
            commit,
            ..
        } = merged
        else {
            panic!("{merged:?}");
        };
        assert_eq!((branch.as_str(), base.as_str()), ("feature", "main"));
        assert!(head(&r.main).starts_with(&commit));
        let parents = run(&r.main, &["show", "--no-patch", "--format=%p", "HEAD"]);
        assert_eq!(parents.split_whitespace().count(), 2, "{parents}");
        assert_eq!(head(&r.feature), feature_head);
        assert!(r.main.join("new.txt").is_file());
    }

    #[tokio::test]
    async fn the_merge_commit_says_what_it_is_asked_to() {
        let r = repo();
        commit(&r.feature, "new.txt", "new\n");

        merge(
            &r.feature,
            MainChanges::Refuse,
            Some("treewide: merge feature"),
        )
        .await
        .unwrap();

        let subject = run(&r.main, &["log", "-1", "--format=%s"]);
        assert_eq!(subject.trim(), "treewide: merge feature");
    }

    #[tokio::test]
    async fn changes_in_the_main_checkout_stop_a_merge_that_was_not_told_what_to_do_with_them() {
        let r = repo();
        commit(&r.feature, "new.txt", "new\n");
        write(&r.main, "shared.txt", "mine\n");
        write(&r.main, "untracked.txt", "mine too\n");
        let before = head(&r.main);

        let refused = merge(&r.feature, MainChanges::Refuse, None)
            .await
            .unwrap_err();

        assert!(refused.contains("uncommitted"), "{refused}");
        assert_eq!(head(&r.main), before);
        assert_eq!(
            std::fs::read_to_string(r.main.join("shared.txt")).unwrap(),
            "mine\n"
        );
        assert!(r.main.join("untracked.txt").is_file());
        assert_eq!(run(&r.main, &["stash", "list"]), "");
    }

    #[tokio::test]
    async fn stashed_changes_are_kept_in_the_stash_and_never_popped() {
        let r = repo();
        commit(&r.feature, "new.txt", "new\n");
        write(&r.main, "shared.txt", "mine\n");
        write(&r.main, "untracked.txt", "mine too\n");

        let merged = merge(&r.feature, MainChanges::Stash, None).await.unwrap();

        let Merged::Done { stash, .. } = merged else {
            panic!("{merged:?}");
        };
        assert!(stash.unwrap().contains("feature"));
        assert!(r.main.join("new.txt").is_file());
        assert_eq!(
            std::fs::read_to_string(r.main.join("shared.txt")).unwrap(),
            "one\n"
        );
        assert!(!r.main.join("untracked.txt").exists());
        assert_eq!(run(&r.main, &["stash", "list"]).lines().count(), 1);
    }

    #[tokio::test]
    async fn committed_changes_are_a_commit_of_their_own_before_the_merge() {
        let r = repo();
        commit(&r.feature, "new.txt", "new\n");
        write(&r.main, "untracked.txt", "mine too\n");

        merge(&r.feature, MainChanges::Commit, None).await.unwrap();

        let log = run(&r.main, &["log", "--format=%s", "--no-merges"]);
        assert!(
            log.contains("Save the changes in the working tree"),
            "{log}"
        );
        assert_eq!(run(&r.main, &["status", "--porcelain"]), "");
        assert!(r.main.join("untracked.txt").is_file());
    }

    #[tokio::test]
    async fn a_merge_that_would_conflict_is_not_begun_in_the_main_checkout() {
        let r = repo();
        commit(&r.main, "shared.txt", "main\n");
        commit(&r.feature, "shared.txt", "feature\n");
        let before = head(&r.main);

        let merged = merge(&r.feature, MainChanges::Refuse, None).await.unwrap();

        assert_eq!(
            merged,
            Merged::Conflicts {
                files: vec!["shared.txt".into()]
            }
        );
        assert_eq!(head(&r.main), before);
        assert_eq!(branch::operation(&r.main).await, None);
        assert_eq!(run(&r.main, &["status", "--porcelain"]), "");
    }

    #[tokio::test]
    async fn conflicts_do_not_stash_the_main_checkout_s_changes() {
        let r = repo();
        commit(&r.main, "shared.txt", "main\n");
        commit(&r.feature, "shared.txt", "feature\n");
        write(&r.main, "untracked.txt", "mine\n");

        let merged = merge(&r.feature, MainChanges::Stash, None).await.unwrap();

        assert!(matches!(merged, Merged::Conflicts { .. }));
        assert!(r.main.join("untracked.txt").is_file());
        assert_eq!(run(&r.main, &["stash", "list"]), "");
    }

    #[tokio::test]
    async fn an_operation_unfinished_in_the_main_checkout_stops_the_merge() {
        let r = repo();
        run(&r.main, &["checkout", "-q", "-b", "other"]);
        commit(&r.main, "shared.txt", "other\n");
        run(&r.main, &["checkout", "-q", "main"]);
        commit(&r.main, "shared.txt", "main\n");
        let begun = std::process::Command::new("git")
            .args(["merge", "other"])
            .current_dir(&r.main)
            .output()
            .unwrap();
        assert!(!begun.status.success());
        commit(&r.feature, "new.txt", "new\n");

        let refused = merge(&r.feature, MainChanges::Stash, None)
            .await
            .unwrap_err();

        assert!(refused.contains("merge is in progress"), "{refused}");
        assert_eq!(branch::operation(&r.main).await, Some(Operation::Merge));
    }

    #[tokio::test]
    async fn a_branch_the_base_already_holds_is_nothing_to_merge() {
        let r = repo();

        let refused = merge(&r.feature, MainChanges::Refuse, None)
            .await
            .unwrap_err();

        assert!(refused.contains("lacks"), "{refused}");
    }

    #[tokio::test]
    async fn bringing_the_base_into_the_branch_leaves_the_conflicts_in_the_worktree_only() {
        let r = repo();
        commit(&r.main, "shared.txt", "main\n");
        commit(&r.feature, "shared.txt", "feature\n");
        let before = head(&r.main);

        let files = bring_in_base(&r.feature).await.unwrap();

        assert_eq!(files, ["shared.txt"]);
        assert_eq!(branch::operation(&r.feature).await, Some(Operation::Merge));
        assert_eq!(
            (head(&r.main), branch::operation(&r.main).await),
            (before, None)
        );
        crate::write::abort(&r.feature).await.unwrap();
        assert_eq!(branch::operation(&r.feature).await, None);
    }

    #[tokio::test]
    async fn bringing_the_base_in_waits_for_the_worktree_s_changes_to_be_committed() {
        let r = repo();
        commit(&r.main, "other.txt", "x\n");
        write(&r.feature, "shared.txt", "mine\n");

        let refused = bring_in_base(&r.feature).await.unwrap_err();

        assert!(refused.contains("uncommitted"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(r.feature.join("shared.txt")).unwrap(),
            "mine\n"
        );
    }

    #[tokio::test]
    async fn a_base_that_merges_clean_into_the_branch_leaves_no_conflicts() {
        let r = repo();
        commit(&r.main, "other.txt", "x\n");

        assert_eq!(
            bring_in_base(&r.feature).await.unwrap(),
            Vec::<String>::new()
        );
        assert!(r.feature.join("other.txt").is_file());
    }
}
