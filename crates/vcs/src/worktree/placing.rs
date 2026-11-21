//! What a new worktree is given of the repository's primary checkout that Git
//! does not carry: gitignored directories linked in, and gitignored files
//! copied in. Nothing here runs a program the repository names, and nothing
//! reaches outside the primary checkout or the worktree.

use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::{canonical, output};

/// The most bytes of files one worktree is given by copying.
const COPY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The most files and directories one worktree is given by copying.
const COPY_ENTRIES: u64 = 50_000;
/// The most paths a list is read for.
const LISTED: usize = 100;

/// `path` as a path below a checkout's root, or none when it is not one:
/// backslashes become `/`, a leading `./` and a trailing `/` go, and an
/// absolute path, a drive, an empty, `.` or `..` segment and any `.git`
/// segment refuse it.
pub fn relative(path: &str) -> Option<String> {
    let path = path.trim().replace('\\', "/");
    let mut path = path.as_str();
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    let path = path.trim_end_matches('/');
    let drive = path.as_bytes().get(1) == Some(&b':');
    let refused = path.is_empty()
        || path.starts_with('/')
        || drive
        || path
            .split('/')
            .any(|part| matches!(part, "" | "." | ".." | ".git"));
    (!refused).then(|| path.to_string())
}

/// The paths of a `.worktreeinclude`: one literal path a line, blank lines
/// and `#` lines ignored, and what says more than one path, a glob or a
/// negation, or is no path below the root, left out with a word on why.
pub fn included(text: &str) -> (Vec<String>, Vec<String>) {
    let mut paths: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('!') || line.contains(['*', '?']) {
            warnings.push(format!(
                "{line} is a pattern, and .worktreeinclude lists literal paths"
            ));
            continue;
        }
        match relative(line) {
            Some(path) if paths.contains(&path) => {}
            Some(path) if paths.len() < LISTED => paths.push(path),
            Some(_) => warnings.push(format!("{line} is past the first {LISTED} paths")),
            None => warnings.push(format!("{line} is not a path below the repository")),
        }
    }
    (paths, warnings)
}

/// Gives the worktree at `worktree` what the primary checkout at `primary`
/// has of `shared`, the directories to link, and `copied`, the files and
/// directories to copy. Only what exists and Git ignores there is given, and
/// only where the worktree has nothing yet, so a file the repository tracks
/// is never replaced. What is left out is told in the warnings; none stops
/// the rest.
pub async fn place(
    primary: &Path,
    worktree: &Path,
    shared: &[String],
    copied: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();
    let primary = match canonical(primary) {
        Ok(primary) => primary,
        Err(error) => return vec![error],
    };
    let mut paths = Vec::new();
    for (wanted, linked) in shared
        .iter()
        .map(|path| (path, true))
        .chain(copied.iter().map(|path| (path, false)))
    {
        let Some(path) = relative(wanted) else {
            warnings.push(format!("{wanted} is not a path below the repository"));
            continue;
        };
        // What the primary checkout does not have is not an error: the list
        // is written for every machine the repository is used on.
        if primary.join(&path).symlink_metadata().is_err() {
            continue;
        }
        if ignored(&primary, &path).await {
            paths.push((path, linked));
        } else {
            warnings.push(format!("{path} is not ignored by Git, so it is not given"));
        }
    }
    let worktree = worktree.to_path_buf();
    let given = tokio::task::spawn_blocking(move || {
        let mut warnings = Vec::new();
        let mut budget = Budget::default();
        for (path, linked) in paths {
            let done = if linked {
                link(&primary, &worktree, &path)
            } else {
                copy(&primary, &worktree, &path, &mut budget)
            };
            if let Err(why) = done {
                warnings.push(format!("{path}: {why}"));
            }
        }
        warnings
    })
    .await
    .unwrap_or_else(|error| vec![format!("giving files stopped: {error}")]);
    warnings.extend(given);
    warnings
}

/// Whether Git ignores `path` in the checkout at `root`.
async fn ignored(root: &Path, path: &str) -> bool {
    let args: [&OsStr; 4] = [
        "check-ignore".as_ref(),
        "-q".as_ref(),
        "--".as_ref(),
        path.as_ref(),
    ];
    output(root, "git", &args)
        .await
        .is_ok_and(|output| output.status.code() == Some(0))
}

/// The primary checkout's `path`, resolved, when it is a real thing inside
/// the primary checkout: a link out of it is refused.
fn source_of(primary: &Path, path: &str) -> Result<PathBuf, String> {
    let from = primary.join(path);
    match std::fs::symlink_metadata(&from) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => return Err("is not there".into()),
        Err(error) => return Err(error.to_string()),
    }
    let real = std::fs::canonicalize(&from).map_err(|error| error.to_string())?;
    if !real.starts_with(primary) {
        return Err("leads out of the repository, so it is not given".into());
    }
    Ok(real)
}

/// Where `path` goes in the worktree: its parents made, none of which may be
/// a link, and none of it taken already. Nothing when something is there.
fn destination_of(worktree: &Path, path: &str) -> Result<Option<PathBuf>, String> {
    let to = worktree.join(path);
    let mut at = worktree.to_path_buf();
    let mut parts: Vec<&str> = path.split('/').collect();
    let last = parts.pop().unwrap_or_default();
    for part in parts {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.is_dir() => {}
            // A link, or a file, where a directory is wanted: what a path
            // below it would reach is not the worktree's own.
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                std::fs::create_dir(&at).map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    at.push(last);
    match std::fs::symlink_metadata(&at) {
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Some(to)),
        Err(error) => Err(error.to_string()),
    }
}

fn link(primary: &Path, worktree: &Path, path: &str) -> Result<(), String> {
    let real = source_of(primary, path)?;
    if !real.is_dir() {
        return Err("is not a directory, so it is not shared".into());
    }
    let Some(to) = destination_of(worktree, path)? else {
        return Ok(());
    };
    symlink(&real, &to).map_err(|error| format!("cannot link it: {error}"))
}

#[cfg(unix)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(from, to)
}

#[cfg(windows)]
fn symlink(from: &Path, to: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(from, to)
}

/// What copying has taken of the budget.
#[derive(Default)]
struct Budget {
    bytes: u64,
    entries: u64,
}

fn copy(primary: &Path, worktree: &Path, path: &str, budget: &mut Budget) -> Result<(), String> {
    let real = source_of(primary, path)?;
    let Some(to) = destination_of(worktree, path)? else {
        return Ok(());
    };
    let (mut bytes, mut entries) = (0, 0);
    size(&real, &mut bytes, &mut entries)?;
    if budget.bytes + bytes > COPY_BYTES || budget.entries + entries > COPY_ENTRIES {
        return Err("is too large to copy".into());
    }
    budget.bytes += bytes;
    budget.entries += entries;
    let mut skipped = Vec::new();
    let made = copy_into(&real, &to, &mut skipped);
    if made.is_err() {
        // What this copy made is taken back, so a copy that stopped is not
        // taken for the whole thing.
        let _ = if to.is_dir() {
            std::fs::remove_dir_all(&to)
        } else {
            std::fs::remove_file(&to)
        };
    }
    made.map_err(|error| format!("cannot copy it: {error}"))?;
    match skipped.as_slice() {
        [] => Ok(()),
        some => Err(format!("is copied without {}", some.join(", "))),
    }
}

/// The bytes and entries below `path`, which stops at what a copy would
/// refuse, so a tree too large is not walked to its end.
fn size(path: &Path, bytes: &mut u64, entries: &mut u64) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    *entries += 1;
    if *entries > COPY_ENTRIES || *bytes > COPY_BYTES {
        return Err("is too large to copy".into());
    }
    if meta.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
            size(&entry.map_err(|e| e.to_string())?.path(), bytes, entries)?;
        }
    } else if meta.is_file() {
        *bytes += meta.len();
    }
    Ok(())
}

/// `from` copied to `to`, which is not there. A link below it is left out,
/// whatever it leads to, and so is anything but a file and a directory; each
/// is named in `skipped`.
fn copy_into(from: &Path, to: &Path, skipped: &mut Vec<String>) -> std::io::Result<()> {
    let meta = std::fs::metadata(from)?;
    if meta.is_dir() {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let name = entry.file_name();
            let kind = entry.file_type()?;
            if name == ".git" || !(kind.is_file() || kind.is_dir()) {
                skipped.push(name.to_string_lossy().into_owned());
                continue;
            }
            copy_into(&entry.path(), &to.join(&name), skipped)?;
        }
        std::fs::set_permissions(to, meta.permissions())?;
    } else if meta.is_file() {
        let mut source = std::fs::File::open(from)?;
        // `create_new` keeps a file that appeared since the check.
        let mut target = OpenOptions::new().write(true).create_new(true).open(to)?;
        std::io::copy(&mut source, &mut target)?;
        std::fs::set_permissions(to, meta.permissions())?;
    }
    Ok(())
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

    /// A repository ignoring `.env`, `.cache/` and `local/`, tracking `a.txt`
    /// and `keep.env`, and a second directory for a worktree of it.
    fn repository() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let primary = std::fs::canonicalize(dir.path()).unwrap().join("api");
        std::fs::create_dir(&primary).unwrap();
        git(&primary, &["init", "-q", "-b", "main"]);
        std::fs::write(
            primary.join(".gitignore"),
            ".env\n.cache/\nlocal/\nnote.txt\n",
        )
        .unwrap();
        std::fs::write(primary.join("a.txt"), "a").unwrap();
        git(&primary, &["add", "."]);
        git(&primary, &["commit", "-q", "-m", "first"]);
        let worktree = dir.path().join("api-wt");
        std::fs::create_dir(&worktree).unwrap();
        (dir, primary, worktree)
    }

    fn strings(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_path_is_below_the_root_or_none() {
        assert_eq!(relative("./.env/").as_deref(), Some(".env"));
        assert_eq!(
            relative(r".vscode\settings.json").as_deref(),
            Some(".vscode/settings.json")
        );
        for refused in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "C:/x",
            ".git/config",
            "a/.git",
            ".",
        ] {
            assert_eq!(relative(refused), None, "{refused}");
        }
    }

    #[test]
    fn an_include_list_keeps_literal_paths_and_says_what_it_leaves_out() {
        let (paths, warnings) = included("# secrets\n.env\n\n.env\n./local/\n*.log\n!x\n../out\n");

        assert_eq!(paths, [".env", "local"]);
        assert_eq!(warnings.len(), 3, "{warnings:?}");
    }

    #[tokio::test]
    async fn an_ignored_file_is_copied_on_its_own() {
        let (_dir, primary, worktree) = repository();
        std::fs::write(primary.join(".env"), "KEY=1").unwrap();

        let warnings = place(&primary, &worktree, &[], &strings(&[".env"])).await;

        assert_eq!(warnings, Vec::<String>::new());
        std::fs::write(primary.join(".env"), "KEY=2").unwrap();
        assert_eq!(
            std::fs::read_to_string(worktree.join(".env")).unwrap(),
            "KEY=1"
        );
    }

    #[tokio::test]
    async fn an_ignored_directory_is_copied_with_what_is_in_it() {
        let (_dir, primary, worktree) = repository();
        std::fs::create_dir_all(primary.join("local/deep")).unwrap();
        std::fs::write(primary.join("local/deep/x"), "x").unwrap();

        place(&primary, &worktree, &[], &strings(&["local"])).await;

        assert_eq!(
            std::fs::read_to_string(worktree.join("local/deep/x")).unwrap(),
            "x"
        );
        assert!(
            !worktree
                .join("local")
                .symlink_metadata()
                .unwrap()
                .is_symlink()
        );
    }

    #[tokio::test]
    async fn a_file_git_tracks_is_not_copied_over_the_worktrees_own() {
        let (_dir, primary, worktree) = repository();
        std::fs::write(worktree.join("a.txt"), "the worktree's").unwrap();
        std::fs::write(primary.join("a.txt"), "changed in the primary").unwrap();

        let warnings = place(&primary, &worktree, &[], &strings(&["a.txt"])).await;

        assert_eq!(
            std::fs::read_to_string(worktree.join("a.txt")).unwrap(),
            "the worktree's"
        );
        assert!(warnings[0].contains("not ignored"), "{warnings:?}");
    }

    #[tokio::test]
    async fn an_ignored_file_the_worktree_already_has_is_left_as_it_is() {
        let (_dir, primary, worktree) = repository();
        std::fs::write(primary.join(".env"), "from the primary").unwrap();
        std::fs::write(worktree.join(".env"), "already here").unwrap();

        place(&primary, &worktree, &[], &strings(&[".env"])).await;

        assert_eq!(
            std::fs::read_to_string(worktree.join(".env")).unwrap(),
            "already here"
        );
    }

    #[tokio::test]
    async fn a_missing_file_is_passed_over_without_a_word_and_the_rest_is_given() {
        let (_dir, primary, worktree) = repository();
        std::fs::write(primary.join("note.txt"), "n").unwrap();

        let warnings = place(&primary, &worktree, &[], &strings(&[".env", "note.txt"])).await;

        assert_eq!(warnings, Vec::<String>::new());
        assert!(worktree.join("note.txt").is_file());
        assert!(!worktree.join(".env").exists());
    }

    #[tokio::test]
    async fn a_path_that_climbs_out_is_refused() {
        let (dir, primary, worktree) = repository();
        std::fs::write(dir.path().join("outside"), "x").unwrap();

        let warnings = place(
            &primary,
            &worktree,
            &[],
            &strings(&["../outside", "/etc/hosts"]),
        )
        .await;

        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(!worktree.join("outside").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_link_in_the_primary_that_leads_out_of_it_is_not_followed() {
        let (dir, primary, worktree) = repository();
        std::fs::write(dir.path().join("secret"), "x").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret"), primary.join(".env")).unwrap();

        let warnings = place(&primary, &worktree, &[], &strings(&[".env"])).await;

        assert!(warnings[0].contains("leads out"), "{warnings:?}");
        assert!(!worktree.join(".env").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_link_inside_a_copied_directory_is_left_out() {
        let (dir, primary, worktree) = repository();
        std::fs::create_dir(primary.join("local")).unwrap();
        std::fs::write(primary.join("local/kept"), "k").unwrap();
        std::os::unix::fs::symlink(dir.path(), primary.join("local/out")).unwrap();

        let warnings = place(&primary, &worktree, &[], &strings(&["local"])).await;

        assert!(worktree.join("local/kept").is_file());
        assert!(worktree.join("local/out").symlink_metadata().is_err());
        assert!(warnings[0].contains("without out"), "{warnings:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_directory_in_the_worktree_that_is_a_link_is_not_written_through() {
        let (dir, primary, worktree) = repository();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, worktree.join("local")).unwrap();
        std::fs::create_dir(primary.join("local")).unwrap();
        std::fs::write(primary.join("local/x"), "x").unwrap();

        place(&primary, &worktree, &[], &strings(&["local/x"])).await;

        assert!(!elsewhere.join("x").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_shared_directory_is_a_link_to_the_primarys() {
        let (_dir, primary, worktree) = repository();
        std::fs::create_dir(primary.join(".cache")).unwrap();
        std::fs::write(primary.join(".cache/x"), "x").unwrap();

        let warnings = place(&primary, &worktree, &strings(&[".cache"]), &[]).await;

        assert_eq!(warnings, Vec::<String>::new());
        assert_eq!(
            std::fs::read_link(worktree.join(".cache")).unwrap(),
            primary.join(".cache")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_file_is_not_shared_and_an_unignored_directory_is_not_either() {
        let (_dir, primary, worktree) = repository();
        std::fs::write(primary.join(".env"), "x").unwrap();
        std::fs::create_dir(primary.join("src")).unwrap();

        let warnings = place(&primary, &worktree, &strings(&[".env", "src"]), &[]).await;

        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(!worktree.join(".env").exists());
        assert!(!worktree.join("src").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_path_already_shared_is_not_copied_into_it() {
        let (_dir, primary, worktree) = repository();
        std::fs::create_dir(primary.join(".cache")).unwrap();
        std::fs::write(primary.join(".cache/x"), "x").unwrap();

        place(
            &primary,
            &worktree,
            &strings(&[".cache"]),
            &strings(&[".cache/x"]),
        )
        .await;

        assert!(
            !worktree.join(".cache").is_dir()
                || worktree
                    .join(".cache")
                    .symlink_metadata()
                    .unwrap()
                    .is_symlink()
        );
    }
}
