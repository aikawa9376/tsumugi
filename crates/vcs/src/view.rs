//! What the details panel reads of a worktree: the files a change touches,
//! one file's diff in it, or the history.

use crate::{Error, Listing, Repo, Rev, Revision, Spot, git};

/// How much is read for a spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most lines of a diff.
    pub diff_lines: usize,
    /// The most commits or changes of the history.
    pub log_commits: usize,
}

/// What was read for a spot, and the revision whose files it lists.
#[derive(Debug)]
pub struct View {
    pub listing: Result<Listing, Error>,
    pub revision: Option<Revision>,
}

pub(crate) async fn load(repo: &Repo, spot: &Spot, limits: Limits) -> View {
    let worktree = repo.path();
    let read = match spot {
        Spot::Files(Rev::WorkingCopy) => git::changes(worktree).await.map(Listing::Files),
        Spot::Files(Rev::Commit(id)) => git::commit_files(worktree, id).await.map(Listing::Files),
        Spot::Files(Rev::Branch(since)) => {
            git::since_files(worktree, since).await.map(Listing::Files)
        }
        Spot::Diff(Rev::Branch(since), change) | Spot::Lines(Rev::Branch(since), change) => {
            git::since_diff(worktree, since, change, limits.diff_lines)
                .await
                .map(Listing::Diff)
        }
        Spot::Branch => crate::branch::branch(worktree)
            .await
            .map(|b| Listing::Branch(Box::new(b))),
        Spot::Diff(rev, change) => {
            let revision = match rev {
                Rev::WorkingCopy => None,
                Rev::Commit(id) => Some(id.as_str()),
                Rev::Branch(_) => unreachable!("a branch diff is read above"),
            };
            crate::review::file_diff(worktree, revision, change, limits.diff_lines)
                .await
                .map(Listing::Diff)
        }
        // Review comments are pinned to line numbers, which only the line
        // diff has, never difftastic's text.
        Spot::Lines(rev, change) => {
            let revision = match rev {
                Rev::WorkingCopy => None,
                Rev::Commit(id) => Some(id.as_str()),
                Rev::Branch(_) => unreachable!("a branch diff is read above"),
            };
            git::file_diff(worktree, revision, change, limits.diff_lines)
                .await
                .map(Listing::Diff)
        }
        Spot::Log => git::history(worktree, limits.log_commits)
            .await
            .map(Listing::Log),
    };
    let revision = if read.is_ok() {
        match spot {
            Spot::Files(rev) => {
                let target = match rev {
                    Rev::WorkingCopy => "HEAD",
                    Rev::Commit(id) => id,
                    Rev::Branch(_) => "HEAD",
                };
                git::revision(worktree, target).await.ok()
            }
            _ => None,
        }
    } else {
        None
    };
    View {
        listing: read,
        revision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: Limits = Limits {
        diff_lines: 2000,
        log_commits: 50,
    };

    #[tokio::test]
    async fn changes_and_opened_log_entries_show_their_full_descriptions_even_without_files() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .output()
                .unwrap()
                .status
                .success()
        };
        assert!(git(&["init", "-q"]));
        std::fs::create_dir(dir.path().join(".jj")).unwrap();
        assert!(git(&[
            "commit",
            "--allow-empty",
            "-m",
            "first subject",
            "-m",
            "The reason for this change."
        ]));
        let repo = Repo::open(dir.path());
        let changes = repo.load(&Spot::Files(Rev::WorkingCopy), LIMITS).await;
        assert!(matches!(changes.listing, Ok(Listing::Files(ref files)) if files.is_empty()));
        let first = changes.revision.unwrap();
        assert_eq!(
            first.description,
            "first subject\n\nThe reason for this change."
        );
        assert!(git(&["commit", "--allow-empty", "-m", "second subject"]));
        let opened = repo
            .load(&Spot::Files(Rev::Commit(first.id.clone())), LIMITS)
            .await;
        assert_eq!(opened.revision, Some(first));
        let changes = repo.load(&Spot::Files(Rev::WorkingCopy), LIMITS).await;
        assert_eq!(changes.revision.unwrap().description, "second subject");
        assert!(repo.load(&Spot::Log, LIMITS).await.revision.is_none());
    }

    #[tokio::test]
    async fn an_unborn_git_worktree_lists_its_files_without_a_commit_description() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(dir.path().join("new.txt"), "new").unwrap();
        let view = Repo::open(dir.path())
            .load(&Spot::Files(Rev::WorkingCopy), LIMITS)
            .await;
        assert!(matches!(view.listing, Ok(Listing::Files(ref files)) if files.len() == 1));
        assert!(view.revision.is_none());
    }

    #[tokio::test]
    async fn the_lines_of_a_change_are_the_line_diff_whatever_the_structural_one_reads() {
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
        std::fs::write(dir.path().join("メモ.txt"), "one\ntwo\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(dir.path().join("メモ.txt"), "one\nTWO\n").unwrap();
        let change = tsumugi_model::repository::Change {
            status: " M".into(),
            path: "メモ.txt".into(),
            from: None,
        };
        let view = Repo::open(dir.path())
            .load(&Spot::Lines(Rev::WorkingCopy, change), LIMITS)
            .await;
        let Ok(Listing::Diff(lines)) = view.listing else {
            panic!("no diff: {:?}", view.listing);
        };
        let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&tsumugi_model::repository::DiffKind::Hunk));
        assert!(lines.iter().any(|l| l.text == "+TWO"));
        assert!(!kinds.contains(&tsumugi_model::repository::DiffKind::Difft));
    }
}
