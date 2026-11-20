//! New Git worktrees for sessions to work in.
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use crate::Backend;

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The full commit the worktree starts from.
    pub base: String,
    root: PathBuf,
    /// The branch git made for it, which goes with it.
    branch: Option<String>,
}

/// Makes a worktree named `name` of the repository holding `source`, starting
/// from `base`, or from `source`'s current commit when none is given.
pub async fn create(source: &Path, name: &str, base: Option<&str>) -> Result<Worktree> {
    let spot = Spot::find(source, name).await?;
    let base = revision(&spot.source, base.unwrap_or("HEAD")).await?;
    let native = spot.native.clone();
    spot.git(&[
        "-b".as_ref(),
        native.as_ref(),
        spot.path.as_os_str(),
        base.as_ref(),
    ])
    .await?;
    Ok(spot.with(base, Some(native)))
}

/// Makes a worktree of the repository holding `source` on its branch
/// `branch`, as `remote` has it: checks the branch out there, made to
/// track `remote`'s when there is none yet.
pub async fn create_on_branch(source: &Path, branch: &str, remote: &str) -> Result<Worktree> {
    let spot = Spot::find(source, branch).await?;
    let tracked = format!("{remote}/{branch}");
    let local = format!("refs/heads/{branch}");
    let base = revision(&spot.source, &tracked).await?;
    let has = command(
        &spot.root,
        "git",
        &[
            "show-ref".as_ref(),
            "--verify".as_ref(),
            "--quiet".as_ref(),
            local.as_ref(),
        ],
    )
    .await
    .is_ok();
    if has {
        spot.git(&[spot.path.as_os_str(), branch.as_ref()]).await?;
        Ok(spot.with(base, None))
    } else {
        spot.git(&[
            "--track".as_ref(),
            "-b".as_ref(),
            branch.as_ref(),
            spot.path.as_os_str(),
            tracked.as_ref(),
        ])
        .await?;
        Ok(spot.with(base, Some(branch.to_string())))
    }
}

/// Where a new worktree goes: a free directory named after it beside the
/// repository's others.
struct Spot {
    source: PathBuf,
    root: PathBuf,
    path: PathBuf,
    native: String,
}

impl Spot {
    async fn find(source: &Path, name: &str) -> Result<Self> {
        let source = canonical(source)?;
        let (root, repository) = identify(&source).await?;
        let slug = slug(name);
        let parent = destination(&base_directory(&root, &repository).await?)?;
        let path = (1..)
            .map(|n| match n {
                1 => parent.join(&slug),
                n => parent.join(format!("{slug}-{n}")),
            })
            .map(|path| exists(&path).map(|taken| (!taken).then_some(path)))
            .find_map(Result::transpose)
            .expect("an unbounded search finds a free name")?;
        let native = path
            .file_name()
            .expect("a joined name")
            .to_string_lossy()
            .into_owned();
        std::fs::create_dir_all(&parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        Ok(Self {
            source,
            root,
            path,
            native,
        })
    }

    async fn git(&self, args: &[&OsStr]) -> Result<()> {
        let all: Vec<&OsStr> = ["worktree".as_ref(), "add".as_ref()]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        command(&self.root, "git", &all).await?;
        Ok(())
    }

    fn with(self, base: String, branch: Option<String>) -> Worktree {
        Worktree {
            path: self.path,
            base,
            root: self.root,
            branch,
        }
    }
}

/// Undoes `create` for a worktree nothing has worked in yet: the directory,
/// the worktree, and the branch made for it.
pub async fn remove(worktree: &Worktree) -> Result<()> {
    command(
        &worktree.root,
        "git",
        &[
            "worktree".as_ref(),
            "remove".as_ref(),
            "--force".as_ref(),
            worktree.path.as_os_str(),
        ],
    )
    .await?;
    if let Some(branch) = &worktree.branch {
        command(
            &worktree.root,
            "git",
            &["branch".as_ref(), "-D".as_ref(), branch.as_ref()],
        )
        .await?;
    }

    Ok(())
}

/// Lower-case words joined by dashes, so a session's name can name a directory.
fn slug(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() {
        "session".into()
    } else {
        words.join("-")
    }
}

async fn identify(source: &Path) -> Result<(PathBuf, PathBuf)> {
    match Backend::of(source) {
        Some(Backend::Git) => {
            let reported = command(
                source,
                "git",
                &[
                    "rev-parse".as_ref(),
                    "--path-format=absolute".as_ref(),
                    "--show-toplevel".as_ref(),
                    "--git-common-dir".as_ref(),
                ],
            )
            .await?;
            let lines: Vec<_> = reported.lines().collect();
            let [top, common] = lines.as_slice() else {
                return Err("Git did not report its worktree and repository".into());
            };
            Ok((canonical(Path::new(top))?, canonical(Path::new(common))?))
        }
        None => Err(format!(
            "{} is not under version control, so no worktree can be made of it",
            source.display()
        )),
    }
}

async fn output(dir: &Path, program: &str, args: &[&OsStr]) -> Result<std::process::Output> {
    let mut process = tokio::process::Command::new(program);
    process
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
    ] {
        process.env_remove(key);
    }
    tokio::time::timeout(Duration::from_secs(30), process.output())
        .await
        .map_err(|_| format!("{program} timed out"))?
        .map_err(|error| format!("cannot start {program}: {error}"))
}

async fn command(dir: &Path, program: &str, args: &[&OsStr]) -> Result<String> {
    let output = output(dir, program, args).await?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{program} returned invalid UTF-8"))
}

/// Where new worktrees go: Git's `wt.basedir` when set, otherwise a sibling
/// of the repository named after it.
async fn base_directory(root: &Path, repository: &Path) -> Result<PathBuf> {
    let anchor = (repository.file_name().is_some_and(|name| name == ".git"))
        .then(|| repository.parent())
        .flatten()
        .unwrap_or(root);
    if exists(&root.join(".git"))? {
        let configured = output(
            root,
            "git",
            &[
                "config".as_ref(),
                "--path".as_ref(),
                "--get".as_ref(),
                "wt.basedir".as_ref(),
            ],
        )
        .await?;
        if configured.status.success() {
            let configured = String::from_utf8(configured.stdout)
                .map_err(|_| "the configured worktree base is not UTF-8")?;
            let configured = configured.trim_end_matches('\n');
            if configured.is_empty() || configured.lines().count() != 1 {
                return Err("the configured worktree base must be one nonempty path".into());
            }
            let configured = Path::new(configured);
            return Ok(if configured.is_absolute() {
                configured.into()
            } else {
                anchor.join(configured)
            });
        }
        if configured.status.code() != Some(1) {
            return Err("cannot read the configured worktree base".into());
        }
    }
    Ok(anchor
        .parent()
        .ok_or("the repository has no parent directory")?
        .join(format!(
            "{}-wt",
            anchor.file_name().unwrap_or_default().to_string_lossy()
        )))
}

async fn revision(dir: &Path, requested: &str) -> Result<String> {
    let output = command(
        dir,
        "git",
        &[
            "rev-parse".as_ref(),
            "--verify".as_ref(),
            format!("{requested}^{{commit}}").as_ref(),
        ],
    )
    .await?;
    let values: Vec<_> = output.lines().collect();
    match values.as_slice() {
        [value]
            if matches!(value.len(), 40 | 64)
                && value.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            Ok((*value).into())
        }
        _ => Err(format!("{requested} does not name exactly one revision")),
    }
}

fn exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn canonical(path: &Path) -> Result<PathBuf> {
    tsumugi_sys::fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))
}

/// `path` with its existing part resolved, refused inside VCS metadata.
fn destination(path: &Path) -> Result<PathBuf> {
    let inside_metadata = |path: &Path| {
        path.components()
            .any(|part| matches!(part, Component::Normal(name) if name == ".git" || name == ".jj"))
    };
    if !path.is_absolute() || inside_metadata(path) {
        return Err(format!("{} cannot hold worktrees", path.display()));
    }
    let mut parent = path;
    let mut suffix = Vec::new();
    while !exists(parent)? {
        suffix.push(
            parent
                .file_name()
                .ok_or("the destination has no name")?
                .to_owned(),
        );
        parent = parent.parent().ok_or("the destination has no parent")?;
    }
    let mut resolved = canonical(parent)?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    if inside_metadata(&resolved) {
        return Err(format!("{} cannot hold worktrees", resolved.display()));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn run(dir: &Path, program: &str, args: &[&str]) -> String {
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn git_repository(dir: &Path) -> PathBuf {
        let source = dir.join("api");
        std::fs::create_dir(&source).unwrap();
        run(&source, "git", &["init", "-q", "-b", "main"]);
        std::fs::write(source.join("a.txt"), "a").unwrap();
        run(&source, "git", &["add", "a.txt"]);
        run(&source, "git", &["commit", "-q", "-m", "first"]);
        source
    }

    #[tokio::test]
    async fn a_git_repository_gets_a_worktree_where_git_keeps_them_named_after_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        run(&source, "git", &["config", "wt.basedir", "../trees"]);
        let head = run(&source, "git", &["rev-parse", "HEAD"]);

        let made = create(&source, "Fix the flaky login test", None)
            .await
            .unwrap();

        let trees = tsumugi_sys::fs::canonicalize(dir.path())
            .unwrap()
            .join("trees");
        assert_eq!(made.path, trees.join("fix-the-flaky-login-test"));
        assert_eq!(made.base, head.trim());
        assert!(made.path.join("a.txt").is_file());
    }

    #[tokio::test]
    async fn a_second_worktree_of_the_same_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());

        let first = create(&source, "review", None).await.unwrap();
        let second = create(&source, "review", None).await.unwrap();

        assert_ne!(first.path, second.path);
        assert!(second.path.ends_with("review-2"));
    }

    #[tokio::test]
    async fn colocated_metadata_does_not_change_git_worktree_creation_or_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        std::fs::create_dir(source.join(".jj")).unwrap();
        let head = run(&source, "git", &["rev-parse", "HEAD"]);

        let made = create(&source, "review", None).await.unwrap();

        assert_eq!(made.base, head.trim());
        assert!(made.path.join(".git").is_file());
        remove(&made).await.unwrap();
        assert!(!made.path.exists());
        assert!(
            run(&source, "git", &["branch", "--list", "review"])
                .trim()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_worktree_starts_from_the_revision_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let first = run(&source, "git", &["rev-parse", "HEAD"]);
        std::fs::write(source.join("b.txt"), "b").unwrap();
        run(&source, "git", &["add", "b.txt"]);
        run(&source, "git", &["commit", "-q", "-m", "second"]);

        let made = create(&source, "continue", Some(first.trim()))
            .await
            .unwrap();

        assert_eq!(made.base, first.trim());
        assert!(!made.path.join("b.txt").exists());
    }

    #[tokio::test]
    async fn a_git_worktree_taken_back_leaves_no_directory_worktree_or_branch() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create(&source, "review", None).await.unwrap();

        remove(&made).await.unwrap();

        assert!(!made.path.exists());
        let worktrees = run(&source, "git", &["worktree", "list"]);
        assert!(!worktrees.contains("review"), "{worktrees}");
        let branches = run(&source, "git", &["branch", "--list", "review"]);
        assert!(branches.trim().is_empty(), "{branches}");
    }

    #[tokio::test]
    async fn a_directory_outside_version_control_gets_no_worktree() {
        let dir = tempfile::tempdir().unwrap();

        let refused = create(dir.path(), "anything", None).await.unwrap_err();

        assert!(refused.contains("not under version control"), "{refused}");
    }

    #[test]
    fn a_name_in_words_becomes_a_directory_name() {
        assert_eq!(slug("Review the API!"), "review-the-api");
        assert_eq!(slug("ログイン修正"), "session");
    }
}
