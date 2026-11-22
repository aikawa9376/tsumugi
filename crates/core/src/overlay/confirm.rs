//! A question the window holds the keys for until it is answered.

use std::path::PathBuf;

use tsumugi_model::{EnvironmentId, MergeStanding, Process, Session, WorktreeStanding};

use crate::details::Entry;
use crate::icons::Icon;

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// End the session, in the environment it runs in.
    Stop(EnvironmentId, Session),
    /// Kill the process of the session, in the environment it runs in.
    KillProcess(EnvironmentId, Session, Process),
    /// Restart the terminal host of the environment.
    RestartDaemon(EnvironmentId),
    /// Remove the environment, by this name, from the environments kept.
    RemoveEnvironment(EnvironmentId, String),
    /// Take the workspace at `root`, by this name, out of the tree, ending
    /// what runs there: these sessions and the terminals by these ids.
    RemoveWorkspace {
        environment: EnvironmentId,
        root: PathBuf,
        name: String,
        sessions: Vec<Session>,
        terminals: Vec<u128>,
    },
    /// End what runs in a worktree, by this name, which takes its row out
    /// of the tree: these sessions and the terminals by these ids.
    CloseWorktree {
        environment: EnvironmentId,
        name: String,
        sessions: Vec<Session>,
        terminals: Vec<u128>,
    },
    /// Delete the worktree at `path` by this name, as it stands: its
    /// directory goes, after ending these sessions and the terminals by these
    /// ids, and its branch too when `delete_branch`.
    DeleteWorktree {
        environment: EnvironmentId,
        path: PathBuf,
        name: String,
        standing: WorktreeStanding,
        sessions: Vec<Session>,
        terminals: Vec<u128>,
        delete_branch: bool,
    },
    /// Merge the branch of the worktree at `path` by this name into the
    /// branch the main checkout has checked out, as it stands; with changes
    /// in the main checkout, they are stashed first, or committed when
    /// `commit_main`.
    MergeWorktree {
        environment: EnvironmentId,
        path: PathBuf,
        name: String,
        standing: MergeStanding,
        commit_main: bool,
    },
    /// Resolve in the branch of the worktree at `path`, by this name, what
    /// would conflict in `files` when it is merged into `base`, which the
    /// main checkout has checked out.
    ResolveMerge {
        environment: EnvironmentId,
        path: PathBuf,
        name: String,
        base: String,
        files: Vec<String>,
    },
    /// Delete the worktrees a cleanup chose, each as it stands, after ending
    /// what runs in them.
    CleanUpWorktrees {
        environment: EnvironmentId,
        doomed: Vec<crate::worktree::Doomed>,
    },
    /// Run the setup script of the pending worktree `id`, made at `worktree`
    /// of `repository` and called `name`, which the repository lists and the
    /// user's policy leaves to be asked about.
    SetupWorktree {
        environment: EnvironmentId,
        repository: PathBuf,
        worktree: PathBuf,
        id: u64,
        name: String,
        script: String,
    },
    /// Replace the last commit of a worktree.
    AmendCommit {
        environment: EnvironmentId,
        worktree: PathBuf,
    },
    /// Push a worktree's branch over what the remote has of it.
    ForcePush {
        environment: EnvironmentId,
        worktree: PathBuf,
    },
    /// Give up the merge, rebase, cherry-pick or revert of a worktree.
    AbortOperation {
        environment: EnvironmentId,
        worktree: PathBuf,
        operation: tsumugi_model::repository::Operation,
    },
    /// Merge, close or mark ready the pull request `number` of the repository
    /// at `repository`, as `write` says.
    PullWrite {
        environment: EnvironmentId,
        repository: PathBuf,
        number: u64,
        write: tsumugi_model::github::Write,
    },
    /// Cherry-pick `commit` onto the branch a worktree has checked out.
    CherryPick {
        environment: EnvironmentId,
        worktree: PathBuf,
        commit: tsumugi_model::repository::Commit,
    },
    /// Revert `commit` on the branch a worktree has checked out.
    RevertCommit {
        environment: EnvironmentId,
        worktree: PathBuf,
        commit: tsumugi_model::repository::Commit,
    },
    /// Lose these changes of a worktree, staged or not.
    DiscardChanges {
        environment: EnvironmentId,
        worktree: PathBuf,
        changes: Vec<tsumugi_model::repository::Change>,
    },
}

impl Ask {
    /// Kill the process the overview's row `entry` stands for, in
    /// `session` of `environment`; a process already gone is not asked of.
    pub fn kill_process(
        environment: &EnvironmentId,
        session: Option<&Session>,
        entry: Option<&Entry>,
    ) -> Option<Self> {
        match entry? {
            Entry::Port(port) if !port.process.zombie => Some(Self::KillProcess(
                environment.clone(),
                session?.clone(),
                port.process.clone(),
            )),
            Entry::Process(process) if !process.zombie => Some(Self::KillProcess(
                environment.clone(),
                session?.clone(),
                process.clone(),
            )),
            _ => None,
        }
    }

    /// What the dialog shows: its icon, its title, what it says, and what
    /// answering yes is called.
    pub fn dialog(&self) -> (Icon, String, String, &'static str) {
        match self {
            Self::Stop(_, s) => (
                Icon::PlayerStop,
                format!("End {}?", s.name),
                "The agent exits where it runs. Its conversation remains in the provider's history.".into(),
                "End",
            ),
            Self::KillProcess(_, s, p) => (
                Icon::X,
                format!("Kill {}?", p.name),
                format!("Send SIGKILL to PID {} in session {}? Unsaved work in this process will be lost.", p.pid, s.name),
                "Kill process",
            ),
            Self::RestartDaemon(_) => (
                Icon::ServerBolt,
                "Restart the sessions' daemon?".into(),
                "Every session it holds ends: agents and shells, with what they scrolled back. Afterwards you can bring them back: agents resume their conversations, and shells open again where they ran.".into(),
                "Restart daemon",
            ),
            Self::RemoveEnvironment(_, name) => (
                Icon::SquareMinus,
                format!("Remove {name}?"),
                "Every window stops listing it. Its sessions keep running on its host, and it can be restored from Settings.".into(),
                "Remove",
            ),
            Self::RemoveWorkspace {
                name,
                sessions,
                terminals,
                ..
            } => (
                Icon::SquareMinus,
                format!("Remove {name} from the tree?"),
                format!("Ending its {} takes it out of the tree; it is listed again once anything starts there. Conversations remain in the provider's history.", running(sessions, terminals)),
                "End and remove",
            ),
            Self::CloseWorktree {
                name,
                sessions,
                terminals,
                ..
            } => (
                Icon::X,
                format!("Close {name}?"),
                format!("Ending its {} takes it out of the tree; the workspace stays listed. Conversations remain in the provider's history.", running(sessions, terminals)),
                "End and close",
            ),
            Self::DeleteWorktree {
                path,
                name,
                standing,
                sessions,
                terminals,
                ..
            } => {
                let mut lines = vec![format!("Its directory, {}, is deleted.", path.display())];
                if let Some(running) = Some(running(sessions, terminals)).filter(|r| !r.is_empty()) {
                    lines.push(format!("Its {running} end first. Conversations remain in the provider's history."));
                }
                if standing.dirty() {
                    lines.push(format!("{} with changes not committed will be lost.", counted(standing.changed, "file").unwrap_or_default()));
                }
                if standing.unpushed > 0 {
                    lines.push(format!("{} not pushed anywhere.", counted(standing.unpushed, "commit").unwrap_or_default()));
                }
                (
                    Icon::Trash,
                    format!("Delete {name}?"),
                    lines.join("\n"),
                    if standing.dirty() { "Delete and discard changes" } else { "Delete" },
                )
            }
            Self::MergeWorktree {
                name,
                standing,
                commit_main,
                ..
            } => {
                let base = standing.base.as_deref().unwrap_or_default();
                let mut lines = vec![format!(
                    "{} of {name} are merged into {base} with a merge commit of its own.",
                    counted(standing.ahead, "commit").unwrap_or_default()
                )];
                if standing.main_dirty() {
                    let changed = counted(standing.main_changed, "file").unwrap_or_default();
                    lines.push(if *commit_main {
                        format!("{changed} with uncommitted changes in the main checkout are committed on {base} first.")
                    } else {
                        format!("{changed} with uncommitted changes in the main checkout are set aside in a stash first, and kept there.")
                    });
                }
                if standing.changed > 0 {
                    lines.push(format!(
                        "{} with uncommitted changes in the worktree are not part of the merge.",
                        counted(standing.changed, "file").unwrap_or_default()
                    ));
                }
                if let Some(issue) = &standing.issue {
                    lines.push(format!(
                        "Issue #{} ({}) is closed.",
                        issue.number, issue.title
                    ));
                }
                lines.push("Afterwards the worktree is offered for deleting.".into());
                (
                    Icon::GitBranch,
                    format!("Merge {name} into {base}?"),
                    lines.join("
"),
                    match (standing.main_dirty(), commit_main) {
                        (false, _) => "Merge",
                        (true, false) => "Stash and merge",
                        (true, true) => "Commit and merge",
                    },
                )
            }
            Self::ResolveMerge {
                name, base, files, ..
            } => (
                Icon::GitBranch,
                format!("Merging {name} into {base} would conflict"),
                format!(
                    "The conflict is in {}:
{}
The main checkout is not touched. {base} is merged into {name} in its worktree instead, leaving the conflicts there, and a session starts in it to resolve them.",
                    counted(files.len(), "file").unwrap_or_default(),
                    files.iter().map(|f| format!("- {f}")).collect::<Vec<_>>().join("
")
                ),
                "Resolve in the branch",
            ),
            Self::CleanUpWorktrees { doomed, .. } => {
                let lines: Vec<String> = doomed
                    .iter()
                    .filter(|d| d.risky())
                    .map(|d| {
                        let mut what = Vec::new();
                        if d.standing.dirty() {
                            what.push(format!("{} with changes not committed will be lost", counted(d.standing.changed, "file").unwrap_or_default()));
                        }
                        if d.standing.unpushed > 0 {
                            what.push(format!("{} not pushed anywhere", counted(d.standing.unpushed, "commit").unwrap_or_default()));
                        }
                        if d.standing.branch.is_none() {
                            what.push("no branch is checked out, so its commits may be reached by nothing else".into());
                        }
                        if let Some(running) = Some(running(&d.sessions, &d.terminals)).filter(|r| !r.is_empty()) {
                            what.push(format!("{running} end first"));
                        }
                        format!("{}: {}", d.name, what.join("; "))
                    })
                    .collect();
                let lost = doomed.iter().any(|d| d.standing.dirty());
                (
                    Icon::Trash,
                    format!("Delete {}?", counted(doomed.len(), "worktree").unwrap_or_default()),
                    format!("Their directories are deleted. Conversations remain in the provider's history. A branch with commits nothing else has is kept.\n{}", lines.join("\n")),
                    if lost { "Delete and discard changes" } else { "Delete" },
                )
            }
            Self::SetupWorktree { name, script, .. } => (
                Icon::Terminal2,
                format!("Run the setup of {name}?"),
                format!(
                    "The repository lists this for new worktrees. It runs in a terminal of the worktree, with your permissions:\n\n{script}"
                ),
                "Run setup",
            ),
            Self::AmendCommit { .. } => (
                Icon::Pencil,
                "Amend the last commit?".into(),
                "The last commit is replaced by a new one holding what is staged now, with the message you write. If it was pushed, pushing it again needs a force push.".into(),
                "Amend",
            ),
            Self::ForcePush { .. } => (
                Icon::ArrowBarToUp,
                "Force push?".into(),
                "The remote's branch is overwritten with this one, unless somebody else pushed to it since you last fetched. Commits only the remote has are lost.".into(),
                "Force push",
            ),
            Self::AbortOperation { operation, .. } => (
                Icon::Trash,
                format!("Abort the {}?", operation.name()),
                format!(
                    "The worktree goes back as it was before the {} began. What you resolved during it is lost.",
                    operation.name()
                ),
                "Abort",
            ),
            Self::PullWrite { number, write, .. } => {
                let (title, body, yes, _) = crate::pull_change::question(*number, write);
                let icon = match write {
                    tsumugi_model::github::Write::Close { .. } => Icon::X,
                    tsumugi_model::github::Write::Draft { .. } => Icon::BrandGithub,
                    _ => Icon::GitBranch,
                };
                (icon, title, body, yes)
            }
            Self::CherryPick { commit, .. } => (
                Icon::GitBranch,
                format!("Cherry-pick {}?", commit.short),
                format!(
                    "{}\n\nIts changes are applied to the branch checked out in a commit of its own. A conflict stops it for you to resolve and continue, or abort.",
                    commit.subject
                ),
                "Cherry-pick",
            ),
            Self::RevertCommit { commit, .. } => (
                Icon::ArrowBackUp,
                format!("Revert {}?", commit.short),
                format!(
                    "{}\n\nIts changes are undone on the branch checked out in a commit of its own. A conflict stops it for you to resolve and continue, or abort.",
                    commit.subject
                ),
                "Revert",
            ),
            Self::DiscardChanges { changes, .. } => (
                Icon::Trash,
                match changes.as_slice() {
                    [one] => format!("Discard changes to {}?", one.path),
                    many => format!("Discard {} changes?", many.len()),
                },
                "Staged and unstaged changes go back to the last commit, and a new file is deleted. This cannot be undone.".into(),
                "Discard",
            ),
        }
    }

    /// The choice the question holds besides yes and no: what it says, and
    /// whether it is chosen.
    pub fn option(&self) -> Option<(String, bool)> {
        match self {
            Self::DeleteWorktree {
                standing,
                delete_branch,
                ..
            } => {
                let branch = standing.branch.as_deref()?;
                let lost = match standing.unmerged {
                    0 => String::new(),
                    n => format!(", losing {}", counted(n, "commit").unwrap_or_default()),
                };
                Some((
                    format!("Delete the branch {branch} too{lost}"),
                    *delete_branch,
                ))
            }
            Self::MergeWorktree {
                standing,
                commit_main,
                ..
            } if standing.main_dirty() => Some((
                format!(
                    "Commit them on {} instead of stashing them",
                    standing.base.as_deref().unwrap_or_default()
                ),
                *commit_main,
            )),
            _ => None,
        }
    }

    /// Whether answering yes loses what cannot be had back.
    pub fn destroys(&self) -> bool {
        match self {
            Self::KillProcess(..)
            | Self::RestartDaemon(_)
            | Self::DiscardChanges { .. }
            | Self::AbortOperation { .. }
            | Self::AmendCommit { .. }
            | Self::ForcePush { .. } => true,
            Self::RemoveWorkspace { terminals, .. } | Self::CloseWorktree { terminals, .. } => {
                !terminals.is_empty()
            }
            Self::DeleteWorktree {
                standing,
                terminals,
                delete_branch,
                ..
            } => {
                standing.dirty()
                    || !terminals.is_empty()
                    || (*delete_branch && standing.unmerged > 0)
            }
            Self::CleanUpWorktrees { doomed, .. } => doomed.iter().any(|d| {
                d.standing.dirty()
                    || d.standing.unpushed > 0
                    || d.standing.branch.is_none()
                    || !d.terminals.is_empty()
            }),
            Self::PullWrite { number, write, .. } => crate::pull_change::question(*number, write).3,
            Self::Stop(..)
            | Self::RemoveEnvironment(..)
            | Self::SetupWorktree { .. }
            | Self::MergeWorktree { .. }
            | Self::ResolveMerge { .. }
            | Self::CherryPick { .. }
            | Self::RevertCommit { .. } => false,
        }
    }
}

/// What ends, said as a count of each kind.
fn running(sessions: &[Session], terminals: &[u128]) -> String {
    [
        counted(sessions.len(), "session"),
        counted(terminals.len(), "terminal"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" and ")
}

/// `n` of `what`, said as a count; none when there are none.
fn counted(n: usize, what: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {what}")),
        n => Some(format!("{n} {what}s")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_the_daemon_is_asked_as_a_loss_as_killing_a_process_is() {
        assert!(Ask::RestartDaemon(EnvironmentId::default()).destroys());
        assert!(!Ask::Stop(EnvironmentId::default(), Session::default()).destroys());
    }

    #[test]
    fn removing_an_environment_names_it_and_is_not_asked_as_a_loss() {
        let ask = Ask::RemoveEnvironment(EnvironmentId::new("there"), "There".into());
        let (_, title, body, action) = ask.dialog();
        assert_eq!(title, "Remove There?");
        assert!(body.contains("keep running"), "{body}");
        assert_eq!(action, "Remove");
        assert!(!ask.destroys());
    }

    fn removing(sessions: Vec<Session>, terminals: Vec<u128>) -> Ask {
        Ask::RemoveWorkspace {
            environment: EnvironmentId::default(),
            root: "/api".into(),
            name: "api".into(),
            sessions,
            terminals,
        }
    }

    #[test]
    fn removing_a_workspace_names_it_and_counts_what_ends_there() {
        let (_, title, body, action) =
            removing(vec![Session::default(), Session::default()], vec![7]).dialog();
        assert_eq!(title, "Remove api from the tree?");
        assert!(
            body.starts_with("Ending its 2 sessions and 1 terminal takes"),
            "{body}"
        );
        assert_eq!(action, "End and remove");
        let (_, _, body, _) = removing(vec![Session::default()], vec![]).dialog();
        assert!(body.starts_with("Ending its 1 session takes"), "{body}");
    }

    #[test]
    fn closing_a_worktree_names_it_counts_what_ends_and_is_a_loss_only_for_terminals() {
        let closing = |sessions, terminals| Ask::CloseWorktree {
            environment: EnvironmentId::default(),
            name: "feat/oauth".into(),
            sessions,
            terminals,
        };
        let ask = closing(vec![Session::default()], vec![7, 8]);
        let (_, title, body, action) = ask.dialog();
        assert_eq!(title, "Close feat/oauth?");
        assert!(
            body.starts_with("Ending its 1 session and 2 terminals takes"),
            "{body}"
        );
        assert_eq!(action, "End and close");
        assert!(ask.destroys());
        assert!(!closing(vec![Session::default()], vec![]).destroys());
    }

    #[test]
    fn removing_a_workspace_is_asked_as_a_loss_only_when_terminals_end_with_it() {
        assert!(!removing(vec![Session::default()], vec![]).destroys());
        assert!(removing(vec![], vec![7]).destroys());
    }

    #[test]
    fn killing_an_overview_process_requires_a_confirmation_bound_to_its_session_and_identity() {
        let session = tsumugi_model::testing::session("session-one")
            .name("work")
            .build();
        let mut process = Process {
            pid: 123,
            started_secs: 456,
            name: "node_repl".into(),
            ..Default::default()
        };
        let there = EnvironmentId::new("there");
        let ask = Ask::kill_process(
            &there,
            Some(&session),
            Some(&Entry::Process(process.clone())),
        )
        .unwrap();
        assert_eq!(
            ask,
            Ask::KillProcess(there.clone(), session.clone(), process.clone())
        );
        let (_, title, body, action) = ask.dialog();
        assert!(title.contains("node_repl"));
        assert!(body.contains("123"));
        assert!(body.contains("work"));
        assert!(body.contains("SIGKILL"));
        assert_eq!(action, "Kill process");
        assert!(Ask::kill_process(&there, None, Some(&Entry::Process(process.clone()))).is_none());
        assert!(Ask::kill_process(&there, Some(&session), None).is_none());
        process.zombie = true;
        assert!(
            Ask::kill_process(&there, Some(&session), Some(&Entry::Process(process))).is_none()
        );
    }

    #[test]
    fn stopping_a_port_kills_the_process_that_listens_on_it() {
        let session = tsumugi_model::testing::session("session-one").build();
        let process = Process {
            pid: 123,
            started_secs: 456,
            name: "node".into(),
            ..Default::default()
        };
        let port = tsumugi_model::Port {
            port: 3000,
            process: process.clone(),
            ..Default::default()
        };
        let there = EnvironmentId::new("there");
        assert_eq!(
            Ask::kill_process(&there, Some(&session), Some(&Entry::Port(port.clone()))),
            Some(Ask::KillProcess(there.clone(), session.clone(), process))
        );
        let mut gone = port;
        gone.process.zombie = true;
        assert!(Ask::kill_process(&there, Some(&session), Some(&Entry::Port(gone))).is_none());
    }
}
