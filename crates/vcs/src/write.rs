//! What tsumugi makes in a Git worktree: the changes it stages, unstages and
//! discards, the commits it makes, and what it pushes and pulls. Every
//! function takes the worktree's path and the [`Change`]s as
//! `git status` listed them, and names files only by literal path, so a name
//! with spaces, a glob character or non-ASCII letters is the file it names
//! and nothing else.

use std::path::Path;

use tokio::process::Command;

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

/// What git, asked `args` in `path`, tells on both its outputs, or why it
/// failed, told on both of them as well: the hooks of a commit or a push and
/// the remote's refusals speak on either, and what the user needs to fix a
/// failure is all of it. Git is never left waiting for a person: it may not
/// ask for a password, and a merge is not edited.
async fn tell(path: &Path, args: &[&str]) -> Result<String, Error> {
    let spawn_error = |source| Error::Spawn {
        program: "git".into(),
        source,
    };
    let out = Command::new("git")
        .args(["-C", git_dir(path)?, "--literal-pathspecs"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .output()
        .await
        .map_err(spawn_error)?;
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    let told = [said(&out.stdout), said(&out.stderr)]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if out.status.success() {
        Ok(told)
    } else {
        Err(Error::Failed {
            program: "git".into(),
            stderr: told,
        })
    }
}

/// Commit what is staged with `message`, or with `amend` take the last
/// commit's place with it, its changes and what is staged now. What git told
/// comes back, the hooks' words included; a hook that refuses fails it with
/// those words, and nothing is lost: the index stays as it was.
pub async fn commit(path: &Path, message: &str, amend: bool) -> Result<String, Error> {
    if message.trim().is_empty() {
        return Err(Error::Failed {
            program: "git".into(),
            stderr: "the commit message is empty".into(),
        });
    }
    let mut args = vec!["commit"];
    if amend {
        args.push("--amend");
    }
    args.extend(["--message", message]);
    tell(path, &args).await
}

/// The remote the branch checked out pushes to: the one its upstream is on,
/// or else `origin`.
async fn remote(path: &Path) -> Result<String, Error> {
    let branch = git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .await
        .map_err(|_| Error::Failed {
            program: "git".into(),
            stderr: "HEAD is not on a branch; check one out before you push".into(),
        })?;
    let key = format!("branch.{}.remote", branch.trim());
    Ok(git(path, &["config", "--get", &key])
        .await
        .map(|remote| remote.trim().to_string())
        .ok()
        .filter(|remote| !remote.is_empty() && remote != ".")
        .unwrap_or_else(|| "origin".into()))
}

/// Push the branch checked out, setting its upstream where it has none. With
/// `lease` it overwrites the remote's branch, but only if nobody pushed to
/// it since it was last fetched, so a push of someone else's is not lost.
pub async fn push(path: &Path, lease: bool) -> Result<String, Error> {
    let remote = remote(path).await?;
    let mut args = vec!["push"];
    if lease {
        args.push("--force-with-lease");
    }
    args.extend(["--set-upstream", &remote, "HEAD"]);
    tell(path, &args).await
}

/// Pull the upstream of the branch checked out, as git is configured to. Where
/// git would not choose between merging and rebasing diverged branches, they
/// are merged.
pub async fn pull(path: &Path) -> Result<String, Error> {
    match tell(path, &["pull", "--no-edit"]).await {
        Err(Error::Failed { stderr, .. }) if stderr.contains("divergent branches") => {
            tell(path, &["pull", "--no-edit", "--no-rebase"]).await
        }
        done => done,
    }
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

    /// A clone of a bare repository standing for the remote, and the remote.
    fn cloned() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let remote = root.path().join("remote.git");
        git_in(
            root.path(),
            &["init", "-q", "--bare", "-b", "main", "remote.git"],
        );
        let work = root.path().join("work");
        git_in(root.path(), &["clone", "-q", "remote.git", "work"]);
        git_in(&work, &["config", "user.name", "a"]);
        git_in(&work, &["config", "user.email", "a@b"]);
        git_in(&work, &["checkout", "-q", "-b", "main"]);
        write(&work, "a.txt", "1\n");
        git_in(&work, &["add", "--all"]);
        git_in(&work, &["commit", "-q", "-m", "first"]);
        (root, work, remote)
    }

    fn log(dir: &Path) -> Vec<String> {
        let out = std::process::Command::new("git")
            .args(["log", "--format=%s"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    #[tokio::test]
    async fn a_commit_takes_the_staged_changes_and_leaves_the_rest() {
        let dir = repo();
        git_in(dir.path(), &["config", "user.name", "a"]);
        git_in(dir.path(), &["config", "user.email", "a@b"]);
        write(dir.path(), "staged.txt", "x\n");
        write(dir.path(), "left.txt", "x\n");
        git_in(dir.path(), &["add", "staged.txt"]);
        commit(dir.path(), "add staged\n\nWhy: 日本語 \"quoted\" -x", false)
            .await
            .unwrap();
        assert_eq!(log(dir.path()), ["add staged"]);
        assert_eq!(statuses(dir.path()).await, [status("??", "left.txt")]);
    }

    #[tokio::test]
    async fn an_empty_message_and_nothing_staged_are_refused_with_the_reason() {
        let dir = repo();
        git_in(dir.path(), &["config", "user.name", "a"]);
        git_in(dir.path(), &["config", "user.email", "a@b"]);
        committed(dir.path(), &[("a.txt", "1\n")]);
        write(dir.path(), "a.txt", "2\n");
        let Err(Error::Failed { stderr, .. }) = commit(dir.path(), "  \n", false).await else {
            panic!("an empty message was taken");
        };
        assert!(stderr.contains("message is empty"), "{stderr}");
        git_in(dir.path(), &["add", "a.txt"]);
        commit(dir.path(), "second", false).await.unwrap();
        let Err(Error::Failed { stderr, .. }) = commit(dir.path(), "third", false).await else {
            panic!("an empty commit was made");
        };
        assert!(stderr.contains("nothing to commit"), "{stderr}");
    }

    #[tokio::test]
    async fn amending_replaces_the_last_commit_with_its_changes_and_what_is_staged() {
        let dir = repo();
        git_in(dir.path(), &["config", "user.name", "a"]);
        git_in(dir.path(), &["config", "user.email", "a@b"]);
        committed(dir.path(), &[("a.txt", "1\n")]);
        write(dir.path(), "b.txt", "x\n");
        git_in(dir.path(), &["add", "b.txt"]);
        commit(dir.path(), "first, better", true).await.unwrap();
        assert_eq!(log(dir.path()), ["first, better"]);
        let files = crate::git::commit_files(dir.path(), "HEAD").await.unwrap();
        assert_eq!(files.len(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_hook_that_refuses_a_commit_has_its_words_returned_and_the_index_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = repo();
        git_in(dir.path(), &["config", "user.name", "a"]);
        git_in(dir.path(), &["config", "user.email", "a@b"]);
        let hook = dir.path().join(".git/hooks/pre-commit");
        std::fs::write(
            &hook,
            "#!/bin/sh\necho 'lint said no'\necho 'and on stderr' >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        write(dir.path(), "a.txt", "x\n");
        git_in(dir.path(), &["add", "a.txt"]);
        let Err(Error::Failed { stderr, .. }) = commit(dir.path(), "msg", false).await else {
            panic!("the hook did not refuse");
        };
        assert!(stderr.contains("lint said no"), "{stderr}");
        assert!(stderr.contains("and on stderr"), "{stderr}");
        assert_eq!(statuses(dir.path()).await, [status("A ", "a.txt")]);
    }

    #[tokio::test]
    async fn the_first_push_sets_the_upstream_and_later_ones_follow_it() {
        let (_root, work, remote) = cloned();
        push(&work, false).await.unwrap();
        assert_eq!(log(&remote), ["first"]);
        write(&work, "a.txt", "2\n");
        git_in(&work, &["commit", "-q", "-am", "second"]);
        push(&work, false).await.unwrap();
        assert_eq!(log(&remote), ["second", "first"]);
        let upstream = git(&work, &["rev-parse", "--abbrev-ref", "@{upstream}"])
            .await
            .unwrap();
        assert_eq!(upstream.trim(), "origin/main");
    }

    #[tokio::test]
    async fn a_rewritten_branch_is_pushed_only_with_the_lease_and_only_if_nobody_else_pushed() {
        let (root, work, remote) = cloned();
        push(&work, false).await.unwrap();
        commit(&work, "first, reworded", true).await.unwrap();
        let Err(Error::Failed { stderr, .. }) = push(&work, false).await else {
            panic!("a rewritten branch was pushed plain");
        };
        assert!(stderr.contains("rejected"), "{stderr}");
        // Someone else pushes first: the lease holds the push back.
        let other = root.path().join("other");
        git_in(root.path(), &["clone", "-q", "remote.git", "other"]);
        git_in(&other, &["config", "user.name", "o"]);
        git_in(&other, &["config", "user.email", "o@b"]);
        write(&other, "o.txt", "x\n");
        git_in(&other, &["add", "--all"]);
        git_in(&other, &["commit", "-q", "-m", "theirs"]);
        git_in(&other, &["push", "-q"]);
        assert!(push(&work, true).await.is_err());
        assert_eq!(log(&remote), ["theirs", "first"]);
        // Once it is fetched and rewritten again, the lease lets it by.
        git_in(&work, &["fetch", "-q"]);
        push(&work, true).await.unwrap();
        assert_eq!(log(&remote), ["first, reworded"]);
    }

    #[tokio::test]
    async fn pulling_brings_in_what_the_remote_has_and_merges_diverged_branches() {
        let (root, work, remote) = cloned();
        push(&work, false).await.unwrap();
        let other = root.path().join("other");
        git_in(root.path(), &["clone", "-q", "remote.git", "other"]);
        git_in(&other, &["config", "user.name", "o"]);
        git_in(&other, &["config", "user.email", "o@b"]);
        write(&other, "o.txt", "x\n");
        git_in(&other, &["add", "--all"]);
        git_in(&other, &["commit", "-q", "-m", "theirs"]);
        git_in(&other, &["push", "-q"]);
        pull(&work).await.unwrap();
        assert_eq!(log(&work), ["theirs", "first"]);
        write(&work, "mine.txt", "x\n");
        git_in(&work, &["add", "--all"]);
        git_in(&work, &["commit", "-q", "-m", "mine"]);
        write(&other, "o2.txt", "x\n");
        git_in(&other, &["add", "--all"]);
        git_in(&other, &["commit", "-q", "-m", "theirs 2"]);
        git_in(&other, &["push", "-q"]);
        pull(&work).await.unwrap();
        let history = log(&work);
        assert!(history.contains(&"mine".to_string()), "{history:?}");
        assert!(history.contains(&"theirs 2".to_string()), "{history:?}");
        let _ = remote;
    }

    #[tokio::test]
    async fn pushing_with_a_detached_head_says_so() {
        let (_root, work, _remote) = cloned();
        git_in(&work, &["checkout", "-q", "--detach"]);
        let Err(Error::Failed { stderr, .. }) = push(&work, false).await else {
            panic!("a detached head was pushed");
        };
        assert!(stderr.contains("not on a branch"), "{stderr}");
    }
}
