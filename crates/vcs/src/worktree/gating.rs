//! The worktree a gate runs in: one per repository, detached, put at the
//! commit a session asked to have checked and kept for the next ask.

use std::path::Path;

use super::{Result, command};

/// The full commit `dir`'s checkout is at.
pub async fn head(dir: &Path) -> Result<String> {
    let out = command(dir, "git", &["rev-parse".as_ref(), "HEAD".as_ref()]).await?;
    Ok(out.trim().to_string())
}

/// Puts the gate worktree of the repository whose primary checkout is
/// `primary` at `path` on `commit`: made detached when it is not there, else
/// moved. Files the commit tracks are reset and untracked ones that are not
/// ignored are removed, while ignored ones, a build folder among them, stay
/// for the next run.
pub async fn place_gate(primary: &Path, path: &Path, commit: &str) -> Result<()> {
    if path.join(".git").exists() {
        command(
            path,
            "git",
            &[
                "checkout".as_ref(),
                "--force".as_ref(),
                "--detach".as_ref(),
                commit.as_ref(),
            ],
        )
        .await?;
        command(path, "git", &["clean".as_ref(), "-fdq".as_ref()]).await?;
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot make {}: {error}", parent.display()))?;
    }
    // A record of a gate worktree whose directory was deleted would stop the
    // new one from using its path.
    command(primary, "git", &["worktree".as_ref(), "prune".as_ref()]).await?;
    command(
        primary,
        "git",
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            path.as_os_str(),
            commit.as_ref(),
        ],
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

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
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn repository(dir: &Path) -> std::path::PathBuf {
        let primary = std::fs::canonicalize(dir).unwrap().join("api");
        std::fs::create_dir(&primary).unwrap();
        git(&primary, &["init", "-q", "-b", "main"]);
        std::fs::write(primary.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(primary.join("a.txt"), "one").unwrap();
        git(&primary, &["add", "."]);
        git(&primary, &["commit", "-q", "-m", "first"]);
        primary
    }

    #[tokio::test]
    async fn the_gate_worktree_is_made_detached_at_the_commit_asked() {
        let dir = tempfile::tempdir().unwrap();
        let primary = repository(dir.path());
        let first = head(&primary).await.unwrap();
        std::fs::write(primary.join("a.txt"), "two").unwrap();
        git(&primary, &["commit", "-qam", "second"]);
        let gate = dir.path().join("gate").join("api");

        place_gate(&primary, &gate, &first).await.unwrap();

        assert_eq!(std::fs::read_to_string(gate.join("a.txt")).unwrap(), "one");
        assert_eq!(head(&gate).await.unwrap(), first);
        assert_eq!(git(&gate, &["branch", "--show-current"]), "");
    }

    #[tokio::test]
    async fn the_next_ask_moves_it_and_keeps_what_git_ignores_but_not_stray_files() {
        let dir = tempfile::tempdir().unwrap();
        let primary = repository(dir.path());
        let first = head(&primary).await.unwrap();
        let gate = dir.path().join("gate").join("api");
        place_gate(&primary, &gate, &first).await.unwrap();
        std::fs::create_dir(gate.join("target")).unwrap();
        std::fs::write(gate.join("target/built"), "x").unwrap();
        std::fs::write(gate.join("stray.txt"), "x").unwrap();
        std::fs::write(gate.join("a.txt"), "edited").unwrap();
        std::fs::write(primary.join("a.txt"), "two").unwrap();
        git(&primary, &["commit", "-qam", "second"]);
        let second = head(&primary).await.unwrap();

        place_gate(&primary, &gate, &second).await.unwrap();

        assert_eq!(head(&gate).await.unwrap(), second);
        assert_eq!(std::fs::read_to_string(gate.join("a.txt")).unwrap(), "two");
        assert!(gate.join("target/built").exists());
        assert!(!gate.join("stray.txt").exists());
    }

    #[tokio::test]
    async fn a_deleted_gate_directory_is_made_again() {
        let dir = tempfile::tempdir().unwrap();
        let primary = repository(dir.path());
        let first = head(&primary).await.unwrap();
        let gate = dir.path().join("gate").join("api");
        place_gate(&primary, &gate, &first).await.unwrap();
        std::fs::remove_dir_all(&gate).unwrap();

        place_gate(&primary, &gate, &first).await.unwrap();

        assert!(gate.join("a.txt").is_file());
    }
}
