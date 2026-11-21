//! Opening a pull request for the branch a worktree has checked out: the
//! branch is pushed, then `gh` creates the pull request, so the login stays
//! `gh`'s.

use std::path::Path;

use serde::Deserialize;

use crate::git::git;
use crate::{Error, run, write};

/// A pull request to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPull {
    /// The branch it merges into; the repository's default branch when none.
    pub base: Option<String>,
    pub title: String,
    pub body: String,
    pub draft: bool,
}

/// A pull request that was opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub number: u64,
    pub url: String,
}

/// Push the branch the worktree at `path` has checked out and open a pull
/// request for it. It fails, with nothing created, when `gh` is missing or
/// logged out, when the branch has a pull request open already, and when the
/// push is refused.
pub async fn create(path: &Path, new: &NewPull) -> Result<Created, Error> {
    create_with("gh", path, new).await
}

/// [`create`], running `gh` as the program `gh`.
async fn create_with(gh: &str, path: &Path, new: &NewPull) -> Result<Created, Error> {
    let failed = |stderr: String| Error::Failed {
        program: "gh".into(),
        stderr,
    };
    if new.title.trim().is_empty() {
        return Err(failed("the pull request title is empty".into()));
    }
    match run(gh, &["auth", "status"], Some(path)).await {
        Ok(_) => {}
        Err(Error::Spawn { .. }) => {
            return Err(failed(
                "gh is not installed on the machine that holds the worktree".into(),
            ));
        }
        Err(Error::Failed { .. }) => {
            return Err(failed(
                "gh is not logged in to GitHub; run `gh auth login` on the machine that holds the worktree".into(),
            ));
        }
    }
    let branch = git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .await
        .map(|b| b.trim().to_string())
        .map_err(|_| failed("HEAD is not on a branch; check one out first".into()))?;
    if let Some(base) = &new.base
        && base.trim_start_matches("origin/") == branch
    {
        return Err(failed(format!(
            "the base is {branch}, the branch itself; choose another base"
        )));
    }
    let listed = run(
        gh,
        &[
            "pr",
            "list",
            "--head",
            &branch,
            "--state",
            "open",
            "--limit",
            "1",
            "--json",
            "number,url",
        ],
        Some(path),
    )
    .await
    .map_err(|e| failed(explain(&e)))?;
    if let Some(open) = open_pull(&listed) {
        return Err(failed(format!(
            "{branch} has a pull request open already: #{} {}",
            open.number, open.url
        )));
    }
    write::push(path, false).await.map_err(|e| match e {
        Error::Failed { stderr, .. } => Error::Failed {
            program: "git".into(),
            stderr: format!("push failed, no pull request was created: {stderr}"),
        },
        e => e,
    })?;
    let args = create_args(&branch, new);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run(gh, &args, Some(path))
        .await
        .map_err(|e| failed(format!("the branch was pushed, but {}", explain(&e))))?;
    created(&out).ok_or_else(|| {
        failed(format!(
            "the branch was pushed, but gh printed no pull request address: {}",
            out.trim()
        ))
    })
}

/// The arguments of `gh pr create`. A value is joined to its flag with `=`,
/// so a title or a body that starts with `-` is not read as a flag.
fn create_args(branch: &str, new: &NewPull) -> Vec<String> {
    let mut args: Vec<String> = ["pr", "create", "--head", branch]
        .into_iter()
        .map(String::from)
        .collect();
    if let Some(base) = new.base.as_deref().filter(|b| !b.trim().is_empty()) {
        args.push(format!("--base={}", base.trim()));
    }
    args.push(format!("--title={}", new.title.trim()));
    args.push(format!("--body={}", new.body));
    if new.draft {
        args.push("--draft".into());
    }
    args
}

/// Why `gh` failed, from its own words.
fn explain(error: &Error) -> String {
    match error {
        Error::Failed { stderr, .. } => stderr
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("; "),
        e => e.to_string(),
    }
}

/// The pull request `gh pr list --json number,url` printed first, if any.
fn open_pull(out: &str) -> Option<Created> {
    #[derive(Deserialize)]
    struct Listed {
        number: u64,
        url: String,
    }
    let listed: Vec<Listed> = serde_json::from_str(out).ok()?;
    listed.into_iter().next().map(|l| Created {
        number: l.number,
        url: l.url,
    })
}

/// The pull request `gh pr create` printed the address of: its last line.
fn created(out: &str) -> Option<Created> {
    let url = out.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let number = url
        .rsplit_once("/pull/")?
        .1
        .trim_end_matches('/')
        .parse()
        .ok()?;
    Some(Created {
        number,
        url: url.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new() -> NewPull {
        NewPull {
            base: Some("main".into()),
            title: " core: add it ".into(),
            body: "Why.\n\n- one".into(),
            draft: true,
        }
    }

    #[test]
    fn the_arguments_join_values_to_their_flags_so_a_dash_is_not_a_flag() {
        let args = create_args(
            "feature",
            &NewPull {
                title: "-x".into(),
                body: "--draft".into(),
                draft: false,
                ..new()
            },
        );
        assert_eq!(
            args,
            [
                "pr",
                "create",
                "--head",
                "feature",
                "--base=main",
                "--title=-x",
                "--body=--draft"
            ]
        );
    }

    #[test]
    fn a_draft_is_asked_for_and_a_blank_base_is_the_default_branch() {
        let args = create_args(
            "feature",
            &NewPull {
                base: Some("  ".into()),
                ..new()
            },
        );
        assert!(args.contains(&"--draft".to_string()));
        assert!(args.contains(&"--title=core: add it".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("--base")));
    }

    #[test]
    fn the_pull_request_created_is_read_from_the_last_line() {
        assert_eq!(
            created("\nCreating pull request\nhttps://github.com/o/r/pull/12\n"),
            Some(Created {
                number: 12,
                url: "https://github.com/o/r/pull/12".into()
            })
        );
        assert_eq!(created("no address"), None);
        assert_eq!(created(""), None);
    }

    #[test]
    fn a_pull_request_open_already_is_read_from_the_listing() {
        assert_eq!(open_pull("[]"), None);
        assert_eq!(
            open_pull(r#"[{"number":3,"url":"https://github.com/o/r/pull/3"}]"#),
            Some(Created {
                number: 3,
                url: "https://github.com/o/r/pull/3".into()
            })
        );
    }

    #[cfg(unix)]
    mod with_a_fake_gh {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn git_in(dir: &Path, args: &[&str]) {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=a", "-c", "user.email=a@b"])
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        }

        /// A worktree on branch `feature` whose `origin` is a bare repository.
        fn worktree() -> (tempfile::TempDir, std::path::PathBuf) {
            let dir = tempfile::tempdir().unwrap();
            let remote = dir.path().join("remote.git");
            let work = dir.path().join("work");
            std::fs::create_dir_all(&work).unwrap();
            git_in(dir.path(), &["init", "-q", "--bare", "remote.git"]);
            git_in(&work, &["init", "-q", "-b", "feature"]);
            std::fs::write(work.join("a"), "a").unwrap();
            git_in(&work, &["add", "a"]);
            git_in(&work, &["commit", "-q", "-m", "a"]);
            git_in(
                &work,
                &["remote", "add", "origin", remote.to_str().unwrap()],
            );
            (dir, work)
        }

        /// A `gh` that answers the way `script` says; `log` collects its calls.
        fn gh(dir: &Path, script: &str) -> String {
            let path = dir.join("gh");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho \"$@\" >> \"{}/calls\"\n{script}\n",
                    dir.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path.to_str().unwrap().into()
        }

        const HAPPY: &str = r#"case "$1 $2" in
  "auth status") exit 0;;
  "pr list") echo '[]';;
  "pr create") echo "https://github.com/o/r/pull/9";;
esac"#;

        fn calls(dir: &Path) -> String {
            std::fs::read_to_string(dir.join("calls")).unwrap_or_default()
        }

        #[tokio::test]
        async fn the_branch_is_pushed_and_then_the_pull_request_is_created() {
            let (dir, work) = worktree();
            let gh = gh(dir.path(), HAPPY);
            let made = create_with(&gh, &work, &new()).await.unwrap();
            assert_eq!(made.number, 9);
            let pushed = git_in_out(&dir.path().join("remote.git"), &["branch", "--list"]);
            assert!(pushed.contains("feature"), "{pushed}");
            assert!(calls(dir.path()).contains("pr create --head feature --base=main"));
        }

        fn git_in_out(dir: &Path, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).into()
        }

        #[tokio::test]
        async fn a_gh_that_is_not_logged_in_fails_before_anything_is_pushed() {
            let (dir, work) = worktree();
            let gh = gh(dir.path(), "exit 1");
            let error = create_with(&gh, &work, &new()).await.unwrap_err();
            assert!(error.to_string().contains("gh auth login"), "{error}");
            assert_eq!(git_in_out(&dir.path().join("remote.git"), &["branch"]), "");
        }

        #[tokio::test]
        async fn a_missing_gh_says_it_is_not_installed() {
            let (dir, work) = worktree();
            let missing = dir.path().join("none").to_str().unwrap().to_string();
            let error = create_with(&missing, &work, &new()).await.unwrap_err();
            assert!(error.to_string().contains("not installed"), "{error}");
        }

        #[tokio::test]
        async fn a_branch_with_a_pull_request_open_is_refused_and_not_pushed() {
            let (dir, work) = worktree();
            let gh = gh(
                dir.path(),
                r#"case "$1 $2" in
  "pr list") echo '[{"number":4,"url":"https://github.com/o/r/pull/4"}]';;
esac"#,
            );
            let error = create_with(&gh, &work, &new()).await.unwrap_err();
            assert!(error.to_string().contains("open already: #4"), "{error}");
            assert_eq!(git_in_out(&dir.path().join("remote.git"), &["branch"]), "");
            assert!(!calls(dir.path()).contains("pr create"));
        }

        #[tokio::test]
        async fn a_refused_push_creates_nothing_and_says_so() {
            let (dir, work) = worktree();
            git_in(
                &work,
                &["remote", "set-url", "origin", "/nonexistent/remote.git"],
            );
            let gh = gh(dir.path(), HAPPY);
            let error = create_with(&gh, &work, &new()).await.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("push failed, no pull request was created"),
                "{error}"
            );
            assert!(!calls(dir.path()).contains("pr create"));
        }

        #[tokio::test]
        async fn a_pull_request_gh_refused_says_the_branch_was_pushed() {
            let (dir, work) = worktree();
            let gh = gh(
                dir.path(),
                r#"case "$1 $2" in
  "pr list") echo '[]';;
  "pr create") echo "GraphQL: no commits between main and feature" >&2; exit 1;;
esac"#,
            );
            let error = create_with(&gh, &work, &new()).await.unwrap_err();
            let text = error.to_string();
            assert!(text.contains("was pushed"), "{text}");
            assert!(text.contains("no commits between"), "{text}");
        }

        #[tokio::test]
        async fn an_empty_title_or_the_branch_as_its_own_base_is_refused() {
            let (dir, work) = worktree();
            let gh = gh(dir.path(), HAPPY);
            let empty = NewPull {
                title: "  ".into(),
                ..new()
            };
            assert!(create_with(&gh, &work, &empty).await.is_err());
            let same = NewPull {
                base: Some("feature".into()),
                ..new()
            };
            let error = create_with(&gh, &work, &same).await.unwrap_err();
            assert!(error.to_string().contains("the branch itself"), "{error}");
            assert!(!calls(dir.path()).contains("pr create"));
        }
    }
}
