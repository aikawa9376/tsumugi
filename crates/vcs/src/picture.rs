//! One version of a picture a diff compares, read as bytes.

use std::path::{Component, Path};

/// The most bytes of a picture read; a larger one is not shown.
const PICTURE_BYTES: usize = 128 * 1024 * 1024;

/// The file `path`, relative to the worktree at `worktree`, as Git has it in
/// `rev`, or as the worktree has it when `rev` is none; `None` when it is
/// not there, is not a file, or is too large.
pub async fn version(worktree: &Path, rev: Option<&str>, path: &str) -> Option<Vec<u8>> {
    // Whatever the window asks, only a file inside the worktree is read.
    if path.is_empty()
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return None;
    }
    match rev {
        None => {
            let file = worktree.join(path);
            tokio::task::spawn_blocking(move || {
                let meta = std::fs::symlink_metadata(&file).ok()?;
                (meta.is_file() && meta.len() <= PICTURE_BYTES as u64)
                    .then(|| std::fs::read(&file).ok())
                    .flatten()
            })
            .await
            .ok()
            .flatten()
        }
        // A name that starts with `-` would be taken for an option.
        Some(rev) if rev.is_empty() || rev.starts_with('-') => None,
        Some(rev) => {
            crate::review::blob_upto(worktree, &format!("{rev}:{path}"), PICTURE_BYTES).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(dir.path())
                    .env("GIT_AUTHOR_NAME", "Test")
                    .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                    .env("GIT_COMMITTER_NAME", "Test")
                    .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                    .status()
                    .unwrap()
                    .success()
            )
        };
        git(&["init", "-q"]);
        std::fs::write(dir.path().join("a.png"), b"old\0bytes").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(dir.path().join("a.png"), b"new\0bytes").unwrap();
        dir
    }

    #[tokio::test]
    async fn a_picture_is_read_as_the_worktree_has_it_and_as_a_commit_had_it() {
        let dir = repo();
        let now = version(dir.path(), None, "a.png").await;
        assert_eq!(now.as_deref(), Some(&b"new\0bytes"[..]));
        let before = version(dir.path(), Some("HEAD"), "a.png").await;
        assert_eq!(before.as_deref(), Some(&b"old\0bytes"[..]));
    }

    #[tokio::test]
    async fn a_picture_that_is_not_there_is_none_on_either_side() {
        let dir = repo();
        assert_eq!(version(dir.path(), None, "gone.png").await, None);
        assert_eq!(version(dir.path(), Some("HEAD"), "gone.png").await, None);
        assert_eq!(version(dir.path(), Some("nowhere^"), "a.png").await, None);
    }

    #[tokio::test]
    async fn only_a_file_inside_the_worktree_is_read() {
        let dir = repo();
        for path in [
            "../secret-outside.png",
            "/etc/passwd",
            "",
            ".",
            "sub/../a.png",
        ] {
            assert_eq!(version(dir.path(), None, path).await, None, "{path:?}");
        }
        assert_eq!(version(dir.path(), Some("-x"), "a.png").await, None);
        assert_eq!(version(dir.path(), Some(""), "a.png").await, None);
    }

    #[tokio::test]
    async fn a_directory_is_not_a_picture() {
        let dir = repo();
        std::fs::create_dir(dir.path().join("shots.png")).unwrap();
        assert_eq!(version(dir.path(), None, "shots.png").await, None);
    }
}
