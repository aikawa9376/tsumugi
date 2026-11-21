//! What an agent is shown to write a commit message or a pull request: read
//! from the worktree with Git, and changing nothing in it.

use std::path::Path;

use tsumugi_model::repository::Compared;
use tsumugi_model::{CommitContext, PullContext};

use crate::branch;
use crate::git::git;
use crate::{Error, Revision};

const UNQUOTED: &str = "core.quotepath=false";

fn failed(why: impl Into<String>) -> Error {
    Error::Failed {
        program: "git".into(),
        stderr: why.into(),
    }
}

/// The branch checked out, or `(detached)`.
async fn branch_name(path: &Path) -> String {
    git(path, &["symbolic-ref", "--short", "-q", "HEAD"])
        .await
        .map(|out| out.trim().to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "(detached)".into())
}

/// What a commit of the staged changes is written from. Nothing staged is
/// an error, since there would be nothing to write a message for.
pub async fn commit_context(path: &Path) -> Result<CommitContext, Error> {
    let files = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "diff",
            "--cached",
            "--name-status",
            "-M",
            "--no-ext-diff",
        ],
    )
    .await?;
    if files.trim().is_empty() {
        return Err(failed("Nothing is staged to write a commit message for"));
    }
    let patch = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "diff",
            "--cached",
            "-M",
            "--no-ext-diff",
            "--no-color",
        ],
    )
    .await?;
    Ok(CommitContext {
        branch: branch_name(path).await,
        staged_files: files,
        staged_patch: patch,
    })
}

/// What a pull request of the branch checked out is written from: the commits
/// and changes it has over its base, as the Changes list compares them. A
/// branch that cannot be compared says why.
pub async fn pull_context(path: &Path) -> Result<PullContext, Error> {
    let comparison = branch::compare(path, None).await;
    let (base, merge_base) = match (&comparison.status, &comparison.base, &comparison.merge_base) {
        (Compared::Ready, Some(base), Some(merge_base)) => (base.clone(), merge_base.clone()),
        (Compared::Ready, _, _) => {
            return Err(failed(
                "The branch has no commit to open a pull request for",
            ));
        }
        (Compared::NoBase(why) | Compared::Unrelated(why) | Compared::Failed(why), _, _) => {
            return Err(failed(why.clone()));
        }
        (Compared::Unborn, _, _) => {
            return Err(failed(
                "The branch has no commit to open a pull request for",
            ));
        }
    };
    if comparison.ahead == 0 {
        return Err(failed(format!(
            "The branch has no commit over {base} to open a pull request for"
        )));
    }
    let range = format!("{merge_base}..HEAD");
    let commits = git(
        path,
        &[
            "log",
            "--oneline",
            "--no-decorate",
            "--no-show-signature",
            &range,
        ],
    )
    .await?;
    let stat = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "diff",
            "--stat",
            "-M",
            "--no-ext-diff",
            "--no-color",
            &merge_base,
            "HEAD",
        ],
    )
    .await?;
    let patch = git(
        path,
        &[
            "-c",
            UNQUOTED,
            "diff",
            "-M",
            "--no-ext-diff",
            "--no-color",
            &merge_base,
            "HEAD",
        ],
    )
    .await?;
    let last: Revision = crate::git::revision(path, "HEAD").await?;
    let (title, body) = match last.description.split_once('\n') {
        Some((title, body)) => (title.trim().to_string(), body.trim().to_string()),
        None => (last.description.trim().to_string(), String::new()),
    };
    Ok(PullContext {
        branch: branch_name(path).await,
        base,
        current_title: title,
        current_body: body,
        commit_summary: commits,
        changed_files: stat,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.path().join("a.rs"), "fn old() {}\n").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-qm", "initial"]);
        dir
    }

    fn run(dir: &tempfile::TempDir, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
    }

    #[tokio::test]
    async fn a_commit_message_is_written_from_what_is_staged_and_not_from_what_is_not() {
        let dir = repo();
        std::fs::write(dir.path().join("a.rs"), "fn staged() {}\n").unwrap();
        std::fs::write(dir.path().join("日本語.rs"), "fn other() {}\n").unwrap();
        run(&dir, &["add", "a.rs"]);
        std::fs::write(
            dir.path().join("a.rs"),
            "fn staged() {}\nfn unstaged() {}\n",
        )
        .unwrap();
        let context = commit_context(dir.path()).await.unwrap();
        assert_eq!(context.branch, "main");
        assert!(context.staged_files.contains("M\ta.rs"), "{context:?}");
        assert!(context.staged_patch.contains("+fn staged"));
        assert!(!context.staged_patch.contains("unstaged"));
        assert!(!context.staged_files.contains("日本語"));
    }

    #[tokio::test]
    async fn nothing_staged_is_said_before_an_agent_is_asked() {
        let dir = repo();
        std::fs::write(dir.path().join("a.rs"), "changed\n").unwrap();
        let wrong = commit_context(dir.path()).await.unwrap_err().to_string();
        assert!(wrong.contains("Nothing is staged"), "{wrong}");
    }

    #[tokio::test]
    async fn a_pull_request_is_written_from_what_the_branch_has_over_its_base() {
        let dir = repo();
        run(&dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.path().join("b.rs"), "fn added() {}\n").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-qm", "core: add b\n\nBecause."]);
        let context = pull_context(dir.path()).await.unwrap();
        assert_eq!(context.branch, "feature");
        assert_eq!(context.base, "main");
        assert!(
            context.commit_summary.contains("core: add b"),
            "{context:?}"
        );
        assert!(!context.commit_summary.contains("initial"));
        assert!(context.changed_files.contains("b.rs"));
        assert!(context.patch.contains("+fn added"));
        assert_eq!(context.current_title, "core: add b");
        assert_eq!(context.current_body, "Because.");
    }

    #[tokio::test]
    async fn a_branch_with_nothing_over_its_base_is_not_written_a_pull_request() {
        let dir = repo();
        run(&dir, &["checkout", "-q", "-b", "feature"]);
        let wrong = pull_context(dir.path()).await.unwrap_err().to_string();
        assert!(wrong.contains("no commit over main"), "{wrong}");
    }
}
