//! The files of each worktree's changes marked viewed, in the instance's
//! state folder, each with what its diff was when it was marked.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use tsumugi_model::review::Viewed;

/// Taken while what is kept is read, changed and written again.
static KEEPING: Mutex<()> = Mutex::new(());

const FILE: &str = "viewed-files.json";

/// By the worktree's path.
type Kept = BTreeMap<String, Vec<Viewed>>;

fn read(directory: &Path) -> Result<Kept, String> {
    let path = directory.join(FILE);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("cannot read {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Kept::default()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

fn key(worktree: &Path) -> String {
    worktree.to_string_lossy().into_owned()
}

/// The files of `worktree` marked viewed.
pub fn load(directory: &Path, worktree: &Path) -> Result<Vec<Viewed>, String> {
    Ok(read(directory)?.remove(&key(worktree)).unwrap_or_default())
}

/// Mark the file of `mark` viewed as its diff reads now, or not viewed;
/// what is then kept for `worktree`.
pub fn set(
    directory: &Path,
    worktree: &Path,
    mark: Viewed,
    viewed: bool,
) -> Result<Vec<Viewed>, String> {
    let _keeping = KEEPING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut kept = read(directory)?;
    let marks = kept.entry(key(worktree)).or_default();
    marks.retain(|m| !(m.rev == mark.rev && m.path == mark.path));
    if viewed {
        marks.push(mark);
    }
    let now = marks.clone();
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&kept).map_err(|error| error.to_string())?;
    tsumugi_sys::fs::replace(directory.join(FILE), bytes, 0o600)
        .map_err(|error| error.to_string())?;
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::repository::Rev;

    fn mark(path: &str, digest: &str) -> Viewed {
        Viewed {
            rev: Rev::WorkingCopy,
            path: path.into(),
            digest: digest.into(),
        }
    }

    #[test]
    fn a_file_marked_viewed_is_kept_for_its_worktree_as_last_marked_until_unmarked() {
        let dir = tempfile::tempdir().unwrap();
        let (w, other) = (Path::new("/w"), Path::new("/other"));
        set(dir.path(), w, mark("a", "1"), true).unwrap();
        let now = set(dir.path(), w, mark("a", "2"), true).unwrap();
        assert_eq!(now, [mark("a", "2")]);
        assert_eq!(load(dir.path(), w).unwrap(), [mark("a", "2")]);
        assert!(load(dir.path(), other).unwrap().is_empty());
        set(dir.path(), w, mark("a", "2"), false).unwrap();
        assert!(load(dir.path(), w).unwrap().is_empty());
    }
}
