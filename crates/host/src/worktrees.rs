//! Making and deleting the worktrees a window asks for, on the machine the
//! work is on.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use tsumugi_model::NewWorktree;
use tsumugi_vcs::worktree::Worktree;

/// A step run on a worktree just made, before it is handed over. One that
/// fails takes the worktree back.
type Step =
    for<'a> fn(&'a Worktree) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// What follows making a worktree, in order. A repository's own setup is not
/// here: it runs as a step of its own, which asks the person when it must.
const AFTER_CREATE: &[Step] = &[];

/// Makes the worktree `new` asks for in the repository at `repository`, and
/// returns where it is. A request that cannot be met leaves nothing behind.
pub(crate) async fn make(repository: &Path, new: &NewWorktree) -> Result<PathBuf, String> {
    make_with(repository, new, AFTER_CREATE).await
}

async fn make_with(
    repository: &Path,
    new: &NewWorktree,
    steps: &[Step],
) -> Result<PathBuf, String> {
    let made = tsumugi_vcs::worktree::create_new(repository, new).await?;
    for step in steps {
        if let Err(error) = step(&made).await {
            return Err(match tsumugi_vcs::worktree::remove(&made).await {
                Ok(()) => error,
                Err(left) => format!(
                    "{error}; the worktree made for it is left at {}: {left}",
                    made.path.display()
                ),
            });
        }
    }
    Ok(made.path)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use tsumugi_model::StartPoint;

    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn repository(dir: &Path) -> PathBuf {
        let source = dir.join("api");
        std::fs::create_dir(&source).unwrap();
        git(&source, &["init", "-q", "-b", "main"]);
        std::fs::write(source.join("a.txt"), "a").unwrap();
        git(&source, &["add", "a.txt"]);
        git(&source, &["commit", "-q", "-m", "first"]);
        source
    }

    fn named(name: &str) -> NewWorktree {
        NewWorktree {
            name: name.into(),
            start: StartPoint::Head,
            branch: None,
            checkout: false,
        }
    }

    fn fails<'a>(_: &'a Worktree) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async { Err("setup failed".to_string()) })
    }

    fn passes<'a>(
        _: &'a Worktree,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    #[tokio::test]
    async fn a_worktree_is_made_and_where_it_is_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let source = repository(dir.path());

        let path = make(&source, &named("review")).await.unwrap();

        assert!(path.join("a.txt").is_file());
    }

    #[tokio::test]
    async fn a_step_after_making_that_fails_takes_the_worktree_and_its_branch_back() {
        let dir = tempfile::tempdir().unwrap();
        let source = repository(dir.path());

        let refused = make_with(&source, &named("review"), &[passes, fails])
            .await
            .unwrap_err();

        assert_eq!(refused, "setup failed");
        assert_eq!(git(&source, &["worktree", "list"]).lines().count(), 1);
        assert_eq!(git(&source, &["branch", "--list", "review"]).trim(), "");
    }

    #[tokio::test]
    async fn a_branch_that_was_there_before_survives_a_failed_make() {
        let dir = tempfile::tempdir().unwrap();
        let source = repository(dir.path());
        git(&source, &["branch", "work"]);
        let mut asked = named("review");
        asked.branch = Some("work".into());

        let refused = make_with(&source, &asked, &[]).await.unwrap_err();

        assert!(refused.contains("already exists"), "{refused}");
        assert_eq!(git(&source, &["branch", "--list", "work"]).trim(), "work");
    }
}
