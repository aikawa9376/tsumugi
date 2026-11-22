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

/// Whether the setup script of a new worktree runs, as the user set it for a
/// repository. A repository's own file never decides this.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SetupRunPolicy {
    /// Run it in every new worktree.
    Run,
    /// Show it and run it only when the person says so.
    #[default]
    Ask,
    /// Never run it.
    Skip,
}

/// What is decided for one new worktree's setup script.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupChoice {
    /// What the repository's [`SetupRunPolicy`] says.
    #[default]
    Inherit,
    Run,
    Skip,
}

/// What came of setting up a new worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSetup {
    /// What was left out of the files given to the worktree, and why. None
    /// stops the rest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub outcome: SetupOutcome,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SetupOutcome {
    /// There was no script, or it was skipped.
    #[default]
    Nothing,
    /// The script ran in a terminal of the worktree and ended well, or runs
    /// on while the agent starts.
    Ran,
    /// The script waits for the person to say whether it runs.
    Asked { script: String },
}

/// What becomes of the uncommitted changes of the main checkout a branch is
/// merged into. Nothing of them is ever stashed, committed or overwritten
/// unless the person chose it.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MainChanges {
    /// Merge nothing while the main checkout has changes.
    #[default]
    Refuse,
    /// Set them aside in a stash first; the stash is kept, never popped.
    Stash,
    /// Commit them on the branch checked out first.
    Commit,
}

/// The issue of the local issues a worktree's session works on, as far as a
/// merge closes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedIssue {
    pub number: u64,
    pub title: String,
}

/// How merging the branch of a worktree into the main checkout stands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeStanding {
    /// The branch of the worktree, none for a detached head.
    pub branch: Option<String>,
    /// The branch the main checkout has checked out, which the merge makes
    /// its commit on; none for a detached head.
    pub base: Option<String>,
    /// Whether the worktree is the main checkout itself.
    pub main: bool,
    /// The commits of the branch the base lacks.
    pub ahead: usize,
    /// The files with uncommitted changes in the worktree, which a merge
    /// does not bring.
    pub changed: usize,
    /// The files with uncommitted changes in the main checkout.
    pub main_changed: usize,
    /// The operation Git began in the main checkout and has not finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_operation: Option<crate::repository::Operation>,
    /// The files the merge would leave in conflict, found without touching
    /// anything.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    /// The local issue the session of the worktree works on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<LinkedIssue>,
}

impl MergeStanding {
    /// Why the branch cannot be merged now, whatever the person chooses.
    pub fn refusal(&self) -> Option<String> {
        let Some(branch) = &self.branch else {
            return Some("no branch is checked out in it".into());
        };
        if self.main {
            return Some("the main checkout is not merged into itself".into());
        }
        let Some(base) = &self.base else {
            return Some("the main checkout has no branch checked out".into());
        };
        if let Some(operation) = self.main_operation {
            return Some(format!(
                "a {} is in progress in the main checkout; finish or abort it first",
                operation.name()
            ));
        }
        if self.ahead == 0 {
            return Some(format!("{branch} has no commit that {base} lacks"));
        }
        None
    }

    /// Whether the main checkout has changes that need a choice first.
    pub fn main_dirty(&self) -> bool {
        self.main_changed > 0
    }
}

/// What came of merging a branch into the main checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Merged {
    /// The branch was merged, with a merge commit of its own.
    Done {
        branch: String,
        base: String,
        /// The merge commit, abbreviated.
        commit: String,
        /// The stash the main checkout's changes were set aside in, which is
        /// kept for the person to pop.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stash: Option<String>,
        /// The issue the merge closed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closed: Option<LinkedIssue>,
    },
    /// The merge would conflict in these files, so nothing was touched.
    Conflicts { files: Vec<String> },
}

/// How a pull request of a worktree's branch stands on GitHub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullState {
    Open,
    Merged,
    Closed,
}

/// The pull request GitHub has for a worktree's branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupPull {
    pub number: u64,
    pub state: PullState,
}

/// A worktree as the cleanup lists it, with what the person weighs before
/// deleting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupEntry {
    pub path: std::path::PathBuf,
    pub standing: WorktreeStanding,
    /// What it takes of the disk, in bytes.
    pub bytes: u64,
    /// Whether its branch holds nothing the repository's base branch lacks.
    pub merged: bool,
    /// When its last commit was made, in seconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_commit: Option<u64>,
    /// The pull request of its branch, when GitHub has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull: Option<CleanupPull>,
}

impl CleanupEntry {
    /// Whether the work of its branch is done with, as GitHub says: its pull
    /// request is merged or closed.
    pub fn finished(&self) -> bool {
        self.pull
            .as_ref()
            .is_some_and(|p| matches!(p.state, PullState::Merged | PullState::Closed))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::Operation;

    fn standing() -> MergeStanding {
        MergeStanding {
            branch: Some("issue-1".into()),
            base: Some("main".into()),
            ahead: 2,
            ..MergeStanding::default()
        }
    }

    #[test]
    fn a_branch_with_commits_the_base_lacks_can_be_merged() {
        assert_eq!(standing().refusal(), None);
    }

    #[test]
    fn what_cannot_be_merged_says_why_whatever_the_main_checkout_holds() {
        let refused = |change: fn(&mut MergeStanding)| {
            let mut s = standing();
            change(&mut s);
            s.refusal().expect("a refusal")
        };
        assert!(refused(|s| s.branch = None).contains("no branch"));
        assert!(refused(|s| s.main = true).contains("main checkout"));
        assert!(refused(|s| s.base = None).contains("no branch checked out"));
        assert!(refused(|s| s.main_operation = Some(Operation::Rebase)).contains("rebase"));
        assert!(refused(|s| s.ahead = 0).contains("lacks"));
    }

    #[test]
    fn changes_in_the_main_checkout_need_a_choice_but_are_no_refusal() {
        let s = MergeStanding {
            main_changed: 3,
            ..standing()
        };
        assert!(s.main_dirty());
        assert_eq!(s.refusal(), None);
    }
}
