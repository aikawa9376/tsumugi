//! What tsumugi makes in a Git worktree: the changes it stages, unstages and
//! discards, the commits it makes, and what it pushes and pulls. Every
//! function takes the worktree's path and the [`Change`]s as
//! `git status` listed them, and names files only by literal path, so a name
//! with spaces, a glob character or non-ASCII letters is the file it names
//! and nothing else.

use std::path::Path;

use tokio::process::Command;

use tsumugi_model::repository::{Head, Operation};

use crate::branch;
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

/// Fail while a merge, rebase, cherry-pick or revert is unfinished in the
/// worktree: what is `what` would be built on a state Git has not settled,
/// and is lost or tangled with it.
async fn refuse_during_operation(path: &Path, what: &str) -> Result<(), Error> {
    match branch::operation(path).await {
        Some(operation) => Err(Error::Failed {
            program: "git".into(),
            stderr: format!(
                "a {} is in progress; finish or abort it before you {what}",
                operation.name()
            ),
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

/// Apply `patch`, a unified diff of one file, to the index when `index`,
/// else to the worktree; `reverse` applies it backwards.
pub async fn apply_patch(
    path: &Path,
    patch: &str,
    index: bool,
    reverse: bool,
) -> Result<(), Error> {
    let file = tempfile::NamedTempFile::new().map_err(|source| Error::Spawn {
        program: "git".into(),
        source,
    })?;
    std::fs::write(file.path(), patch).map_err(|source| Error::Spawn {
        program: "git".into(),
        source,
    })?;
    let patch_path = file.path().to_string_lossy().into_owned();
    let mut args = vec!["apply", "--whitespace=nowarn"];
    if index {
        args.push("--cached");
    }
    if reverse {
        args.push("--reverse");
    }
    args.push(&patch_path);
    let all = {
        let mut all = vec!["-C", git_dir(path)?];
        all.extend(args);
        all
    };
    run("git", &all, None).await.map(drop)
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
        // The last commit is not the one this operation is making.
        refuse_during_operation(path, "amend the last commit").await?;
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
    refuse_during_operation(path, "pull").await?;
    match tell(path, &["pull", "--no-edit"]).await {
        Err(Error::Failed { stderr, .. }) if stderr.contains("divergent branches") => {
            tell(path, &["pull", "--no-edit", "--no-rebase"]).await
        }
        done => done,
    }
}

/// The operation unfinished in the worktree, or why there is none.
async fn unfinished(path: &Path) -> Result<Operation, Error> {
    branch::operation(path).await.ok_or_else(|| Error::Failed {
        program: "git".into(),
        stderr: "no merge, rebase, cherry-pick or revert is in progress".into(),
    })
}

/// Give up the merge, rebase, cherry-pick or revert in progress and put the
/// worktree back as it was before it began. What was resolved during it is
/// lost, so whoever asks has asked the user first.
pub async fn abort(path: &Path) -> Result<String, Error> {
    let operation = unfinished(path).await?;
    tell(path, &[operation.name(), "--abort"]).await
}

/// Finish what the user resolved of the operation in progress: a merge is
/// committed, a rebase goes on to its next commit, a cherry-pick or revert
/// makes its commit. A file still in conflict stops it with the files named,
/// and a rebase that meets another conflict stops there, as told.
pub async fn resume(path: &Path) -> Result<String, Error> {
    let operation = unfinished(path).await?;
    let unmerged = make(path, &["ls-files", "--unmerged", "-z"]).await?;
    let mut left: Vec<&str> = unmerged
        .split('\0')
        .filter_map(|entry| entry.split_once('\t').map(|(_, name)| name))
        .collect();
    left.dedup();
    if !left.is_empty() {
        return Err(Error::Failed {
            program: "git".into(),
            stderr: format!(
                "{} still in conflict; resolve and stage {} before you continue the {}",
                left.join(", "),
                if left.len() == 1 { "it" } else { "them" },
                operation.name()
            ),
        });
    }
    match tell(path, &[operation.name(), "--continue"]).await {
        Err(Error::Failed { stderr, .. })
            if branch::operation(path).await == Some(operation)
                && !make(path, &["ls-files", "--unmerged"])
                    .await
                    .unwrap_or_default()
                    .is_empty() =>
        {
            // The next commit of a rebase conflicts: the operation goes on.
            Ok(stderr)
        }
        done => done,
    }
}

fn fail(stderr: impl Into<String>) -> Error {
    Error::Failed {
        program: "git".into(),
        stderr: stderr.into(),
    }
}

/// Only a full object id names a commit to act on: a name or an option
/// would let what the list showed and what git runs differ.
fn require_full_id(commit: &str) -> Result<(), Error> {
    let hex = commit.bytes().all(|b| b.is_ascii_hexdigit());
    if hex && matches!(commit.len(), 40 | 64) {
        Ok(())
    } else {
        Err(fail(format!("{commit} is not a full commit id")))
    }
}

/// Whether what git said of a refused cherry-pick or revert is that the
/// worktree's own changes are in its way.
fn is_dirty_refusal(stderr: &str) -> bool {
    let said = stderr.to_lowercase();
    [
        "would be overwritten",
        "local changes",
        "commit your changes or stash",
        "uncommitted changes",
    ]
    .iter()
    .any(|needle| said.contains(needle))
}

/// Cherry-pick or revert (`verb`) `commit` onto the branch checked out.
///
/// It is refused, with nothing touched, while another operation is
/// unfinished, for a merge commit, and where the worktree's changes are in
/// the way. A commit already applied (or already undone) is told as a
/// refusal and leaves no operation behind. A conflict is no failure: the
/// operation stays in progress for the user to resolve and continue, or
/// abort, and what to do is told.
async fn apply(path: &Path, verb: &'static str, commit: &str) -> Result<String, Error> {
    require_full_id(commit)?;
    refuse_during_operation(path, verb).await?;
    let parents = make(path, &["show", "--no-patch", "--format=%P", commit, "--"]).await?;
    if parents.split_whitespace().count() > 1 {
        return Err(fail(format!("cannot {verb} a merge commit from here")));
    }
    let Err(Error::Failed { stderr, .. }) = tell(path, &[verb, "--no-edit", commit]).await else {
        return tell(path, &["rev-parse", "--short", "HEAD"])
            .await
            .map(|head| format!("{verb} done, now at {head}"));
    };
    let began = branch::operation(path)
        .await
        .is_some_and(|o| o.name() == verb);
    let unmerged = began && !make(path, &["ls-files", "--unmerged"]).await?.is_empty();
    if unmerged {
        return Ok(format!(
            "{stderr}\nresolve the conflicts, then continue or abort the {verb}"
        ));
    }
    if began && tell(path, &[verb, "--skip"]).await.is_err() {
        // It stopped on a commit that would change nothing; give it up.
        tell(path, &[verb, "--abort"]).await?;
    }
    if began || stderr.contains("nothing to commit") {
        return Err(fail(if verb == "revert" {
            "nothing to revert, the changes are already undone"
        } else {
            "already applied, nothing to cherry-pick"
        }));
    }
    Err(fail(if is_dirty_refusal(&stderr) {
        "commit or stash your changes, then try again".to_string()
    } else {
        stderr
    }))
}

/// Apply the changes of `commit` as a commit of its own on the branch
/// checked out. See [`apply`] for what is refused.
pub async fn cherry_pick(path: &Path, commit: &str) -> Result<String, Error> {
    apply(path, "cherry-pick", commit).await
}

/// Undo the changes of `commit` in a commit of its own on the branch
/// checked out. See [`apply`] for what is refused.
pub async fn revert(path: &Path, commit: &str) -> Result<String, Error> {
    apply(path, "revert", commit).await
}

/// Make the branch `name` at `commit` without checking it out and without an
/// upstream. A name that is taken or is not a branch name is refused, and
/// nothing is made.
pub async fn create_branch(path: &Path, name: &str, commit: &str) -> Result<(), Error> {
    require_full_id(commit)?;
    make(path, &["check-ref-format", "--branch", name])
        .await
        .map_err(|_| fail(format!("{name:?} is not a branch name")))?;
    let taken = format!("refs/heads/{name}");
    if make(path, &["rev-parse", "--verify", "--quiet", &taken])
        .await
        .is_ok()
    {
        return Err(fail(format!("the branch {name} exists already")));
    }
    let at = format!("{commit}^{{commit}}");
    make(path, &["branch", "--no-track", "--", name, &at])
        .await
        .map(drop)
}

/// Compare the branch checked out with `base` from now on, or with the one
/// Tsumugi finds when `base` is `None`. A base that is not a commit of the
/// repository is refused, so a comparison is never left without one.
pub async fn choose_base(path: &Path, base: Option<&str>) -> Result<(), Error> {
    let fail = |stderr: String| Error::Failed {
        program: "git".into(),
        stderr,
    };
    let Head::Branch(name) = branch::head(path).await? else {
        return Err(fail(
            "HEAD is not on a branch; check one out before you choose its base".into(),
        ));
    };
    let key = branch::base_key(&name);
    match base.map(str::trim) {
        None | Some("") => {
            // A key never set is already what was asked.
            let _ = git(path, &["config", "--unset", &key]).await;
            Ok(())
        }
        Some(base) => {
            if base.starts_with('-') || !branch::resolves(path, base).await {
                return Err(fail(format!(
                    "{base} does not name a commit in this repository"
                )));
            }
            git(path, &["config", &key, base]).await.map(drop)
        }
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

    const SOME_LINES: &str = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,2 @@\n one\n-two\n three\n";

    #[tokio::test]
    async fn a_patch_stages_and_unstages_only_its_lines_and_discards_them_from_the_worktree() {
        let dir = repo();
        committed(dir.path(), &[("a.txt", "one\ntwo\nthree\n")]);
        write(dir.path(), "a.txt", "one\nTWO\nTHREE\n");
        let staged = || {
            let out = std::process::Command::new("git")
                .args(["show", ":a.txt"])
                .current_dir(dir.path())
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap()
        };
        apply_patch(dir.path(), SOME_LINES, true, false)
            .await
            .unwrap();
        assert_eq!(staged(), "one\nthree\n");
        apply_patch(dir.path(), SOME_LINES, true, true)
            .await
            .unwrap();
        assert_eq!(staged(), "one\ntwo\nthree\n");
        assert!(
            apply_patch(dir.path(), "not a patch", true, false)
                .await
                .is_err()
        );
        let gone = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,2 @@\n one\n-TWO\n THREE\n";
        apply_patch(dir.path(), gone, false, false).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\nTHREE\n"
        );
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
        let dir = repo();
        git_in(dir.path(), &["config", "user.name", "a"]);
        git_in(dir.path(), &["config", "user.email", "a@b"]);
        let hook = dir.path().join(".git/hooks/pre-commit");
        crate::testing::program(
            &hook,
            "#!/bin/sh\necho 'lint said no'\necho 'and on stderr' >&2\nexit 1\n",
        );
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

    /// `main` and `feature` both rewrote `f.txt` since they parted, and `main`
    /// is checked out.
    fn conflicting() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q", "-b", "main"]);
        for (branch, text) in [("feature", "feature\n"), ("main", "main\n")] {
            if branch == "feature" {
                committed(dir.path(), &[("f.txt", "base\n")]);
                git_in(dir.path(), &["switch", "-q", "-c", "feature"]);
            } else {
                git_in(dir.path(), &["switch", "-q", "main"]);
            }
            write(dir.path(), "f.txt", text);
            git_in(dir.path(), &["commit", "-q", "-am", branch]);
        }
        dir
    }

    fn failing(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=a", "-c", "user.email=a@b"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
            .status
            .success();
        assert!(!ok, "git {args:?} was expected to stop");
    }

    #[tokio::test]
    async fn aborting_puts_the_worktree_back_as_before_the_merge_began() {
        let dir = conflicting();
        failing(dir.path(), &["merge", "feature"]);
        assert_eq!(statuses(dir.path()).await, [status("UU", "f.txt")]);
        abort(dir.path()).await.unwrap();
        assert!(statuses(dir.path()).await.is_empty());
        assert_eq!(branch::operation(dir.path()).await, None);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
            "main\n"
        );
    }

    #[tokio::test]
    async fn aborting_a_rebase_cherry_pick_or_revert_goes_by_its_own_command() {
        let dir = conflicting();
        git_in(dir.path(), &["switch", "-q", "feature"]);
        failing(dir.path(), &["rebase", "main"]);
        assert_eq!(branch::operation(dir.path()).await, Some(Operation::Rebase));
        abort(dir.path()).await.unwrap();
        assert_eq!(log(dir.path()), ["feature", "first"]);
        failing(dir.path(), &["cherry-pick", "main"]);
        assert_eq!(
            branch::operation(dir.path()).await,
            Some(Operation::CherryPick)
        );
        abort(dir.path()).await.unwrap();
        git_in(dir.path(), &["switch", "-q", "main"]);
        failing(dir.path(), &["revert", "HEAD~1"]);
        assert_eq!(branch::operation(dir.path()).await, Some(Operation::Revert));
        abort(dir.path()).await.unwrap();
        assert_eq!(branch::operation(dir.path()).await, None);
    }

    #[tokio::test]
    async fn aborting_with_nothing_in_progress_changes_nothing_and_says_so() {
        let dir = conflicting();
        write(dir.path(), "f.txt", "edited\n");
        let Err(Error::Failed { stderr, .. }) = abort(dir.path()).await else {
            panic!("nothing was aborted");
        };
        assert!(stderr.contains("in progress"), "{stderr}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
            "edited\n"
        );
    }

    #[tokio::test]
    async fn continuing_a_merge_waits_for_every_file_to_be_resolved_and_staged() {
        let dir = conflicting();
        failing(dir.path(), &["merge", "feature"]);
        let Err(Error::Failed { stderr, .. }) = resume(dir.path()).await else {
            panic!("a merge went on with a file in conflict");
        };
        assert!(stderr.contains("f.txt"), "{stderr}");
        assert_eq!(branch::operation(dir.path()).await, Some(Operation::Merge));
        write(dir.path(), "f.txt", "resolved\n");
        git_in(dir.path(), &["add", "f.txt"]);
        resume(dir.path()).await.unwrap();
        assert_eq!(branch::operation(dir.path()).await, None);
        assert!(log(dir.path()).first().unwrap().starts_with("Merge"));
    }

    #[tokio::test]
    async fn continuing_a_rebase_that_meets_another_conflict_stops_there_without_failing() {
        let dir = conflicting();
        git_in(dir.path(), &["switch", "-q", "feature"]);
        write(dir.path(), "f.txt", "feature again\n");
        git_in(dir.path(), &["commit", "-q", "-am", "feature again"]);
        failing(dir.path(), &["rebase", "main"]);
        write(dir.path(), "f.txt", "first resolved\n");
        git_in(dir.path(), &["add", "f.txt"]);
        let told = resume(dir.path()).await.unwrap();
        assert!(told.contains("CONFLICT"), "{told}");
        assert_eq!(branch::operation(dir.path()).await, Some(Operation::Rebase));
        write(dir.path(), "f.txt", "second resolved\n");
        git_in(dir.path(), &["add", "f.txt"]);
        resume(dir.path()).await.unwrap();
        assert_eq!(branch::operation(dir.path()).await, None);
        assert_eq!(
            log(dir.path()),
            ["feature again", "feature", "main", "first"]
        );
    }

    #[tokio::test]
    async fn continuing_with_nothing_in_progress_says_so() {
        let dir = conflicting();
        let Err(Error::Failed { stderr, .. }) = resume(dir.path()).await else {
            panic!("nothing was continued");
        };
        assert!(stderr.contains("in progress"), "{stderr}");
    }

    #[tokio::test]
    async fn pulling_and_amending_are_refused_while_an_operation_is_unfinished() {
        let dir = conflicting();
        failing(dir.path(), &["merge", "feature"]);
        for failed in [
            pull(dir.path()).await,
            commit(dir.path(), "again", true).await,
        ] {
            let Err(Error::Failed { stderr, .. }) = failed else {
                panic!("went on during a merge");
            };
            assert!(stderr.contains("merge is in progress"), "{stderr}");
        }
        // The merge itself is as it was left.
        assert_eq!(statuses(dir.path()).await, [status("UU", "f.txt")]);
        assert_eq!(branch::operation(dir.path()).await, Some(Operation::Merge));
    }

    #[tokio::test]
    async fn a_base_is_chosen_for_the_branch_checked_out_and_given_up_again() {
        let dir = conflicting();
        choose_base(dir.path(), Some("feature")).await.unwrap();
        assert_eq!(
            branch::chosen(dir.path(), "main").await.as_deref(),
            Some("feature")
        );
        assert_eq!(branch::chosen(dir.path(), "feature").await, None);
        choose_base(dir.path(), None).await.unwrap();
        assert_eq!(branch::chosen(dir.path(), "main").await, None);
        // Giving up a base never chosen is not a failure.
        choose_base(dir.path(), None).await.unwrap();
    }

    #[tokio::test]
    async fn a_base_that_is_not_a_commit_is_refused_and_the_one_chosen_stays() {
        let dir = conflicting();
        choose_base(dir.path(), Some("feature")).await.unwrap();
        for bad in ["nowhere", "--output=x"] {
            let Err(Error::Failed { stderr, .. }) = choose_base(dir.path(), Some(bad)).await else {
                panic!("{bad} was chosen");
            };
            assert!(stderr.contains(bad), "{stderr}");
        }
        assert_eq!(
            branch::chosen(dir.path(), "main").await.as_deref(),
            Some("feature")
        );
        git_in(dir.path(), &["checkout", "-q", "--detach"]);
        assert!(choose_base(dir.path(), Some("feature")).await.is_err());
    }

    /// The full id of `rev`.
    fn id_of(dir: &Path, rev: &str) -> String {
        let out = std::process::Command::new("git")
            .args(["rev-parse", rev])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// `main` holds `base`; `feature` adds `side.txt`, which `main` lacks.
    fn parted() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q", "-b", "main"]);
        committed(dir.path(), &[("f.txt", "base\n")]);
        git_in(dir.path(), &["switch", "-q", "-c", "feature"]);
        write(dir.path(), "side.txt", "side\n");
        git_in(dir.path(), &["add", "side.txt"]);
        git_in(dir.path(), &["commit", "-q", "-m", "side"]);
        git_in(dir.path(), &["switch", "-q", "main"]);
        dir
    }

    fn stderr_of(result: Result<String, Error>) -> String {
        match result {
            Err(Error::Failed { stderr, .. }) => stderr,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_commit_is_cherry_picked_onto_the_branch_checked_out() {
        let dir = parted();
        let side = id_of(dir.path(), "feature");
        cherry_pick(dir.path(), &side).await.unwrap();
        assert_eq!(log(dir.path()), ["side", "first"]);
        assert!(dir.path().join("side.txt").exists());
        assert_eq!(branch::operation(dir.path()).await, None);
    }

    #[tokio::test]
    async fn a_cherry_pick_that_conflicts_stays_in_progress_and_says_how_to_go_on() {
        let dir = conflicting();
        let feature = id_of(dir.path(), "feature");
        let told = cherry_pick(dir.path(), &feature).await.unwrap();
        assert!(told.contains("conflict"), "{told}");
        assert_eq!(
            branch::operation(dir.path()).await,
            Some(Operation::CherryPick)
        );
        assert_eq!(statuses(dir.path()).await, [status("UU", "f.txt")]);
        // What the user then does with it is the operation's own.
        abort(dir.path()).await.unwrap();
        assert_eq!(log(dir.path()), ["main", "first"]);
    }

    #[tokio::test]
    async fn a_cherry_pick_over_changes_it_would_overwrite_fails_and_touches_nothing() {
        let dir = conflicting();
        let feature = id_of(dir.path(), "feature");
        write(dir.path(), "f.txt", "mine\n");
        let stderr = stderr_of(cherry_pick(dir.path(), &feature).await);
        assert_eq!(stderr, "commit or stash your changes, then try again");
        assert_eq!(branch::operation(dir.path()).await, None);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
            "mine\n"
        );
        assert_eq!(log(dir.path()), ["main", "first"]);
    }

    #[tokio::test]
    async fn a_commit_already_applied_is_told_and_leaves_no_operation_behind() {
        let dir = parted();
        let side = id_of(dir.path(), "feature");
        cherry_pick(dir.path(), &side).await.unwrap();
        let stderr = stderr_of(cherry_pick(dir.path(), &side).await);
        assert_eq!(stderr, "already applied, nothing to cherry-pick");
        assert_eq!(branch::operation(dir.path()).await, None);
        assert_eq!(log(dir.path()), ["side", "first"]);
        assert!(statuses(dir.path()).await.is_empty());
    }

    #[tokio::test]
    async fn a_merge_commit_is_not_cherry_picked_or_reverted_from_here() {
        let dir = parted();
        git_in(dir.path(), &["switch", "-q", "-c", "other"]);
        write(dir.path(), "o.txt", "o\n");
        git_in(dir.path(), &["add", "o.txt"]);
        git_in(dir.path(), &["commit", "-q", "-m", "other"]);
        git_in(dir.path(), &["switch", "-q", "main"]);
        git_in(
            dir.path(),
            &["merge", "-q", "--no-ff", "-m", "joined", "other"],
        );
        let merge = id_of(dir.path(), "HEAD");
        let stderr = stderr_of(cherry_pick(dir.path(), &merge).await);
        assert_eq!(stderr, "cannot cherry-pick a merge commit from here");
        let stderr = stderr_of(revert(dir.path(), &merge).await);
        assert_eq!(stderr, "cannot revert a merge commit from here");
        assert_eq!(branch::operation(dir.path()).await, None);
    }

    #[tokio::test]
    async fn a_cherry_pick_or_revert_waits_for_the_operation_in_progress() {
        let dir = conflicting();
        let feature = id_of(dir.path(), "feature");
        failing(dir.path(), &["merge", "feature"]);
        for stderr in [
            stderr_of(cherry_pick(dir.path(), &feature).await),
            stderr_of(revert(dir.path(), &feature).await),
        ] {
            assert!(stderr.contains("merge is in progress"), "{stderr}");
        }
        assert_eq!(branch::operation(dir.path()).await, Some(Operation::Merge));
    }

    #[tokio::test]
    async fn a_commit_is_reverted_by_a_commit_of_its_own() {
        let dir = parted();
        let side = id_of(dir.path(), "feature");
        cherry_pick(dir.path(), &side).await.unwrap();
        revert(dir.path(), &id_of(dir.path(), "HEAD"))
            .await
            .unwrap();
        assert_eq!(log(dir.path()), ["Revert \"side\"", "side", "first"]);
        assert!(!dir.path().join("side.txt").exists());
    }

    #[tokio::test]
    async fn a_revert_of_what_is_already_undone_is_told_and_leaves_no_operation_behind() {
        let dir = parted();
        let side = id_of(dir.path(), "feature");
        cherry_pick(dir.path(), &side).await.unwrap();
        let applied = id_of(dir.path(), "HEAD");
        revert(dir.path(), &applied).await.unwrap();
        let stderr = stderr_of(revert(dir.path(), &applied).await);
        assert_eq!(stderr, "nothing to revert, the changes are already undone");
        assert_eq!(branch::operation(dir.path()).await, None);
        assert_eq!(log(dir.path()).len(), 3);
    }

    #[tokio::test]
    async fn only_a_full_commit_id_is_taken_to_cherry_pick_or_revert() {
        let dir = parted();
        for bad in ["feature", "--abort", "HEAD", "abc123"] {
            let stderr = stderr_of(cherry_pick(dir.path(), bad).await);
            assert!(stderr.contains(bad), "{stderr}");
            assert!(revert(dir.path(), bad).await.is_err());
        }
        assert_eq!(log(dir.path()), ["first"]);
    }

    #[tokio::test]
    async fn a_branch_is_made_at_a_commit_without_checking_it_out() {
        let dir = parted();
        let side = id_of(dir.path(), "feature");
        create_branch(dir.path(), "from-side", &side).await.unwrap();
        assert_eq!(id_of(dir.path(), "from-side"), side);
        assert_eq!(
            branch::head(dir.path()).await.unwrap(),
            Head::Branch("main".into())
        );
        // It does not follow the branch it was cut from.
        assert!(
            git(dir.path(), &["config", "--get", "branch.from-side.merge"])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_branch_name_that_is_taken_or_not_a_name_is_refused_and_nothing_is_made() {
        let dir = parted();
        let side = id_of(dir.path(), "feature");
        let taken = stderr_of(
            create_branch(dir.path(), "feature", &side)
                .await
                .map(|()| "".into()),
        );
        assert!(taken.contains("feature"), "{taken}");
        for bad in ["", "  ", "-x", "a b", "a..b"] {
            assert!(
                create_branch(dir.path(), bad, &side).await.is_err(),
                "{bad:?}"
            );
        }
        let stderr = stderr_of(
            create_branch(dir.path(), "x", "feature")
                .await
                .map(|()| "".into()),
        );
        assert!(stderr.contains("feature"), "{stderr}");
        let branches = git(dir.path(), &["branch", "--format=%(refname:short)"])
            .await
            .unwrap();
        assert_eq!(branches.lines().collect::<Vec<_>>(), ["feature", "main"]);
    }
}
