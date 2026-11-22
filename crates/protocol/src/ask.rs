//! What a window or a command asks of the work host and waits for: each ask
//! is answered once, by its number, with what came of it or why it failed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tsumugi_model::{Agent, AgentPermissions, TerminalProgram, TerminalSession};

use crate::{DraftChanges, Drafts, HostPath};

/// What is asked of the work host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Ask {
    /// What came of the browser command numbered `call`, which the work host
    /// handed this window for a session's agent.
    BrowserResult {
        call: u64,
        result: crate::BrowserResult,
    },
    CreateLocalIssue(tsumugi_model::NewLocalIssue),
    StartLocalIssue {
        id: String,
    },
    QueueLocalIssue(tsumugi_model::NewLocalIssue),
    /// Hand the open issues of `issues`, by id, to a leader session, which
    /// carries them through itself; `max_workers` limits its workers for this
    /// run in place of the setting.
    StartLeader {
        issues: Vec<String>,
        max_workers: Option<u32>,
    },
    CancelLocalIssue {
        id: String,
    },
    RetryLocalIssue {
        id: String,
    },
    CloseLocalIssue {
        id: String,
    },
    ReopenLocalIssue {
        id: String,
    },
    DeleteLocalIssue {
        id: String,
    },
    /// Keep a command in the project holding `dir`, or keep the command of
    /// `id` anew.
    SaveCommand(tsumugi_model::NewSavedCommand),
    /// Keep the saved command of `id` no more.
    DeleteSavedCommand {
        id: String,
    },
    /// Change what the worktree at `worktree` says of how its work stands.
    SetCheckpoint {
        worktree: std::path::PathBuf,
        comment: tsumugi_model::Set<String>,
        status: tsumugi_model::Set<tsumugi_model::WorktreeStatus>,
    },
    /// Keep a schedule, disabled.
    SaveSchedule(tsumugi_model::NewSchedule),
    /// Change what the schedule of `id` says.
    EditSchedule {
        id: String,
        edit: tsumugi_model::ScheduleEdit,
    },
    /// Enable the schedule of `id`, which allows its times from now on, or
    /// pause it.
    SetScheduleEnabled {
        id: String,
        enabled: bool,
    },
    /// Queue the request of the schedule of `id` now.
    RunSchedule {
        id: String,
    },
    /// Keep the schedule of `id` no more; what it queued stays.
    DeleteSchedule {
        id: String,
    },
    /// Keep `target`, an address or a path, in the project holding `dir`; a
    /// relative path is one of `dir`. One the project keeps already keeps its
    /// identity and takes the label and note given anew.
    KeepResource {
        dir: PathBuf,
        target: String,
        label: Option<String>,
        note: Option<String>,
    },
    /// Keep the PNG `data`, in base64, as a file on the work machine and
    /// keep that file as a resource of the project holding `dir`.
    KeepScreenshot {
        dir: PathBuf,
        data: String,
        label: Option<String>,
        note: Option<String>,
    },
    /// Replace the label and the note of the resource of `id`.
    DescribeResource {
        id: String,
        label: Option<String>,
        note: Option<String>,
    },
    /// Keep the resource of `id` no more.
    DropResource {
        id: String,
    },
    PathEntries {
        root: Option<HostPath>,
        typed: String,
    },
    /// Read what the conversation `past` says, from the file its agent keeps
    /// it in. The host finds the conversation itself, by its agent, login,
    /// directory and id; the file the window names, if any, is not read.
    PastPreview {
        past: Box<tsumugi_model::Past>,
    },
    Apply {
        stamp: crate::MutationStamp,
        request: Box<crate::Request>,
    },
    MutationStatus {
        stamp: crate::MutationStamp,
    },
    MutationEpoch,
    SubmitPrompt {
        session: String,
        agent: Agent,
        text: String,
        images: Vec<PathBuf>,
    },
    /// Start a session with its program held back until the session is
    /// released.
    StartSession(Box<SessionStart>),
    /// Start a shell, a Neovim or a command with its program held back until
    /// the terminal is released.
    StartTerminal(Box<TerminalStart>),
    /// How the terminal host stands against this build.
    TerminalHost,
    /// Replace the terminal host with one of this build, ending every
    /// session it holds.
    RestartTerminalHost,
    /// Keep an image pasted into the prompt, of `mime`, its bytes in base64.
    KeepImage {
        mime: String,
        data: String,
    },
    /// Keep a copy of the image file at `path`, dropped on the prompt.
    KeepImageFile {
        path: PathBuf,
    },
    /// Make `path`, a directory when `dir` or else an empty file, with the
    /// directories it passes through; one already there is left as it is.
    MakeEntry {
        path: PathBuf,
        dir: bool,
    },
    /// Make the settings those `text` writes in `settings.toml`'s format.
    SaveSettings {
        text: String,
    },
    SaveSettingsChecked {
        text: String,
        expected: crate::FileText,
    },
    /// Make `text` the key bindings over the defaults, as `keys.toml` holds
    /// them.
    SaveKeys {
        text: String,
    },
    /// What is written in the prompts, as kept.
    Drafts,
    /// The review comments kept for a worktree, each judged against its
    /// file's diff as it is now.
    Review {
        worktree: PathBuf,
    },
    /// Change the review comments kept for a worktree; answered as `Review`
    /// is, after the change.
    ChangeReview {
        worktree: PathBuf,
        change: Box<tsumugi_model::review::Change>,
    },
    /// Keep what a window changed of what is written in the prompts.
    KeepDrafts(Box<DraftChanges>),
    ConsumeDraft {
        key: String,
        text: String,
        images: Vec<PathBuf>,
    },
    /// The image file at `path` as a `data:` URL: whole, or as the small copy
    /// a list draws.
    Picture {
        path: PathBuf,
        whole: bool,
    },
    /// One version of a picture a diff compares, as a `data:` URL: the file
    /// `path` of the worktree as Git has it in `rev`, or the worktree's own
    /// file when `rev` is none. Answered as `Picture` is.
    DiffPicture {
        worktree: PathBuf,
        rev: Option<String>,
        path: String,
    },
    /// Whether the profile `profile` is logged in, as the CLI it starts says
    /// where it runs it, asked once the terminal `after`, when one is named,
    /// has ended.
    LoginStanding {
        profile: tsumugi_model::profile::ProfileId,
        #[serde(default, with = "crate::wire::wide_opt")]
        after: Option<u128>,
    },
    /// Start a terminal that runs the CLI's own login where the profile
    /// `profile` runs it.
    OpenLogin {
        profile: tsumugi_model::profile::ProfileId,
        cols: u16,
        rows: u16,
    },
    /// Delete the folder of its own the profile `profile` keeps its login
    /// in, as the settings say now: one tsumugi made, in which no session
    /// runs.
    DeleteLogin {
        profile: tsumugi_model::profile::ProfileId,
    },
    /// Bring back what the terminal host took with it, chosen by the
    /// daemon ids it ran as.
    BringBack {
        ids: Vec<String>,
    },
    /// Let go of what the terminal host took with it, chosen by the daemon
    /// ids it ran as, so it is offered no more.
    LetGo {
        ids: Vec<String>,
    },
    /// Clone the repository `typed` names with `ghq get` in a terminal held
    /// back until it is released, which goes on showing what the clone
    /// wrote once it ends.
    Clone {
        typed: String,
        cols: u16,
        rows: u16,
    },
    /// Where ghq keeps the repository at `path` under its roots, asked once
    /// the terminal `after` that cloned it has ended.
    Cloned {
        path: String,
        #[serde(with = "crate::wire::wide")]
        after: u128,
    },
    /// Make a worktree of the repository at `repository` that starts at the
    /// head of its pull request `number` on GitHub.
    PullWorktree {
        repository: PathBuf,
        number: u64,
    },
    /// Where a worktree of the repository at `repository` may start: its
    /// checkout's commit and its branches.
    WorktreeStarts {
        repository: PathBuf,
    },
    /// Make the worktree `new` asks for in the repository at `repository`.
    CreateWorktree {
        repository: PathBuf,
        new: tsumugi_model::NewWorktree,
    },
    /// Set up the worktree made at `worktree` of the repository at
    /// `repository`: give it the ignored files and directories the primary
    /// checkout lists, and run its setup script as `choice` and the
    /// repository's run policy say. Asked again with a choice once a person was
    /// asked what an answer of `Asked` held.
    SetupWorktree {
        repository: PathBuf,
        worktree: PathBuf,
        choice: tsumugi_model::SetupChoice,
    },
    /// Put to sleep the sessions with these ids: their programs stop and
    /// their rows and tabs stay, as when they are left alone.
    Sleep {
        ids: Vec<String>,
    },
    /// How the worktree at `path` stands, as far as deleting it loses work.
    WorktreeStanding {
        path: PathBuf,
    },
    /// The worktrees of the repository at `repository` that a cleanup lists,
    /// the main checkout left out, with what is weighed before deleting each.
    CleanupEntries {
        repository: PathBuf,
    },
    /// Delete the worktree at `path`; one with changes only when `force`,
    /// and its branch as `branch` says.
    DeleteWorktree {
        path: PathBuf,
        force: bool,
        branch: tsumugi_model::BranchFate,
    },
    /// How merging the branch of the worktree at `path` into the main
    /// checkout stands. Reads only.
    MergeStanding {
        path: PathBuf,
    },
    /// Merge the branch of the worktree at `path` into the main checkout,
    /// as `changes` says to treat the main checkout's own changes, with
    /// `message` for the merge commit when given, and, with `close_issue`,
    /// close the local issue the worktree's session works on.
    MergeWorktree {
        path: PathBuf,
        changes: tsumugi_model::MainChanges,
        message: Option<String>,
        close_issue: bool,
    },
    /// Bring the main checkout's branch into the branch of the worktree at
    /// `path` and start a session there to resolve what conflicts, leaving
    /// the main checkout as it is.
    ResolveMerge {
        path: PathBuf,
    },
    /// Make `op` in the Git worktree at `worktree`.
    Git {
        worktree: PathBuf,
        op: GitOp,
    },
    /// Read the pull request or the issue `number` of the repository at
    /// `repository` whole, with what was said on it and the images shown.
    GitHubThread {
        repository: PathBuf,
        listing: tsumugi_model::github::Listing,
        number: u64,
    },
    /// Read afresh the commit the pull request `number` of the repository at
    /// `repository` ends at, and, with `checks`, its checks that failed or
    /// still run, the failed ones with the end of their logs. Reads only.
    GitHubFixContext {
        repository: PathBuf,
        number: u64,
        checks: bool,
    },
    /// Read the log of the Actions job `job` of the repository at
    /// `repository`, as far as it can be read. Reads only.
    GitHubCheckLog {
        repository: PathBuf,
        job: u64,
    },
    /// Write `write` on the pull request or the issue `number` of the
    /// repository at `repository`, as the login `gh` uses there.
    GitHubWrite {
        repository: PathBuf,
        listing: tsumugi_model::github::Listing,
        number: u64,
        write: tsumugi_model::github::Write,
    },
    /// Run the agent of `profile`, the default profile when none, once in the
    /// worktree on the prompt `template` makes for `action` from what the
    /// worktree holds, with `args` added, and answer with the text it printed
    /// as [`Answer::Told`]. Reads and runs the agent only: nothing is
    /// committed, pushed or sent.
    GenerateText {
        worktree: PathBuf,
        action: tsumugi_model::AiAction,
        profile: Option<tsumugi_model::profile::ProfileId>,
        args: Vec<String>,
        template: String,
    },
    /// Run the command `tsumugi <args>` as run in `cwd`, with `input` on its
    /// standard input.
    Run {
        args: Vec<String>,
        cwd: PathBuf,
        input: Option<String>,
    },
}

/// What is made in a Git worktree, each over the files `git status` listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GitOp {
    /// Put these changes in the index.
    Stage {
        changes: Vec<tsumugi_model::repository::Change>,
    },
    /// Take these changes out of the index.
    Unstage {
        changes: Vec<tsumugi_model::repository::Change>,
    },
    /// Lose these changes, staged or not.
    Discard {
        changes: Vec<tsumugi_model::repository::Change>,
    },
    /// Put every change of the worktree in the index.
    StageAll,
    /// Commit what is staged, or with `amend` replace the last commit.
    Commit { message: String, amend: bool },
    /// Push the branch, overwriting the remote's only if it has not moved
    /// since it was fetched when `lease` is set.
    Push { lease: bool },
    /// Pull the branch's upstream.
    Pull,
    /// Give up the merge, rebase, cherry-pick or revert in progress.
    Abort,
    /// Go on with the operation in progress once its conflicts are resolved.
    Resume,
    /// Compare the branch with `base` from now on, or with the one found
    /// when none is given.
    ChooseBase { base: Option<String> },
    /// Apply the commit `commit`, by its full id, to the branch as a commit
    /// of its own.
    CherryPick { commit: String },
    /// Undo the commit `commit`, by its full id, in a commit of its own.
    Revert { commit: String },
    /// Make the branch `name` at the commit `commit`, by its full id,
    /// without checking it out.
    CreateBranch { name: String, commit: String },
    /// Push the branch and open a pull request for it on GitHub, into `base`
    /// or the repository's default branch.
    CreatePull {
        base: Option<String>,
        title: String,
        body: String,
        draft: bool,
    },
}

/// A session to start.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStart {
    pub agent: Agent,
    /// The profile it starts from, which has to start `agent`; `agent`'s
    /// default profile when none is named.
    #[serde(default)]
    pub profile: Option<tsumugi_model::profile::ProfileId>,
    pub name: String,
    /// Whether the person starting it gave the name, which its agent then
    /// never changes; a name made from the prompt is the agent's to change.
    #[serde(default)]
    pub name_given: bool,
    /// Where it runs, or the draft's place a new folder is made under when
    /// it is the scratch folder.
    pub cwd: HostPath,
    /// Made from a draft, whose place may be the scratch folder.
    pub draft: bool,
    pub model: Option<String>,
    pub permissions: AgentPermissions,
    pub prompt: String,
    pub images: Vec<PathBuf>,
    /// The conversation it continues, by its id.
    pub resume: Option<String>,
    /// Whether it continues a copy of `resume`'s conversation, which its
    /// agent makes, and leaves the conversation itself as it was.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fork: bool,
    /// The folder of a profile's own that keeps the conversation it
    /// continues, which it has to run in; none for its agent's own
    /// environment.
    #[serde(default)]
    pub home: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// A shell, a Neovim or a command to start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalStart {
    pub name: String,
    pub cwd: HostPath,
    pub program: TerminalProgram,
    /// The text of the shell command a command terminal runs, given to the
    /// shell as one argument; none for any other terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// A repository review opened after the user's Neovim configuration loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<tsumugi_model::repository::Spot>,
    /// The file Neovim starts on.
    pub file: Option<PathBuf>,
    /// Start a floating terminal: a shell in the folder the work host's
    /// settings name, whatever `cwd` says.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub floating: bool,
    pub cols: u16,
    pub rows: u16,
}

/// How the terminal host stands against the work host's build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalHostStanding {
    /// None listens.
    Absent,
    /// It speaks a version of the terminal protocol this build does.
    Works,
    /// It speaks none: of the build it says, or none when it is too old to
    /// say which versions it speaks.
    Old { build: Option<String> },
}

impl Ask {
    /// Whether answering it waits on something that may take as long as the
    /// user likes, so the answer is worth nothing once the one who asked has
    /// hung up; every other ask is answered whether or not it is waited for.
    pub fn waits(&self) -> bool {
        match self {
            Self::LoginStanding { after, .. } => after.is_some(),
            Self::Cloned { .. } => true,
            Self::Run { args, .. } => {
                matches!(args.as_slice(), [terminal, wait, ..] if terminal == "terminal" && wait == "wait")
            }
            _ => false,
        }
    }
}

/// What came of an ask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Unconfirmed {
        text: String,
    },
    DraftConsumed {
        cleared: bool,
    },
    PathEntries {
        entries: Vec<crate::PathEntry>,
    },
    /// What the conversation says; none when the host cannot read it.
    PastPreview {
        preview: Option<tsumugi_model::Preview>,
    },
    Mutation {
        status: crate::MutationState,
    },
    MutationEpoch {
        epoch: u64,
    },
    /// The session started, by its id, held until released.
    Session {
        id: String,
    },
    /// The terminal started, held until released.
    Terminal {
        #[serde(with = "crate::wire::terminal")]
        terminal: TerminalSession,
    },
    TerminalHost {
        standing: TerminalHostStanding,
    },
    /// The image was kept here.
    Kept {
        path: PathBuf,
    },
    /// What Git told of a commit, push or pull that went through.
    Told {
        text: String,
    },
    /// The pull request that was opened.
    PullCreated {
        number: u64,
        url: String,
    },
    /// The worktree was made here.
    Worktree {
        path: PathBuf,
    },
    /// What came of setting up a worktree.
    WorktreeSetup {
        setup: tsumugi_model::WorktreeSetup,
    },
    /// Where a worktree may start.
    WorktreeStarts {
        starts: Vec<tsumugi_model::StartChoice>,
    },
    /// How a worktree stands.
    WorktreeStanding {
        standing: tsumugi_model::WorktreeStanding,
    },
    /// How merging a worktree's branch into the main checkout stands.
    MergeStanding {
        standing: tsumugi_model::MergeStanding,
    },
    /// What came of merging a worktree's branch into the main checkout.
    Merged {
        merged: tsumugi_model::Merged,
    },
    /// How many sessions slept, and the names of those that could not be woken
    /// again and so run on.
    Slept {
        count: usize,
        left: Vec<String>,
    },
    /// The worktrees a cleanup lists.
    CleanupEntries {
        entries: Vec<tsumugi_model::CleanupEntry>,
    },
    /// The worktree was deleted, with what became of its branch.
    WorktreeRemoved {
        removed: tsumugi_model::WorktreeRemoved,
    },
    /// The pull request or the issue asked for, read whole.
    Thread {
        thread: Box<tsumugi_model::github::Thread>,
    },
    /// What a request to fix a pull request needs, read afresh.
    FixContext {
        context: tsumugi_model::github::FixContext,
    },
    /// A check's job log, as far as it could be read.
    CheckLog {
        log: tsumugi_model::github::CheckLog,
    },
    /// The image asked for, or none when it cannot be read or shown.
    Picture {
        url: Option<String>,
    },
    /// What the command wrote, and its exit status.
    Ran {
        stdout: String,
        stderr: String,
        code: i32,
    },
    /// Whether a profile is logged in, as the CLI it starts said.
    LoginStanding {
        standing: tsumugi_model::profile::LoginStanding,
    },
    /// What came of bringing back each one chosen.
    BroughtBack {
        outcomes: Vec<tsumugi_model::BroughtBack>,
    },
    /// Where ghq keeps the repository cloned, or none when it keeps none
    /// there: the clone failed.
    Cloned {
        dir: Option<PathBuf>,
    },
    /// What is written in the prompts, as kept.
    Drafts(Box<Drafts>),
    /// The review comments of a worktree, in the order they were made.
    Review {
        comments: Vec<tsumugi_model::review::Judged>,
    },
    Done,
    /// It failed, for this reason, said to the user.
    Failed {
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(words: &[&str]) -> Ask {
        Ask::Run {
            args: words.iter().map(|w| w.to_string()).collect(),
            cwd: "/src".into(),
            input: None,
        }
    }

    #[test]
    fn waiting_for_a_clone_ends_with_the_one_who_asked() {
        assert!(
            Ask::Cloned {
                path: "github.com/o/r".into(),
                after: 1
            }
            .waits()
        );
    }

    #[test]
    fn waiting_for_a_terminal_s_output_ends_with_the_one_who_asked() {
        assert!(run(&["terminal", "wait", "--terminal", "x", "--text", "ready"]).waits());
        assert!(!run(&["terminal", "screen", "--terminal", "x"]).waits());
        assert!(!run(&["wait"]).waits());
        assert!(!run(&[]).waits());
    }
}
