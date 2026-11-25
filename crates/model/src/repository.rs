//! What Git says of a repository: the files a
//! change touches, the changes of a history, the lines of a diff, which of
//! them the details panel asks for, and the marks the tree gives files.

mod marks;

pub use marks::*;
use serde::{Deserialize, Serialize};

/// A file a change touches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Change {
    /// How it changed: git's two letters for the worktree, index then
    /// worktree (`M `, ` M`, `??`, …), and one for a commit (`M`, `A`, `D`,
    /// `R`, `C`).
    pub status: String,
    pub path: String,
    /// The path a rename or copy came from.
    pub from: Option<String>,
}

impl Change {
    /// The paths to ask a diff of, the one it came from first.
    pub fn paths(&self) -> Vec<&str> {
        self.from
            .iter()
            .map(String::as_str)
            .chain([self.path.as_str()])
            .collect()
    }

    /// Whether the index holds a change of it: the first status letter is
    /// neither blank nor `?`, and it is not in conflict.
    pub fn is_staged(&self) -> bool {
        !self.is_conflicted() && !matches!(self.status.chars().next(), None | Some(' ' | '?' | '!'))
    }

    /// Whether the worktree holds a change the index does not: a second
    /// status letter, or a file git does not track.
    pub fn is_unstaged(&self) -> bool {
        self.status == "??" || !matches!(self.status.chars().nth(1), None | Some(' '))
    }

    /// Whether the merge left it unresolved.
    pub fn is_conflicted(&self) -> bool {
        matches!(
            self.status.as_str(),
            "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU"
        )
    }

    /// Its path, and where it came from.
    pub fn shown(&self) -> String {
        match &self.from {
            Some(from) => format!("{from} -> {}", self.path),
            None => self.path.clone(),
        }
    }
}

/// A Git commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    /// Its commit hash.
    pub id: String,
    /// `id` as short as it is shown.
    pub short: String,
    /// The first line of its description; empty when it has none.
    pub subject: String,
    /// How long ago, as Git words it.
    pub when: String,
    /// The commit hash its children name it by among their parents.
    pub node: String,
    /// The commit hashes of its parents, the first parent first.
    pub parents: Vec<String>,
    /// Whether it is the working copy.
    pub current: bool,
    pub empty: bool,
    pub conflict: bool,
    pub bookmarks: Vec<String>,
}

/// The identity and full description of a single revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub id: String,
    pub short: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    /// Structural diff text with SGR colors; never parsed as a unified patch.
    Difft,
    File,
    Hunk,
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// Where changes are read from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rev {
    /// What is being worked on: Git's worktree.
    WorkingCopy,
    /// A commit by its [`Commit::id`].
    Commit(String),
    /// What the branch committed since the commit named: the merge base the
    /// branch is compared from.
    Branch(String),
}

/// What a level of a sheet shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Spot {
    Files(Rev),
    Diff(Rev, Change),
    /// A file's diff as lines of the line diff, which review comments are
    /// pinned to.
    Lines(Rev, Change),
    /// Every file of a change, one after another, each as its line diff
    /// under its heading.
    Stream(Rev),
    Log,
    /// Where the branch stands: what it is checked out as, what is
    /// unfinished in it, and how it compares with its base.
    Branch,
}

/// What was read for a spot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Listing {
    Files(Vec<Change>),
    Log(Vec<Commit>),
    Diff(Vec<DiffLine>),
    Branch(Box<Branch>),
}

/// What HEAD is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Head {
    /// A branch, which may have no commit yet.
    Branch(String),
    /// A commit checked out with no branch, by its short id.
    Detached(String),
}

/// An operation Git began in a worktree and has not finished, which stops
/// where a file is in conflict or a commit is to be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl Operation {
    /// What Git calls it.
    pub fn name(self) -> &'static str {
        match self {
            Operation::Merge => "merge",
            Operation::Rebase => "rebase",
            Operation::CherryPick => "cherry-pick",
            Operation::Revert => "revert",
        }
    }
}

/// Whether the branch could be compared with its base, and if not why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compared {
    /// The branch and its base were compared.
    Ready,
    /// No base was set or found, or the one named cannot be resolved.
    NoBase(String),
    /// The branch has no commit yet.
    Unborn,
    /// The branch and its base share no commit, as when a clone is too
    /// shallow to reach where they parted.
    Unrelated(String),
    /// Git failed to compare them.
    Failed(String),
}

/// How a branch compares with its base: what it committed since they parted,
/// and how far each has gone since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparison {
    pub status: Compared,
    /// The base as named, when there is one.
    pub base: Option<String>,
    /// The commit they parted at.
    pub merge_base: Option<String>,
    /// The commits the branch has that the base does not.
    pub ahead: usize,
    /// The commits the base has that the branch does not.
    pub behind: usize,
    /// The lines the branch added and took out since they parted.
    pub added: usize,
    pub removed: usize,
    /// The files it changed since they parted.
    pub files: Vec<Change>,
}

/// Where a worktree's branch stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    pub head: Head,
    /// What Git began and has not finished.
    pub operation: Option<Operation>,
    /// The base the user chose for this branch, which the comparison uses
    /// before any Tsumugi would find.
    pub chosen: Option<String>,
    pub comparison: Comparison,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(status: &str, path: &str, from: Option<&str>) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: from.map(Into::into),
        }
    }

    #[test]
    fn a_renamed_file_is_shown_with_where_it_came_from_and_diffed_on_both_paths() {
        let c = change("R", "new.rs", Some("old.rs"));
        assert_eq!(c.shown(), "old.rs -> new.rs");
        assert_eq!(c.paths(), ["old.rs", "new.rs"]);
        assert_eq!(change("M", "a.rs", None).paths(), ["a.rs"]);
    }

    #[test]
    fn a_status_tells_what_is_staged_what_is_not_and_what_is_in_conflict() {
        // (status, staged, unstaged, conflicted)
        for (status, staged, unstaged, conflicted) in [
            ("M ", true, false, false),
            (" M", false, true, false),
            ("MM", true, true, false),
            ("A ", true, false, false),
            ("AM", true, true, false),
            ("R ", true, false, false),
            ("RM", true, true, false),
            ("D ", true, false, false),
            (" D", false, true, false),
            ("??", false, true, false),
            ("UU", false, true, true),
            ("AA", false, true, true),
            ("DU", false, true, true),
        ] {
            let c = change(status, "a.rs", None);
            assert_eq!(
                (c.is_staged(), c.is_unstaged(), c.is_conflicted()),
                (staged, unstaged, conflicted),
                "{status:?}"
            );
        }
    }
}
