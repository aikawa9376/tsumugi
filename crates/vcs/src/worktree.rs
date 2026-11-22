//! New Git worktrees for sessions to work in.
use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tsumugi_model::{NewWorktree, StartPoint};

use crate::Backend;

mod gating;
mod listing;
mod placing;
mod removal;
mod starts;

pub use gating::{head, place_gate};
pub use listing::{Entry, all};
pub use placing::{included, place, relative};
pub use removal::{delete, standing};
pub use starts::starts;

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
    let start = base.map_or(StartPoint::Head, |rev| StartPoint::Rev { rev: rev.into() });
    create_new(
        source,
        &NewWorktree {
            name: name.into(),
            start,
            branch: None,
        },
    )
    .await
}

/// Makes the worktree `new` asks for in the repository holding `source`. A
/// request that cannot be met, for a branch that exists, a start that is no
/// revision or a remote that cannot be fetched, makes nothing: git makes
/// the directory and the branch together or neither.
pub async fn create_new(source: &Path, new: &NewWorktree) -> Result<Worktree> {
    if let Some(branch) = &new.branch {
        valid_branch(source, branch).await?;
    }
    let spot = Spot::find(
        source,
        &new.name,
        new.branch.as_deref().map_or(Branch::Derived, Branch::New),
    )
    .await?;
    if let StartPoint::Remote { remote, branch } = &new.start {
        fetch(&spot.root, remote, branch).await?;
    }
    let base = revision(&spot.source, &new.start.name()).await?;
    let branch = spot.branch.clone();
    let mut args: Vec<&OsStr> = Vec::new();
    // A branch made from a remote's would otherwise push to it.
    if matches!(new.start, StartPoint::Remote { .. }) {
        args.push("--no-track".as_ref());
    }
    args.extend([
        "-b".as_ref(),
        branch.as_ref(),
        spot.path.as_os_str(),
        base.as_ref(),
    ]);
    spot.git(&args).await?;
    Ok(spot.with(base, Some(branch)))
}

/// `branch` as a name git can give a branch.
async fn valid_branch(source: &Path, branch: &str) -> Result<()> {
    let checked = output(
        source,
        "git",
        &[
            "check-ref-format".as_ref(),
            "--branch".as_ref(),
            branch.as_ref(),
        ],
    )
    .await?;
    // `--branch` also understands `@{-1}`, which names no new branch.
    if !checked.status.success() || branch.starts_with('@') || branch.starts_with('-') {
        return Err(format!("{branch} is not a valid branch name"));
    }
    Ok(())
}

/// `branch` of `remote` fetched into the repository at `root`, so a worktree
/// starts where the remote is now.
async fn fetch(root: &Path, remote: &str, branch: &str) -> Result<()> {
    let spec = format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}");
    command(
        root,
        "git",
        &[
            "fetch".as_ref(),
            "--quiet".as_ref(),
            remote.as_ref(),
            spec.as_ref(),
        ],
    )
    .await
    .map(drop)
    .map_err(|error| format!("cannot fetch {branch} from {remote}: {error}"))
}

async fn branch_exists(root: &Path, branch: &str) -> bool {
    let local = format!("refs/heads/{branch}");
    command(
        root,
        "git",
        &[
            "show-ref".as_ref(),
            "--verify".as_ref(),
            "--quiet".as_ref(),
            local.as_ref(),
        ],
    )
    .await
    .is_ok()
}

/// Makes a worktree of the repository holding `source` on its branch
/// `branch`, as `remote` has it: checks the branch out there, made to
/// track `remote`'s when there is none yet.
pub async fn create_on_branch(source: &Path, branch: &str, remote: &str) -> Result<Worktree> {
    let spot = Spot::find(source, branch, Branch::Existing(branch)).await?;
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

/// The branch a worktree is made on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Branch<'a> {
    /// One named after its directory, numbered along with it when a branch
    /// has the name.
    Derived,
    /// A new one by this name, which no branch may have.
    New(&'a str),
    /// One that is there, which the worktree checks out.
    Existing(&'a str),
}

/// Where a new worktree goes: a free directory named after it beside the
/// repository's others.
struct Spot {
    source: PathBuf,
    root: PathBuf,
    path: PathBuf,
    /// The branch it is made on.
    branch: String,
}

impl Spot {
    /// A free place for the worktree `name`, with the branch `branch` says.
    async fn find(source: &Path, name: &str, branch: Branch<'_>) -> Result<Self> {
        let source = canonical(source)?;
        let (root, repository) = identify(&source).await?;
        if let Branch::New(branch) = branch
            && branch_exists(&root, branch).await
        {
            return Err(format!("the branch {branch} already exists"));
        }
        let slug = slug(name);
        let parent = destination(&base_directory(&root, &repository).await?)?;
        let mut n = 1;
        let (path, native) = loop {
            let native = match n {
                1 => slug.clone(),
                n => format!("{slug}-{n}"),
            };
            let path = parent.join(&native);
            let free = !exists(&path)?
                && (branch != Branch::Derived || !branch_exists(&root, &native).await);
            if free {
                break (path, native);
            }
            n += 1;
        };
        std::fs::create_dir_all(&parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        Ok(Self {
            source,
            root,
            path,
            branch: match branch {
                Branch::New(branch) | Branch::Existing(branch) => branch.to_string(),
                Branch::Derived => native,
            },
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

/// The primary checkout of the repository holding `source`: the directory
/// holding its `.git`, which every worktree of it was made from. A bare
/// repository has none, and `source`'s own top directory stands for it.
pub async fn primary(source: &Path) -> Result<PathBuf> {
    let (top, common) = identify(&canonical(source)?).await?;
    Ok(match common.file_name() {
        Some(name) if name == ".git" => common.parent().map_or(top, Path::to_path_buf),
        _ => top,
    })
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
    use tsumugi_model::{BranchFate, NewWorktree, StartPoint, WorktreeRemoved, WorktreeStanding};

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

    fn commit_file(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), name).unwrap();
        run(dir, "git", &["add", name]);
        run(dir, "git", &["commit", "-q", "-m", name]);
    }

    fn branch_list(source: &Path) -> String {
        run(source, "git", &["branch", "--format=%(refname:short)"])
    }

    fn worktree_count(source: &Path) -> usize {
        run(source, "git", &["worktree", "list"]).lines().count()
    }

    fn new(name: &str) -> NewWorktree {
        NewWorktree {
            name: name.into(),
            ..NewWorktree::default()
        }
    }

    #[tokio::test]
    async fn a_branch_named_for_the_worktree_is_made_for_it() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let mut asked = new("Fix login");
        asked.branch = Some("feat/login".into());

        let made = create_new(&source, &asked).await.unwrap();

        assert!(made.path.ends_with("fix-login"));
        assert!(branch_list(&source).lines().any(|b| b == "feat/login"));
        let head = run(&made.path, "git", &["branch", "--show-current"]);
        assert_eq!(head.trim(), "feat/login");
    }

    #[tokio::test]
    async fn a_branch_that_exists_is_refused_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        run(&source, "git", &["branch", "taken"]);
        let mut asked = new("review");
        asked.branch = Some("taken".into());

        let refused = create_new(&source, &asked).await.unwrap_err();

        assert!(refused.contains("already exists"), "{refused}");
        assert_eq!(worktree_count(&source), 1);
        let parent = tsumugi_sys::fs::canonicalize(dir.path()).unwrap();
        assert!(!parent.join("api-wt").join("review").exists());
    }

    #[tokio::test]
    async fn a_branch_name_git_cannot_hold_is_refused_before_anything_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        for bad in ["a b", "x..y", "-dash", "ends.lock", "/lead"] {
            let mut asked = new("review");
            asked.branch = Some(bad.into());
            let refused = create_new(&source, &asked).await.unwrap_err();
            assert!(
                refused.contains("not a valid branch name"),
                "{bad}: {refused}"
            );
        }
        assert_eq!(worktree_count(&source), 1);
    }

    #[tokio::test]
    async fn the_branch_made_for_a_name_is_numbered_when_a_branch_has_it_already() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        run(&source, "git", &["branch", "review"]);

        let made = create_new(&source, &new("review")).await.unwrap();

        assert!(made.path.ends_with("review-2"));
        let head = run(&made.path, "git", &["branch", "--show-current"]);
        assert_eq!(head.trim(), "review-2");
    }

    #[tokio::test]
    async fn a_local_branch_is_where_a_worktree_starts_when_it_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        run(&source, "git", &["branch", "older"]);
        commit_file(&source, "b.txt");
        let mut asked = new("from-older");
        asked.start = StartPoint::Local {
            branch: "older".into(),
        };

        let made = create_new(&source, &asked).await.unwrap();

        assert!(!made.path.join("b.txt").exists());
        assert!(made.path.join("a.txt").exists());
    }

    /// A clone of a repository `origin`, which has `feature` one commit
    /// ahead of what the clone knows.
    fn cloned_with_newer_remote_branch(dir: &Path) -> (PathBuf, PathBuf) {
        let origin = git_repository(dir);
        run(&origin, "git", &["branch", "feature"]);
        let clone = dir.join("clone");
        run(
            dir,
            "git",
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        run(&origin, "git", &["checkout", "-q", "feature"]);
        commit_file(&origin, "feature.txt");
        run(&origin, "git", &["checkout", "-q", "main"]);
        (origin, clone)
    }

    #[tokio::test]
    async fn a_remote_branch_is_fetched_before_a_worktree_starts_there_and_is_not_tracked() {
        let dir = tempfile::tempdir().unwrap();
        let (origin, clone) = cloned_with_newer_remote_branch(dir.path());
        let tip = run(&origin, "git", &["rev-parse", "feature"]);
        let mut asked = new("try-feature");
        asked.start = StartPoint::Remote {
            remote: "origin".into(),
            branch: "feature".into(),
        };

        let made = create_new(&clone, &asked).await.unwrap();

        assert_eq!(made.base, tip.trim());
        assert!(made.path.join("feature.txt").exists());
        let upstream = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "try-feature@{upstream}"])
            .current_dir(&made.path)
            .output()
            .unwrap();
        assert!(
            !upstream.status.success(),
            "it must not push to the remote's branch"
        );
    }

    #[tokio::test]
    async fn a_remote_branch_that_cannot_be_fetched_makes_no_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let mut asked = new("nowhere");
        asked.start = StartPoint::Remote {
            remote: "origin".into(),
            branch: "feature".into(),
        };

        let refused = create_new(&source, &asked).await.unwrap_err();

        assert!(refused.contains("fetch"), "{refused}");
        assert_eq!(worktree_count(&source), 1);
        assert_eq!(branch_list(&source).trim(), "main");
    }

    #[tokio::test]
    async fn a_start_that_is_no_revision_makes_no_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let mut asked = new("nowhere");
        asked.start = StartPoint::Rev {
            rev: "not-a-commit".into(),
        };

        assert!(create_new(&source, &asked).await.is_err());
        assert_eq!(worktree_count(&source), 1);
        assert_eq!(branch_list(&source).trim(), "main");
    }

    #[tokio::test]
    async fn the_places_a_worktree_may_start_list_head_then_local_then_remote_branches() {
        let dir = tempfile::tempdir().unwrap();
        let (_, clone) = cloned_with_newer_remote_branch(dir.path());
        run(&clone, "git", &["branch", "topic"]);

        let listed = starts(&clone).await.unwrap();

        let points: Vec<_> = listed.iter().map(|c| c.point.clone()).collect();
        assert_eq!(points[0], StartPoint::Head);
        let local = |b: &str| StartPoint::Local { branch: b.into() };
        let remote = |b: &str| StartPoint::Remote {
            remote: "origin".into(),
            branch: b.into(),
        };
        assert!(points.contains(&local("main")));
        assert!(points.contains(&local("topic")));
        assert!(points.contains(&remote("feature")));
        // origin/HEAD only points at another branch.
        assert!(!points.contains(&remote("HEAD")));
        let first_remote = points
            .iter()
            .position(|p| matches!(p, StartPoint::Remote { .. }));
        let last_local = points
            .iter()
            .rposition(|p| matches!(p, StartPoint::Local { .. }));
        assert!(last_local < first_remote);
        assert_eq!(listed[0].subject, "first");
    }

    #[tokio::test]
    async fn a_clean_worktree_stands_with_nothing_to_lose() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create_new(&source, &new("clean")).await.unwrap();

        let standing = standing(&made.path).await.unwrap();

        assert_eq!(
            standing,
            WorktreeStanding {
                branch: Some("clean".into()),
                ..WorktreeStanding::default()
            }
        );
        assert!(!standing.dirty());
    }

    #[tokio::test]
    async fn changed_and_untracked_files_and_unmerged_commits_are_counted() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create_new(&source, &new("busy")).await.unwrap();
        std::fs::write(made.path.join("a.txt"), "changed").unwrap();
        std::fs::write(made.path.join("new.txt"), "new").unwrap();
        commit_file(&made.path, "c.txt");

        let standing = standing(&made.path).await.unwrap();

        assert_eq!(standing.changed, 2);
        assert_eq!(standing.unmerged, 1);
        assert!(standing.dirty());
        assert!(!standing.main);
    }

    #[tokio::test]
    async fn commits_no_remote_has_are_unpushed_where_a_remote_exists() {
        let dir = tempfile::tempdir().unwrap();
        let (_, clone) = cloned_with_newer_remote_branch(dir.path());
        let made = create_new(&clone, &new("local-work")).await.unwrap();
        commit_file(&made.path, "c.txt");

        let standing = standing(&made.path).await.unwrap();

        assert_eq!(standing.unpushed, 1);
    }

    #[tokio::test]
    async fn the_main_checkout_stands_as_main() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());

        assert!(standing(&source).await.unwrap().main);
    }

    #[tokio::test]
    async fn a_clean_worktree_is_deleted_with_its_merged_branch() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create_new(&source, &new("done")).await.unwrap();

        let removed = delete(&made.path, false, BranchFate::Delete).await.unwrap();

        assert_eq!(removed, WorktreeRemoved::default());
        assert!(!made.path.exists());
        assert_eq!(worktree_count(&source), 1);
        assert_eq!(branch_list(&source).trim(), "main");
    }

    #[tokio::test]
    async fn a_branch_with_unmerged_commits_is_left_when_deleting_it_would_lose_them() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create_new(&source, &new("wip")).await.unwrap();
        commit_file(&made.path, "c.txt");

        let removed = delete(&made.path, false, BranchFate::Delete).await.unwrap();

        assert!(!made.path.exists());
        let kept = removed.kept.expect("the branch stays");
        assert_eq!(kept.name, "wip");
        assert!(kept.why.contains("not fully merged"), "{}", kept.why);
        assert!(branch_list(&source).lines().any(|b| b == "wip"));
    }

    #[tokio::test]
    async fn a_branch_is_kept_when_asked_and_deleted_whole_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let kept = create_new(&source, &new("kept")).await.unwrap();
        let gone = create_new(&source, &new("gone")).await.unwrap();
        commit_file(&gone.path, "c.txt");

        let first = delete(&kept.path, false, BranchFate::Keep).await.unwrap();
        let second = delete(&gone.path, false, BranchFate::DeleteUnmerged)
            .await
            .unwrap();

        assert_eq!(first.kept, None);
        assert_eq!(second.kept, None);
        let branches = branch_list(&source);
        assert!(branches.lines().any(|b| b == "kept"));
        assert!(!branches.lines().any(|b| b == "gone"));
    }

    #[tokio::test]
    async fn a_dirty_worktree_is_not_deleted_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create_new(&source, &new("dirty")).await.unwrap();
        std::fs::write(made.path.join("work.txt"), "precious").unwrap();

        let refused = delete(&made.path, false, BranchFate::Keep)
            .await
            .unwrap_err();

        assert!(refused.contains("force"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(made.path.join("work.txt")).unwrap(),
            "precious"
        );
        delete(&made.path, true, BranchFate::Keep).await.unwrap();
        assert!(!made.path.exists());
    }

    #[tokio::test]
    async fn the_main_checkout_is_never_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());

        let refused = delete(&source, true, BranchFate::DeleteUnmerged)
            .await
            .unwrap_err();

        assert!(refused.contains("main checkout"), "{refused}");
        assert!(source.join("a.txt").exists());
        assert_eq!(branch_list(&source).trim(), "main");
    }

    #[tokio::test]
    async fn a_locked_worktree_is_not_deleted_even_when_forced() {
        let dir = tempfile::tempdir().unwrap();
        let source = git_repository(dir.path());
        let made = create_new(&source, &new("locked")).await.unwrap();
        run(
            &source,
            "git",
            &["worktree", "lock", made.path.to_str().unwrap()],
        );

        let refused = delete(&made.path, true, BranchFate::Delete)
            .await
            .unwrap_err();

        assert!(refused.contains("locked"), "{refused}");
        assert!(made.path.exists());
    }

    #[tokio::test]
    async fn a_directory_that_is_no_worktree_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), "x").unwrap();

        assert!(delete(dir.path(), true, BranchFate::Delete).await.is_err());
        assert!(dir.path().join("keep.txt").exists());
    }
}
