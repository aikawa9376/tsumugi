//! What a person asks of a repository's worktrees from the window: where a
//! new one starts, what it is called, and how a worktree stands before it is
//! deleted.

use serde::{Deserialize, Serialize};

/// Where a new worktree starts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StartPoint {
    /// The commit the repository's checkout is at.
    #[default]
    Head,
    /// A local branch.
    Local { branch: String },
    /// A branch of a remote, fetched first so it is as the remote has it.
    Remote { remote: String, branch: String },
    /// Any other revision git understands, such as a commit.
    Rev { rev: String },
}

impl StartPoint {
    /// How git names it once it is local.
    pub fn name(&self) -> String {
        match self {
            Self::Head => "HEAD".into(),
            Self::Local { branch } => branch.clone(),
            Self::Remote { remote, branch } => format!("{remote}/{branch}"),
            Self::Rev { rev } => rev.clone(),
        }
    }
}

/// A worktree to make.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewWorktree {
    /// What it is called, which names its directory.
    pub name: String,
    pub start: StartPoint,
    /// The branch it works on, new; named after its directory when none is
    /// given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// How a worktree stands, as far as deleting it can lose work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeStanding {
    /// The branch checked out, none for a detached head.
    pub branch: Option<String>,
    /// The files changed, staged or not tracked, which deleting loses.
    pub changed: usize,
    /// The commits of the branch no remote has.
    pub unpushed: usize,
    /// The commits of the branch not in the branch it would be merged into,
    /// which deleting the branch loses.
    pub unmerged: usize,
    /// Whether git keeps it from being removed, as `git worktree lock` does.
    pub locked: bool,
    /// Whether it is the repository's main checkout, which is never deleted.
    pub main: bool,
}

impl WorktreeStanding {
    /// Whether deleting it loses uncommitted work.
    pub fn dirty(&self) -> bool {
        self.changed > 0
    }
}

/// What came of deleting a worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRemoved {
    /// The branch git left, with why, when it did not go with the worktree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<KeptBranch>,
}

/// A branch left when its worktree was deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptBranch {
    pub name: String,
    pub why: String,
}

/// A branch a new worktree may start from, as the repository lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartChoice {
    pub point: StartPoint,
    /// The last commit's subject, for the list to show.
    #[serde(default)]
    pub subject: String,
}

/// What becomes of a worktree's branch when the worktree is deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchFate {
    /// The branch stays.
    #[default]
    Keep,
    /// The branch goes unless git finds commits in it that no other branch
    /// has, in which case it stays.
    Delete,
    /// The branch goes with its commits.
    DeleteUnmerged,
}
