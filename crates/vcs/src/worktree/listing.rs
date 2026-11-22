//! The worktrees a repository has.

use std::path::{Path, PathBuf};

use super::{Result, canonical, removal};

/// One worktree of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    /// The branch checked out, none for a detached head.
    pub branch: Option<String>,
    pub locked: bool,
    /// Whether it is the repository's main checkout.
    pub main: bool,
}

/// Every worktree of the repository `at` belongs to, the main checkout first.
pub async fn all(at: &Path) -> Result<Vec<Entry>> {
    let listed = removal::listed(&canonical(at)?).await?;
    Ok(listed
        .into_iter()
        .enumerate()
        .map(|(index, found)| Entry {
            path: found.path,
            branch: found.branch,
            locked: found.locked,
            main: index == 0,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git(dir: &Path, args: &[&str]) {
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
    }

    #[tokio::test]
    async fn the_main_checkout_comes_first_then_each_worktree_with_its_branch() {
        let dir = tempfile::tempdir().unwrap();
        let primary = std::fs::canonicalize(dir.path()).unwrap().join("api");
        std::fs::create_dir(&primary).unwrap();
        git(&primary, &["init", "-q", "-b", "main"]);
        std::fs::write(primary.join("a"), "a").unwrap();
        git(&primary, &["add", "."]);
        git(&primary, &["commit", "-q", "-m", "first"]);
        let made = crate::worktree::create(&primary, "review", None)
            .await
            .unwrap();
        let detached = dir.path().join("detached");
        git(
            &primary,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                detached.to_str().unwrap(),
            ],
        );

        let listed = all(&made.path).await.unwrap();

        assert_eq!(listed.len(), 3);
        assert!(listed[0].main && listed[0].path == primary);
        assert_eq!(listed[0].branch.as_deref(), Some("main"));
        assert!(!listed[1].main);
        assert!(listed[1].branch.is_some());
        assert_eq!(listed[2].branch, None);
    }
}
