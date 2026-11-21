//! What tsumugi makes in a Git worktree: the changes it stages, unstages and
//! discards. Every function takes the worktree's path and the [`Change`]s as
//! `git status` listed them, and names files only by literal path, so a name
//! with spaces, a glob character or non-ASCII letters is the file it names
//! and nothing else.

use std::path::Path;

use crate::git::{git, git_dir};
use crate::{Change, Error, run};

/// What git, asked `args` about `path`, makes of them, with every path
/// taken literally.
async fn make(path: &Path, args: &[&str]) -> Result<String, Error> {
    let mut all = vec!["-C", git_dir(path)?, "--literal-pathspecs"];
    all.extend(args);
    run("git", &all, None).await
}

/// The paths of `changes`: where a rename or copy came from, then where it
/// went.
fn paths(changes: &[&Change]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for change in changes {
        for path in change.paths() {
            if !found.iter().any(|p| p == path) {
                found.push(path.to_string());
            }
        }
    }
    found
}

fn refuse_conflicts(changes: &[Change], what: &str) -> Result<(), Error> {
    match changes.iter().find(|c| c.is_conflicted()) {
        Some(c) => Err(Error::Failed {
            program: "git".into(),
            stderr: format!("{} is in conflict; resolve it before you {what} it", c.path),
        }),
        None => Ok(()),
    }
}

/// Whether the worktree has a commit yet.
async fn born(path: &Path) -> bool {
    git(path, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .await
        .is_ok()
}

/// Put every change of `changes` in the index, as it is in the worktree: a
/// file it deleted is deleted there, and a file in conflict is marked
/// resolved as it stands.
pub async fn stage(path: &Path, changes: &[Change]) -> Result<(), Error> {
    let all: Vec<&Change> = changes.iter().collect();
    let files = paths(&all);
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add", "--all", "--"];
    args.extend(files.iter().map(String::as_str));
    make(path, &args).await.map(drop)
}

/// Stage everything the worktree changes, untracked files too.
pub async fn stage_all(path: &Path) -> Result<(), Error> {
    make(path, &["add", "--all"]).await.map(drop)
}

/// Take every change of `changes` out of the index, leaving the worktree as
/// it is. A file in conflict has no staged side to take out.
pub async fn unstage(path: &Path, changes: &[Change]) -> Result<(), Error> {
    refuse_conflicts(changes, "unstage")?;
    let all: Vec<&Change> = changes.iter().collect();
    let files = paths(&all);
    if files.is_empty() {
        return Ok(());
    }
    let mut args = if born(path).await {
        vec!["restore", "--staged", "--"]
    } else {
        // Without a commit there is nothing to restore the index from.
        vec!["rm", "--cached", "--quiet", "-r", "--ignore-unmatch", "--"]
    };
    args.extend(files.iter().map(String::as_str));
    make(path, &args).await.map(drop)
}

/// Lose every change of `changes`, staged or not: a tracked file goes back
/// to the last commit, and a file that commit does not have is deleted. What
/// this loses cannot be had back, so whoever asks has asked the user first.
pub async fn discard(path: &Path, changes: &[Change]) -> Result<(), Error> {
    refuse_conflicts(changes, "discard")?;
    let (mut untracked, mut added, mut renamed, mut restored) = (vec![], vec![], vec![], vec![]);
    for change in changes {
        let index = change.status.chars().next();
        if change.status == "??" {
            untracked.push(change);
        } else if matches!(index, Some('A' | 'C')) {
            added.push(change);
        } else if index == Some('R') {
            renamed.push(change);
        } else {
            restored.push(change);
        }
    }
    // A rename gives up the new file and takes the old one back.
    let gone: Vec<&Change> = added.iter().chain(&renamed).copied().collect();
    let back: Vec<&Change> = restored.iter().chain(&renamed).copied().collect();
    let run_on = |mut args: Vec<&'static str>, files: Vec<String>| async move {
        if files.is_empty() {
            return Ok(());
        }
        args.push("--");
        let mut all: Vec<&str> = args;
        all.extend(files.iter().map(String::as_str));
        make(path, &all).await.map(drop)
    };
    let new_files = |changes: &[&Change]| changes.iter().map(|c| c.path.clone()).collect();
    run_on(
        vec!["clean", "--force", "-d", "--quiet"],
        new_files(&untracked),
    )
    .await?;
    run_on(vec!["rm", "--force", "--quiet"], new_files(&gone)).await?;
    let sources = |changes: &[&Change]| {
        paths(changes)
            .into_iter()
            .filter(|p| !gone.iter().any(|c| &c.path == p))
            .collect()
    };
    run_on(
        vec!["restore", "--source=HEAD", "--staged", "--worktree"],
        sources(&back),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::changes;

    fn git_in(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q"]);
        dir
    }

    fn committed(dir: &Path, files: &[(&str, &str)]) {
        for (name, text) in files {
            let file = dir.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        git_in(dir, &["add", "--all"]);
        git_in(dir, &["commit", "-q", "-m", "first"]);
    }

    fn write(dir: &Path, name: &str, text: &str) {
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    async fn statuses(dir: &Path) -> Vec<(String, String)> {
        let mut found: Vec<_> = changes(dir)
            .await
            .unwrap()
            .into_iter()
            .map(|c| (c.status, c.path))
            .collect();
        found.sort_by(|a, b| a.1.cmp(&b.1));
        found
    }

    fn status(code: &str, path: &str) -> (String, String) {
        (code.into(), path.into())
    }

    async fn listed(dir: &Path, path: &str) -> Change {
        changes(dir)
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.path == path)
            .unwrap_or_else(|| panic!("{path} is not listed"))
    }

    #[tokio::test]
    async fn staging_and_unstaging_move_a_change_in_and_out_of_the_index() {
        let dir = repo();
        committed(dir.path(), &[("a.txt", "old\n")]);
        write(dir.path(), "a.txt", "new\n");
        let change = listed(dir.path(), "a.txt").await;

        stage(dir.path(), &[change]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("M ", "a.txt")]);

        let staged = listed(dir.path(), "a.txt").await;
        unstage(dir.path(), &[staged]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status(" M", "a.txt")]);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "new\n"
        );
    }

    #[tokio::test]
    async fn names_with_spaces_unicode_and_glob_characters_are_taken_literally() {
        let dir = repo();
        let names = [
            "with space.txt",
            "日本語 ファイル.txt",
            "star*.txt",
            "[br]acket.txt",
        ];
        for name in names {
            write(dir.path(), name, "x\n");
        }
        write(dir.path(), "starX.txt", "x\n");
        let star = listed(dir.path(), "star*.txt").await;
        stage(dir.path(), &[star]).await.unwrap();
        let all = statuses(dir.path()).await;
        assert!(all.contains(&status("A ", "star*.txt")));
        assert!(all.contains(&status("??", "starX.txt")), "{all:?}");

        for name in ["with space.txt", "日本語 ファイル.txt", "[br]acket.txt"] {
            let change = listed(dir.path(), name).await;
            stage(dir.path(), &[change]).await.unwrap();
        }
        let all = statuses(dir.path()).await;
        for name in names {
            assert!(all.contains(&status("A ", name)), "{name}: {all:?}");
        }
    }

    #[tokio::test]
    async fn a_deleted_file_is_staged_and_unstaged_like_any_other() {
        let dir = repo();
        committed(dir.path(), &[("gone.txt", "x\n")]);
        std::fs::remove_file(dir.path().join("gone.txt")).unwrap();
        let change = listed(dir.path(), "gone.txt").await;
        assert_eq!(change.status, " D");
        stage(dir.path(), &[change]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("D ", "gone.txt")]);
        let staged = listed(dir.path(), "gone.txt").await;
        unstage(dir.path(), &[staged]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status(" D", "gone.txt")]);
    }

    #[tokio::test]
    async fn a_rename_is_staged_and_unstaged_as_one_change() {
        let dir = repo();
        committed(dir.path(), &[("old name.txt", "same text\n")]);
        git_in(dir.path(), &["mv", "old name.txt", "新しい.txt"]);
        let renamed = listed(dir.path(), "新しい.txt").await;
        assert_eq!(renamed.from.as_deref(), Some("old name.txt"));
        unstage(dir.path(), std::slice::from_ref(&renamed))
            .await
            .unwrap();
        assert_eq!(
            statuses(dir.path()).await,
            [status(" D", "old name.txt"), status("??", "新しい.txt")]
        );
        let all = changes(dir.path()).await.unwrap();
        stage(dir.path(), &all).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("R ", "新しい.txt")]);
    }

    #[tokio::test]
    async fn unstaging_before_the_first_commit_leaves_the_files_untracked() {
        let dir = repo();
        write(dir.path(), "a.txt", "x\n");
        stage_all(dir.path()).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("A ", "a.txt")]);
        let staged = listed(dir.path(), "a.txt").await;
        unstage(dir.path(), &[staged]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("??", "a.txt")]);
    }

    #[tokio::test]
    async fn staging_all_takes_in_untracked_deleted_and_modified_files() {
        let dir = repo();
        committed(dir.path(), &[("m.txt", "1\n"), ("d.txt", "1\n")]);
        write(dir.path(), "m.txt", "2\n");
        std::fs::remove_file(dir.path().join("d.txt")).unwrap();
        write(dir.path(), "sub/new.txt", "something else entirely\n");
        stage_all(dir.path()).await.unwrap();
        assert_eq!(
            statuses(dir.path()).await,
            [
                status("D ", "d.txt"),
                status("M ", "m.txt"),
                status("A ", "sub/new.txt")
            ]
        );
    }

    #[tokio::test]
    async fn discarding_puts_a_tracked_file_back_as_the_last_commit_has_it() {
        let dir = repo();
        committed(dir.path(), &[("a.txt", "old\n"), ("b.txt", "old\n")]);
        write(dir.path(), "a.txt", "worktree\n");
        write(dir.path(), "b.txt", "staged\n");
        git_in(dir.path(), &["add", "b.txt"]);
        write(dir.path(), "b.txt", "staged and more\n");
        std::fs::remove_file(dir.path().join("a.txt")).unwrap();
        let all = changes(dir.path()).await.unwrap();
        discard(dir.path(), &all).await.unwrap();
        assert_eq!(statuses(dir.path()).await, []);
        for name in ["a.txt", "b.txt"] {
            assert_eq!(
                std::fs::read_to_string(dir.path().join(name)).unwrap(),
                "old\n"
            );
        }
    }

    #[tokio::test]
    async fn discarding_removes_untracked_and_newly_staged_files() {
        let dir = repo();
        committed(dir.path(), &[("kept.txt", "x\n")]);
        write(dir.path(), "new one.txt", "x\n");
        write(dir.path(), "staged.txt", "x\n");
        git_in(dir.path(), &["add", "staged.txt"]);
        write(dir.path(), "dir/日本.txt", "x\n");
        write(dir.path(), "dir/other.txt", "x\n");
        let all = changes(dir.path()).await.unwrap();
        let chosen: Vec<Change> = all
            .into_iter()
            .filter(|c| c.path != "dir/other.txt")
            .collect();
        discard(dir.path(), &chosen).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("??", "dir/other.txt")]);
        assert!(!dir.path().join("new one.txt").exists());
        assert!(!dir.path().join("staged.txt").exists());
        assert!(dir.path().join("kept.txt").exists());
    }

    #[tokio::test]
    async fn discarding_a_rename_gives_the_old_file_back_and_drops_the_new_one() {
        let dir = repo();
        committed(dir.path(), &[("old.txt", "same text\n")]);
        git_in(dir.path(), &["mv", "old.txt", "new.txt"]);
        let renamed = listed(dir.path(), "new.txt").await;
        discard(dir.path(), &[renamed]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, []);
        assert!(dir.path().join("old.txt").exists());
        assert!(!dir.path().join("new.txt").exists());
    }

    /// A merge of two branches that both changed `a.txt`.
    fn conflicted() -> tempfile::TempDir {
        let dir = repo();
        committed(dir.path(), &[("a.txt", "base\n")]);
        git_in(dir.path(), &["checkout", "-q", "-b", "other"]);
        write(dir.path(), "a.txt", "other\n");
        git_in(dir.path(), &["commit", "-q", "-am", "other"]);
        git_in(dir.path(), &["checkout", "-q", "-"]);
        write(dir.path(), "a.txt", "mine\n");
        git_in(dir.path(), &["commit", "-q", "-am", "mine"]);
        let merged = std::process::Command::new("git")
            .args(["merge", "-q", "other"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(!merged.status.success());
        dir
    }

    #[tokio::test]
    async fn a_file_in_conflict_is_staged_to_resolve_it_and_not_unstaged_or_discarded() {
        let dir = conflicted();
        let change = listed(dir.path(), "a.txt").await;
        assert_eq!(change.status, "UU");
        for refused in [
            unstage(dir.path(), std::slice::from_ref(&change)).await,
            discard(dir.path(), std::slice::from_ref(&change)).await,
        ] {
            let Err(Error::Failed { stderr, .. }) = refused else {
                panic!("a conflict was not refused");
            };
            assert!(stderr.contains("a.txt is in conflict"), "{stderr}");
        }
        assert_eq!(statuses(dir.path()).await, [status("UU", "a.txt")]);
        stage(dir.path(), &[change]).await.unwrap();
        assert_eq!(statuses(dir.path()).await, [status("M ", "a.txt")]);
    }
}
