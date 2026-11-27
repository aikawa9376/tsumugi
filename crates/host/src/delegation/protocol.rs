//! What a session asks the work host, one JSON line each way.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tsumugi_model::{Agent, AgentPermissions, MessageId, SessionCode, SessionId};

use super::{Code, Error, Result};

pub const VERSION: u32 = 1;
pub const MAX_FRAME: usize = 1024 * 1024;

#[cfg(unix)]
pub fn socket(root: &Path) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    let path = root.join("delegation.sock");
    if path.as_os_str().as_bytes().len() < 104 {
        return path;
    }
    // Only the endpoint is shortened; credentials and records stay in their private root.
    let mut hash = 0x6c62272e07bb014262b821756295c58du128;
    for byte in root.as_os_str().as_bytes() {
        hash = (hash ^ u128::from(*byte)).wrapping_mul(0x1000000000000000000013b);
    }
    let uid = tsumugi_sys::euid();
    PathBuf::from(format!("/tmp/tsumugi-{uid}/d{hash:032x}.sock"))
}

/// The socket in `root`, whose path no length limit holds to here.
#[cfg(windows)]
pub fn socket(root: &Path) -> PathBuf {
    root.join("delegation.sock")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub credential: String,
    pub command: Command,
    /// The Codex thread a tool was called from, which tells the session's own
    /// conversation from its subagents'.
    #[serde(default)]
    pub thread: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub version: u32,
    pub result: std::result::Result<Value, Error>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    Context,
    StartSession(Start),
    CreateLocalIssue(CreateLocalIssue),
    /// The local issues of the environment.
    LocalIssues(LocalIssues),
    ReadLocalIssue(LocalIssueName),
    CloseLocalIssue(LocalIssueName),
    ReopenLocalIssue(LocalIssueName),
    CancelLocalIssue(LocalIssueName),
    /// The resources the user keeps in the caller's project.
    Resources(Resources),
    AddResource(AddResource),
    RemoveResource(RemoveResource),
    Send(Send),
    Sessions(Sessions),
    /// The TCP ports the processes of sessions listen on.
    Ports(Ports),
    /// Ask the work host to run the repository's gate on the caller's commit.
    RequestGate(RequestGate),
    /// List, and clear away, the worktrees that are done with.
    CleanupWorktrees(CleanupWorktrees),
    /// Merge a finished worktree's branch into the main checkout, or read how that stands.
    MergeWorktree(MergeWorktree),
    Models,
    /// Drive a page of the browser of the caller's worktree.
    Browser(tsumugi_protocol::BrowserCommand),
    /// Say how the work of the caller's worktree stands, or read it.
    WorktreeCheckpoint(WorktreeCheckpoint),
    /// Read the review of the caller's worktree: its comments and notes.
    ReadReview(ReadReview),
    /// Leave a note on lines of a change of the caller's worktree, or change
    /// or delete one the caller left.
    Annotate(Annotate),
    /// A file the caller's agent is about to write, or has just written,
    /// with one of its own tools, told by the hooks around the tool.
    Writing(Writing),
    /// Name the caller's own session anew.
    Rename(Rename),
    /// End the caller's own session, once its user agreed, or a session the
    /// caller opened.
    Close(CloseSession),
    /// Tell the session that supervises the caller how its work ended.
    ReportDone(ReportDone),
    /// Block until a message arrives for the caller, and take it.
    Wait(Wait),
    /// What the caller's agent is to weigh as a prompt arrives, or once its
    /// conversation was cleared. Still read under its former name, which
    /// hooks of an Tsumugi installed before send to a work host started since.
    #[serde(alias = "naming")]
    Prompted {
        cleared: bool,
    },
    /// Record changes to the caller's timeline.
    TimelineTasks(TimelineTasks),
    /// The caller's timeline as it stands.
    Timeline,
    /// The oldest message waiting for the caller, as its agent is to read it.
    Next,
    Delivered {
        id: MessageId,
    },
}

impl Command {
    /// Whether the command acts for the session rather than reading.
    pub fn acts(&self) -> bool {
        matches!(
            self,
            Self::StartSession(_)
                | Self::CreateLocalIssue(_)
                | Self::CloseLocalIssue(_)
                | Self::ReopenLocalIssue(_)
                | Self::CancelLocalIssue(_)
                | Self::AddResource(_)
                | Self::RemoveResource(_)
                | Self::Send(_)
                | Self::Rename(_)
                | Self::Close(_)
                | Self::ReportDone(_)
                | Self::RequestGate(_)
                | Self::CleanupWorktrees(_)
                | Self::MergeWorktree(_)
                | Self::WorktreeCheckpoint(_)
                | Self::Annotate(_)
                | Self::Wait(_)
                | Self::Browser(
                    tsumugi_protocol::BrowserCommand::Goto { .. }
                        | tsumugi_protocol::BrowserCommand::Click { .. }
                        | tsumugi_protocol::BrowserCommand::Fill { .. }
                        | tsumugi_protocol::BrowserCommand::TabCreate { .. }
                        | tsumugi_protocol::BrowserCommand::TabSwitch { .. }
                        | tsumugi_protocol::BrowserCommand::SetDevice { .. }
                        | tsumugi_protocol::BrowserCommand::CaptureStart
                        | tsumugi_protocol::BrowserCommand::CaptureStop,
                )
        )
    }
}

/// Point the page of your worktree's browser at an address.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserGoto {
    /// The address to open: a web page or a development server, such as `http://localhost:3000`; `localhost` is the machine your worktree is on.
    pub url: String,
}

/// Read the page as a tree of what is on it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserSnapshot {
    /// Only the controls and links.
    #[serde(default)]
    pub interactive: bool,
    /// Only what changed since the last snapshot taken with `delta`.
    #[serde(default)]
    pub delta: bool,
    /// Only the part of the page below the first element this CSS selector matches.
    #[serde(default)]
    pub selector: Option<String>,
    /// Only this many levels of the tree.
    #[serde(default)]
    pub depth: Option<u32>,
}

/// Click an element of the page.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserClick {
    /// The reference the last snapshot gave the element, such as `@e3`.
    pub element: String,
}

/// Replace what a field of the page holds.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserFill {
    /// The reference the last snapshot gave the field, such as `@e1`.
    pub element: String,
    /// What the field is to hold.
    pub value: String,
}

/// Wait until the page is as given.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserWait {
    /// Wait for an element this CSS selector matches.
    #[serde(default)]
    pub selector: Option<String>,
    /// Whether the element of `selector` is to be `visible` (the default) or `hidden`.
    #[serde(default)]
    pub state: Option<WaitState>,
    /// Wait for this text to show on the page.
    #[serde(default)]
    pub text: Option<String>,
    /// Wait for the page's address to match this pattern, `*` standing for anything but a slash and `**` for anything, such as `**/dashboard`.
    #[serde(default)]
    pub url: Option<String>,
    /// Wait for the page to have loaded this far: `load`, `domcontentloaded`, or `networkidle` (loaded, and no request for half a second).
    #[serde(default)]
    pub load: Option<WaitLoad>,
    /// How long to wait for the conditions, in milliseconds, 25000 when omitted; with no condition given, how long to pause. At most 300000.
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Whether the element waited for is to be seen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaitState {
    Visible,
    Hidden,
}

/// How far the page is to have loaded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WaitLoad {
    Load,
    DomContentLoaded,
    NetworkIdle,
}

/// What a picture is encoded as.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PictureFormat {
    Png,
    Jpeg,
}

impl From<WaitState> for tsumugi_protocol::ElementState {
    fn from(state: WaitState) -> Self {
        match state {
            WaitState::Visible => Self::Visible,
            WaitState::Hidden => Self::Hidden,
        }
    }
}

impl From<WaitLoad> for tsumugi_protocol::LoadState {
    fn from(load: WaitLoad) -> Self {
        match load {
            WaitLoad::Load => Self::Load,
            WaitLoad::DomContentLoaded => Self::DomContentLoaded,
            WaitLoad::NetworkIdle => Self::NetworkIdle,
        }
    }
}

impl From<PictureFormat> for tsumugi_protocol::ImageFormat {
    fn from(format: PictureFormat) -> Self {
        match format {
            PictureFormat::Png => Self::Png,
            PictureFormat::Jpeg => Self::Jpeg,
        }
    }
}

/// The latest entries of what the page wrote or asked for.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserLimit {
    /// How many of the latest entries to give: 100 when omitted, at most 500.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Show the page as a device does.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserSetDevice {
    /// The name of a size the browser offers, such as `Mobile M`, `Tablet` or `Laptop L`; `Responsive` shows the page at the size of its room again. An unknown name is refused with the names there are.
    pub name: String,
}

/// Take a picture of the whole page.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserFullScreenshot {
    /// `png` (the default) or `jpeg`.
    #[serde(default)]
    pub format: Option<PictureFormat>,
}

/// Open a page in your worktree's browser.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserTabCreate {
    /// The address to open it at; a blank page when omitted.
    #[serde(default)]
    pub url: Option<String>,
}

/// Make another page of your worktree's browser the one commands act on.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserTabSwitch {
    /// The page's place in the list `browser_tab_list` gives, counted from 0.
    pub index: usize,
}

/// Save work or research findings for the user to start later.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateLocalIssue {
    pub title: String,
    /// The research findings, context and work to do when this issue is started.
    pub body: String,
    /// The working directory on this work machine; the caller's when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// List the local issues of this environment.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalIssues {
    /// Only issues in this state: open, queued, starting, running, waiting, done, closed, failed or cancelled; every state when omitted.
    #[serde(default)]
    pub state: Option<String>,
    /// Only issues of this directory or below it; every directory when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// Name one local issue.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LocalIssueName {
    /// Its number as `local_issues` lists it, or the start of its id.
    pub issue: String,
}

/// Look up the paths and addresses the user keeps in the project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    /// Words each resource's title, path or address, or note must hold, in any case, for example `nginx conf`; every resource when omitted.
    #[serde(default)]
    pub query: Option<String>,
    /// A directory of another project on this work machine; your own project when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// Keep a path or an address in the project, where the user and other sessions find it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddResource {
    /// An address, or the path of a file or folder, absolute or relative to your directory. Only the reference is kept, never a copy.
    pub target: String,
    /// A short title, such as the site's or the file's name in the user's words.
    #[serde(default)]
    pub label: Option<String>,
    /// What it is for, so a later search by words finds it.
    #[serde(default)]
    pub note: Option<String>,
    /// A directory of another project on this work machine; your own project when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// Stop keeping a resource.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveResource {
    /// Its id, as `resources` lists it.
    pub id: String,
}

/// End a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloseSession {
    /// The code or id of a session you opened with `start_session`, to end it; your own session when omitted. A session the user or another session opened is refused.
    #[serde(default)]
    pub target: Option<String>,
    /// End the session although it works or has uncommitted changes, which otherwise refuse the call and are named in the refusal.
    #[serde(default)]
    pub force: bool,
}

/// Open a session beside you with a first prompt.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Start {
    /// The complete first prompt: the line of work and all it needs to know of what you know. Optional with `resume`, where it is sent to the resumed session as a message; with `issue` it follows the issue's own text.
    #[serde(default)]
    pub prompt: String,
    /// The code, id or conversation id of an ended session to bring back with its conversation, in its own directory, agent and profile: `dir`, `worktree`, `base`, `agent` and `profile` are refused with it. Its record stays as it was.
    #[serde(default)]
    pub resume: Option<String>,
    /// The number of a local issue (see `local_issues`) the session works on: the issue is marked as worked on, finished when the session's turn ends, and opens the session from the Issues list. It starts in the issue's directory unless `dir` is given.
    #[serde(default)]
    pub issue: Option<String>,
    /// The directory to work in, or to make the worktree of; your own when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// Work in a new Git worktree of that directory.
    #[serde(default)]
    pub worktree: bool,
    /// The revision the new worktree starts from; your current commit when omitted.
    #[serde(default)]
    pub base: Option<String>,
    /// `claude`, `codex`, `pi` or `opencode`; the agent of `profile`, else of the default profile, when omitted.
    #[serde(default)]
    pub agent: Option<Agent>,
    /// The profile it starts from, by id (see `models`); the agent's default profile when omitted, never your own.
    #[serde(default)]
    pub profile: Option<tsumugi_model::profile::ProfileId>,
    /// A model the agent lists (see `models`); its default when omitted.
    #[serde(default)]
    pub model: Option<String>,
    /// The size of the work, `light`, `standard` or `heavy`: the user's settings say which profile and model each starts. It stands in for `agent`, `profile` and `model`, which it refuses beside it, and it fails, starting nothing, when the user has not defined the tier or its model is not offered.
    #[serde(default)]
    pub tier: Option<tsumugi_core::settings::Tier>,
    #[serde(default)]
    pub permissions: Option<AgentPermissions>,
    /// A short name; made from the prompt when omitted.
    #[serde(default)]
    pub name: Option<String>,
    /// Keep it out of the user's sight: it is only listed, and no tab opens for it. Otherwise it opens in a tab, with the keys while the user is looking at you.
    #[serde(default)]
    pub background: bool,
    /// Supervise it as a worker: it reports to you with `report_done` once its work is done, and you are told when it stops without one, so you can `wait` for it instead of polling. The user still sees and steers it.
    #[serde(default)]
    pub supervised: bool,
    /// With `issue`, try it several ways at once: one session for each entry, each in a new worktree of the issue's directory made from the commit it is at, started from the entry's profile and model and opened by you (supervised by you with `supervised`). The user compares them and merges the one to keep. `dir`, `worktree`, `base`, `agent`, `profile`, `model`, `tier` and `resume` are refused beside it. Answered with the issue and its attempts.
    #[serde(default)]
    pub attempts: Vec<tsumugi_model::TryAs>,
}

/// How a worker's work ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
}

impl Outcome {
    pub fn word(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// Tell the session that opened you to supervise you how your work ended.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportDone {
    pub outcome: Outcome,
    /// What was done, what was found, and what is left, in a few lines.
    pub summary: String,
    /// The files you modified, as paths.
    #[serde(default)]
    pub files: Vec<String>,
}

/// Wait for a message to reach you.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wait {
    /// How long to wait at most; 300 when omitted, and no more than 3600.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// Send a message to another session.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Send {
    /// The recipient's code, or its id, as `sessions` lists them.
    pub to: String,
    pub body: String,
}

/// Read the review of a worktree.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadReview {
    /// The worktree's directory on this work machine; the one you work in when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// A note on lines of a change, for the person reviewing it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Annotate {
    /// The file, as its path from the top of the worktree.
    #[serde(default)]
    pub path: Option<String>,
    /// `new` for lines as they are after the change, the default; `old` for lines the change removed.
    #[serde(default)]
    pub side: Option<String>,
    /// The first line, numbered on that side.
    #[serde(default)]
    pub start: Option<u32>,
    /// The last line; the first when omitted.
    #[serde(default)]
    pub end: Option<u32>,
    /// What the note says: why the lines are as they are, or what to look at.
    #[serde(default)]
    pub text: Option<String>,
    /// The change the lines are of: `branch`, the default, is the worktree against the branch's base, as the reviewer opens it; `uncommitted` is what is not committed; anything else names a commit.
    #[serde(default)]
    pub change: Option<String>,
    /// A note of yours to change to `text`, or to delete with `delete`.
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub delete: bool,
    /// The worktree's directory on this work machine; the one you work in when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// A file an agent writes with one of its tools, read before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Writing {
    pub path: PathBuf,
    /// The tool's call, which ties the read after to the one before.
    pub call: String,
    /// Read after the write rather than before it.
    pub done: bool,
}

/// Say how the work of a worktree stands, for the user and other sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorktreeCheckpoint {
    /// One line: what just happened, where, and the next step.
    #[serde(default)]
    pub comment: Option<String>,
    /// todo, in-progress, in-review or completed.
    #[serde(default)]
    pub status: Option<String>,
    /// Empty the comment.
    #[serde(default)]
    pub clear_comment: bool,
    /// Empty the status.
    #[serde(default)]
    pub clear_status: bool,
    /// The worktree's directory on this work machine; the one you work in when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

impl WorktreeCheckpoint {
    /// Whether it changes what the worktree says, rather than reading it.
    pub fn acts_on_a_worktree(&self) -> bool {
        self.comment.is_some() || self.status.is_some() || self.clear_comment || self.clear_status
    }
}

/// Name your own session anew.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    /// A few words saying what this session works on, in the language the user writes in.
    pub name: String,
}

/// Change the tasks of your timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineTasks {
    /// Applied in the order given.
    pub ops: Vec<tsumugi_model::timeline::Op>,
}

/// List the sessions Tsumugi runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sessions {
    /// Only sessions working in the repository holding this directory.
    #[serde(default)]
    pub repo: Option<PathBuf>,
    /// Only sessions working in this directory or below it.
    #[serde(default)]
    pub dir: Option<PathBuf>,
}

/// List the TCP ports the processes of sessions listen on.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    /// The worktree whose sessions' ports to list, by a directory in it; your own worktree when omitted.
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    /// Every session's ports on this work machine, whatever worktree it works in.
    #[serde(default)]
    pub all: bool,
}

/// Have the work host run the repository's gate on your commit.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestGate {
    /// A directory of the repository to check, in your worktree; your own directory when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// How long to wait for the answer at most; 600 when omitted, and no more than 3600. Asking again joins the run that goes on.
    #[serde(default)]
    pub wait_seconds: Option<u64>,
}

/// List the worktrees of a repository that are done with, and optionally remove them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanupWorktrees {
    /// A directory of the repository; your own directory when omitted.
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// Remove the worktrees that have nothing to lose and no session in them; only list them when false or omitted.
    #[serde(default)]
    pub remove: bool,
}

/// Merge the branch of a finished worktree into the main checkout, or read how that merge stands.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MergeWorktree {
    /// A directory of the worktree whose branch is merged; your own directory when omitted. Not with `branch`.
    #[serde(default)]
    pub dir: Option<PathBuf>,
    /// The branch to merge, found among the repository's worktrees. Not with `dir`.
    #[serde(default)]
    pub branch: Option<String>,
    /// Only read how the merge stands: the branch and the one it merges into, the commits it brings, the files that would conflict, the uncommitted changes in the main checkout, and the local issue it would close. Nothing is changed.
    #[serde(default)]
    pub check: bool,
    /// What to do with uncommitted changes in the main checkout: `refuse` (the default) merges nothing while there are some, `stash` sets them aside in a stash that is kept, `commit` commits them first. Choose `stash` or `commit` only where your discretion lets you merge over uncommitted changes; otherwise ask the user.
    #[serde(default)]
    pub changes: tsumugi_model::MainChanges,
    /// The merge commit's message; Git's own when omitted.
    #[serde(default)]
    pub message: Option<String>,
    /// Close the local issue the worktree's session was started for once the merge is made.
    #[serde(default)]
    pub close_issue: bool,
    /// Delete the worktree and its merged branch afterwards, as a cleanup does: never one with changes, and never one a session or terminal still works in.
    #[serde(default)]
    pub remove: bool,
    /// Instead of merging into the main checkout, bring the main checkout's branch into the branch in its own worktree and start a session there to resolve what conflicts; the main checkout is not touched. For a merge that answered with conflicts.
    #[serde(default)]
    pub resolve: bool,
}

/// A session that opened another, as the one it opened is told of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opener {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
}

impl Opener {
    pub(crate) fn of(session: &super::Session) -> Self {
        Self {
            id: session.id,
            code: session.code.clone(),
            name: session.name.clone(),
        }
    }
}

/// The session that opened the caller, and whether it has ended.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parent {
    #[serde(flatten)]
    pub opener: Opener,
    pub ended: bool,
}

/// The answer to [`Command::Context`]: the caller's own session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Caller {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
    pub dir: PathBuf,
    pub opened_by: Option<Parent>,
}

/// The answer to [`Command::StartSession`]: the session started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Started {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
    pub dir: PathBuf,
}

/// One session of the answer to [`Command::Sessions`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    pub id: SessionId,
    pub code: SessionCode,
    pub name: String,
    pub agent: Agent,
    pub dir: PathBuf,
    /// The repository it works in, by its common directory.
    pub repository: Option<PathBuf>,
    pub readiness: super::Readiness,
    pub opened_by: Option<Opener>,
    /// Whether Tsumugi stopped its program to sleep; sending to it wakes it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub asleep: bool,
}

/// The answer to [`Command::Send`]: the message sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sent {
    pub sent: MessageId,
}

/// The answer to [`Command::Next`]: a message to hand the caller's agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: MessageId,
    /// The session that sent it; none for a tick of a schedule.
    pub from: Option<SessionId>,
    /// The message as its recipient's agent reads it.
    pub content: String,
}

/// The answer to [`Command::ReportDone`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reported {
    pub reported: MessageId,
}

/// The answer to [`Command::Wait`]: the oldest message waiting, now taken, or
/// none when the time ran out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Waited {
    pub message: Option<Delivery>,
    pub timed_out: bool,
}

/// The answer to [`Command::Rename`]: the name taken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Renamed {
    pub name: String,
}

/// The answer to [`Command::Close`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Closing {
    pub closing: bool,
}

/// The answer to [`Command::Close`] naming a session the caller opened: the
/// session ended, and the user told so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Closed {
    pub closed: SessionId,
    pub code: SessionCode,
    pub name: String,
}

/// The answer to [`Command::Prompted`]: what the caller's agent is told, if
/// anything.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub note: Option<String>,
}

pub(crate) async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await.map_err(Error::storage)?;
        if available.is_empty() {
            return Err(Error::new(Code::Invalid, "the frame is incomplete"));
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(available.len(), |index| index + 1);
        if frame.len() + count > MAX_FRAME {
            return Err(Error::new(Code::Invalid, "the frame is too large"));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(frame);
        }
    }
}

pub(crate) async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(Error::storage)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await.map_err(Error::storage)
}

/// Asks the work host serving `root` as the session `credential` names.
pub async fn ask(
    root: &Path,
    credential: &str,
    command: Command,
    thread: Option<String>,
) -> Result<Value> {
    let stream = connect(&socket(root)).await.map_err(|error| {
        Error::new(
            Code::Storage,
            format!("cannot reach the work host: {error}"),
        )
    })?;
    let (read, mut write) = tokio::io::split(stream);
    write_frame(
        &mut write,
        &Request {
            version: VERSION,
            credential: credential.to_owned(),
            command,
            thread,
        },
    )
    .await?;
    let frame = read_frame(&mut BufReader::new(read)).await?;
    let response: Response = serde_json::from_slice(&frame).map_err(Error::storage)?;
    response.result
}

/// [`ask`], its answer read as `T`.
pub async fn ask_for<T: serde::de::DeserializeOwned>(
    root: &Path,
    credential: &str,
    command: Command,
    thread: Option<String>,
) -> Result<T> {
    let answer = ask(root, credential, command, thread).await?;
    serde_json::from_value(answer).map_err(|error| {
        Error::new(
            Code::Storage,
            format!("the work host's answer is not understood: {error}"),
        )
    })
}

/// How long a request waits for a work host to listen again, as one being
/// replaced is stopped and its successor started.
const REPLACING: std::time::Duration = std::time::Duration::from_secs(10);

/// Connect to `socket`, waiting while no work host listens there.
async fn connect(socket: &Path) -> std::io::Result<tsumugi_sys::local::Stream> {
    let until = tokio::time::Instant::now() + REPLACING;
    loop {
        match tsumugi_sys::local::connect(socket).await {
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) && tokio::time::Instant::now() < until =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            connected => return connected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire(answer: impl Serialize) -> Value {
        serde_json::to_value(answer).unwrap()
    }

    fn opener() -> Opener {
        Opener {
            id: "6efede4a-8866-4d1a-9431-07c591be83ba".parse().unwrap(),
            code: "57u7".parse().unwrap(),
            name: "lead".into(),
        }
    }

    #[test]
    fn each_answer_reads_on_the_wire_as_agents_were_told_it_does() {
        let id: tsumugi_model::SessionId = "e0131a25-f7be-40d6-838e-946265fbada4".parse().unwrap();
        let code: tsumugi_model::SessionCode = "ku8c".parse().unwrap();
        assert_eq!(
            wire(Caller {
                id,
                code: code.clone(),
                name: "review".into(),
                dir: "/repo".into(),
                opened_by: Some(Parent {
                    opener: opener(),
                    ended: false
                }),
            }),
            json!({"id": id, "code": "ku8c", "name": "review", "dir": "/repo",
                "opened_by": {"id": opener().id, "code": "57u7", "name": "lead", "ended": false}})
        );
        assert_eq!(
            wire(Started {
                id,
                code: code.clone(),
                name: "review".into(),
                dir: "/repo".into(),
            }),
            json!({"id": id, "code": "ku8c", "name": "review", "dir": "/repo"})
        );
        assert_eq!(
            wire(Listed {
                id,
                code,
                name: "review".into(),
                agent: Agent::Codex,
                dir: "/repo".into(),
                repository: None,
                readiness: super::super::Readiness::Ready,
                opened_by: Some(opener()),
                asleep: false,
            }),
            json!({"id": id, "code": "ku8c", "name": "review", "agent": "codex",
                "dir": "/repo", "repository": null, "readiness": "ready",
                "opened_by": {"id": opener().id, "code": "57u7", "name": "lead"}})
        );
        let message = MessageId::new();
        assert_eq!(wire(Sent { sent: message }), json!({"sent": message}));
        assert_eq!(
            wire(Delivery {
                id: message,
                from: Some(id),
                content: "hi".into()
            }),
            json!({"id": message, "from": id, "content": "hi"})
        );
        assert_eq!(
            wire(Renamed {
                name: "review-2".into()
            }),
            json!({"name": "review-2"})
        );
        assert_eq!(wire(Closing { closing: true }), json!({"closing": true}));
        assert_eq!(wire(Note { note: None }), json!({"note": null}));
    }

    #[tokio::test]
    async fn a_frame_written_is_read_back_as_one_line() {
        let mut written = Vec::new();

        write_frame(&mut written, &Value::from("hello"))
            .await
            .unwrap();

        assert_eq!(written, b"\"hello\"\n");
        let read = read_frame(&mut written.as_slice()).await.unwrap();
        assert_eq!(read, written);
    }

    #[tokio::test]
    async fn a_request_made_while_the_work_host_is_replaced_waits_for_the_new_one() {
        let dir = tempfile::Builder::new()
            .prefix("tsumugi-")
            .tempdir_in(tsumugi_sys::local::temp_dir())
            .unwrap();
        let root = dir.path().to_path_buf();
        let replacing = {
            let root = root.clone();
            crate::environment::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                let listener = tsumugi_sys::local::Listener::bind(socket(&root)).unwrap();
                let stream = listener.accept().await.unwrap();
                let (read, mut write) = tokio::io::split(stream);
                read_frame(&mut BufReader::new(read)).await.unwrap();
                let answer = Response {
                    version: VERSION,
                    result: Ok(Value::from("answered")),
                };
                write_frame(&mut write, &answer).await.unwrap();
            })
        };
        let answer = ask(&root, "credential", Command::Context, None).await;
        assert_eq!(answer.unwrap(), Value::from("answered"));
        replacing.await.unwrap();
    }

    #[test]
    fn a_close_naming_no_session_is_the_close_hooks_of_an_earlier_tsumugi_send() {
        let old: Command = serde_json::from_value(json!({"op": "close"})).unwrap();
        assert!(matches!(
            old,
            Command::Close(CloseSession {
                target: None,
                force: false
            })
        ));
        let named: Command =
            serde_json::from_value(json!({"op": "close", "target": "k7f3", "force": true}))
                .unwrap();
        assert!(matches!(
            named,
            Command::Close(CloseSession {
                target: Some(_),
                force: true
            })
        ));
        assert!(Command::Close(CloseSession::default()).acts());
    }

    #[test]
    fn a_start_resumes_or_works_an_issue_with_a_prompt_that_may_be_left_out() {
        let start = |value: Value| serde_json::from_value::<Start>(value);
        let resumed = start(json!({"resume": "k7f3"})).unwrap();
        assert_eq!(resumed.resume.as_deref(), Some("k7f3"));
        assert!(resumed.prompt.is_empty());
        assert_eq!(
            start(json!({"issue": "3"})).unwrap().issue.as_deref(),
            Some("3")
        );
    }

    #[test]
    fn a_start_opens_in_the_window_unless_asked_to_stay_in_the_background() {
        let start = |value: Value| serde_json::from_value::<Start>(value);
        assert!(
            !start(serde_json::json!({"prompt": "go"}))
                .unwrap()
                .background
        );
        let background = start(serde_json::json!({"prompt": "go", "background": true}));
        assert!(background.unwrap().background);
        assert!(start(serde_json::json!({"prompt": "go", "show": true})).is_err());
    }
}

#[cfg(all(test, unix))]
mod environment_socket_tests {
    use super::*;
    #[test]
    fn long_profile_roots_have_short_distinct_sockets() {
        use std::os::unix::ffi::OsStrExt;
        let root = PathBuf::from(format!(
            "/Users/person/.local/state/tsumugi/environments/{}/state/delegation",
            "a".repeat(32)
        ));
        assert!(socket(&root).as_os_str().as_bytes().len() < 104);
        assert_ne!(socket(&root), socket(&root.with_file_name("other")));
        assert_eq!(
            socket(Path::new("/tmp/short")),
            Path::new("/tmp/short/delegation.sock")
        );
    }
}
