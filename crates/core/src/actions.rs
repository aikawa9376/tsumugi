//! Every action a key, a menu entry or a click can run, each described once:
//! the name the file of bindings calls it by, what the menu bar and the
//! right-click menus call it, and the icon it is shown with.

use std::fmt;
use std::mem::discriminant;
use std::str::FromStr;

use crate::icons::Icon;
use crate::keymap::{Chord, Error};

/// What a key does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Nothing, in place of a binding taken away.
    None,
    WindowLeft,
    WindowDown,
    WindowUp,
    WindowRight,
    WindowNext,
    WindowPrev,
    WindowTop,
    WindowBottom,
    WindowBack,
    WindowSplit,
    WindowVsplit,
    WindowNew,
    WindowQuit,
    WindowOnly,
    WindowMoveLeft,
    WindowMoveDown,
    WindowMoveUp,
    WindowMoveRight,
    WindowExchange,
    WindowRotate,
    WindowEqualize,
    WindowNarrower,
    WindowWider,
    WindowTaller,
    WindowShorter,
    WindowMaximizeHeight,
    WindowMaximizeWidth,
    WindowToTab,
    WindowEdit,
    TabMergeLast,
    TabNext,
    TabPrev,
    TabLast,
    TabMovePrev,
    TabMoveNext,
    TabRecent,
    TabRecentPrev,
    TabNextSame,
    TabPrevSame,
    TabCloseFiles,
    CursorDown,
    CursorUp,
    CursorFirst,
    CursorLast,
    CursorGo,
    CursorSplit,
    CursorVsplit,
    CursorTab,
    CursorFold,
    CursorClose,
    CursorCopy,
    CursorReveal,
    CursorBrowse,
    FilerDown,
    FilerUp,
    FilerInto,
    FilerOut,
    FilerOpen,
    FilerHidden,
    FileNew,
    DirectoryNew,
    WaitingNext,
    WaitingPrev,
    SessionNext,
    SessionPrev,
    WorkspaceNext,
    WorkspacePrev,
    SessionPick,
    SessionNew,
    SessionScratch,
    SessionResume,
    SessionBringBack,
    SessionRestart,
    NoticesOpen,
    SessionStop,
    SessionRename,
    SessionMarkUnread,
    NotificationsMute,
    SessionProfileNext,
    SessionProfilePrev,
    ShellOpen,
    TerminalStop,
    NeovimOpen,
    BrowserOpen,
    BrowserFind,
    BrowserReopen,
    TabClose,
    EnvironmentAdd,
    EnvironmentRemove,
    EnvironmentRename,
    EnvironmentRetry,
    EnvironmentPair,
    WorkspaceAdd,
    LocalIssues,
    LocalIssuesLeader,
    SavedCommands,
    ResourceAdd,
    ResourceEdit,
    ResourceRemove,
    ResourceKeep,
    ResourceScreenshot,
    WorkspaceRemove,
    WorktreeClose,
    WorktreeNew,
    WorktreeMerge,
    WorktreeDelete,
    WorktreeCleanup,
    WorktreeSleep,
    WorktreeWake,
    WorkspacePin,
    TreeSort,
    TreeHideSleeping,
    TreeHideIdle,
    TreeHideMain,
    TreeRevealCurrent,
    WindowClose,
    WindowOpen,
    Refresh,
    Filter,
    WorktreesToggle,
    ScrollDown,
    ScrollUp,
    ScrollHalfDown,
    ScrollHalfUp,
    ScrollTop,
    ScrollBottom,
    ConsoleSendNext,
    ConsolePassEveryKey,
    ConsoleRead,
    PromptFocus,
    PromptWrite,
    PromptSend,
    PromptImages,
    PromptTakeSuggestion,
    HistoryPrev,
    HistoryNext,
    SettingsOpen,
    DaemonRestart,
    KeysHelp,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    TreeToggle,
    TreeFocus,
    SidebarNext,
    AgentsExpand,
    AgentsCollapse,
    AgentsReveal,
    DetailsToggle,
    DetailsStage,
    DetailsStageAll,
    DetailsDiscard,
    DetailsCommit,
    DetailsAmend,
    DetailsPull,
    DetailsPush,
    DetailsForcePush,
    DetailsCreatePull,
    DetailsDraftPull,
    DetailsGenerateCommit,
    DetailsFixCommit,
    DetailsAbort,
    DetailsResume,
    DetailsBase,
    DetailsResolve,
    DetailsNeovim,
    LogCopyHash,
    LogOpenRemote,
    LogExplain,
    LogBranch,
    LogWorktree,
    LogCherryPick,
    LogRevert,
    ViewerZoomIn,
    ViewerZoomOut,
    ViewerZoomFit,
    ViewerPageNext,
    ViewerPagePrev,
    ViewerPlay,
    ViewerBack,
    ViewerForward,
    ViewerCopyPath,
    ReviewComment,
    ReviewSelect,
    ReviewEdit,
    ReviewDelete,
    ReviewResolve,
    ReviewSend,
    ReviewDraft,
    DiffNextHunk,
    DiffPrevHunk,
    DiffNextFile,
    DiffPrevFile,
    DiffWrap,
    DiffFiles,
    DiffPictures,
    GithubNext,
    GithubPrev,
    GithubComment,
    GithubReply,
    GithubResolve,
    GithubReact,
    GithubReview,
    GithubFixChecks,
    GithubFixThreads,
    GithubMerge,
    GithubMergeSquash,
    GithubMergeCommit,
    GithubMergeRebase,
    GithubAutoMerge,
    GithubClose,
    GithubReady,
    CheckRerun,
    BoardOpen,
    BoardLeft,
    BoardRight,
    BoardDown,
    BoardUp,
    BoardFirst,
    BoardLast,
    BoardEnter,
    BoardIdle,
    BoardGroup,
    BoardExpand,
    BoardProject,
    BoardState,
    BoardClear,
    BoardSearch,
    LightboxNext,
    LightboxPrev,
    LightboxZoomIn,
    LightboxZoomOut,
    LightboxZoomFit,
    LightboxPanLeft,
    LightboxPanRight,
    LightboxPanUp,
    LightboxPanDown,
    LightboxClose,
    FloatingToggle,
    FloatingNew,
    FloatingCloseTab,
    FloatingNext,
    FloatingPrev,
    OverlayClose,
    OverlayDown,
    OverlayUp,
    OverlayChoose,
    /// Send a key to the session: the one named, or the one pressed.
    ConsoleSend(Option<Chord>),
    /// Show the content's tab of this number, counted from 1.
    TabGo(u8),
    DetailsTab(u8),
    /// Move the focused window into the content's tab of this number,
    /// counted from 1.
    TabMerge(u8),
}

/// Which of a right-click menu's groups an entry is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Going to what was clicked, or doing something with it.
    Go,
    /// Starting something beside it.
    Start,
    /// Reaching the work machine it works on.
    Reach,
    /// Ending or closing it.
    End,
}

/// What a right-click menu offering an action calls it, and where it draws
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub group: Group,
}

/// One action, as everything that shows or runs it knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    /// The action; one with an argument stands for every argument.
    pub action: Action,
    /// What the file of bindings calls it.
    pub name: &'static str,
    /// What it does, in full, as the key reference says it: Markdown
    /// starting in lower case, its links naming the files of `docs/spec/`.
    pub does: &'static str,
    /// What the menu bar calls it; `None` for an action that is only a key.
    pub bar: Option<&'static str>,
    /// What a right-click menu calls it, unless it says what it does there
    /// in words of its own.
    pub row: Option<Row>,
    /// What the keys help calls it, where neither menu does, or where what
    /// they call it says too little among the keys.
    pub says: Option<&'static str>,
    pub icon: Option<Icon>,
    /// What it steps through, if it does: the last key of a sequence bound
    /// to it runs it again alone, and so do the keys of the actions that
    /// step through the same.
    pub steps: Option<Steps>,
}

/// What a repeating action steps through. Repeating keeps to one of them,
/// so a key that steps through another begins what it begins instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Steps {
    WindowSize,
    Tabs,
    TabOrder,
    Waiting,
    Sessions,
    Workspaces,
}

impl ActionSpec {
    const fn new(action: Action, name: &'static str, does: &'static str) -> Self {
        ActionSpec {
            action,
            name,
            does,
            bar: None,
            row: None,
            says: None,
            icon: None,
            steps: None,
        }
    }

    const fn bar(mut self, label: &'static str, icon: Icon) -> Self {
        self.bar = Some(label);
        self.icon = Some(icon);
        self
    }

    const fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    const fn repeats(mut self, steps: Steps) -> Self {
        self.steps = Some(steps);
        self
    }

    const fn says(mut self, label: &'static str) -> Self {
        self.says = Some(label);
        self
    }

    const fn row(mut self, label: &'static str, group: Group) -> Self {
        self.row = Some(Row { label, group });
        self
    }
}

use Action as A;
use ActionSpec as S;
use Group::{End, Go, Reach, Start};

/// Every action, in the order the Settings window offers them.
pub static ACTIONS: &[ActionSpec] = &[
    S::new(A::None, "none", "nothing; bound to keys, it takes them away"),
    S::new(A::WindowLeft, "window.left", "move to the window to the left: the tree lies left of the content's windows").bar("Left Window", Icon::SquareArrowLeft),
    S::new(A::WindowDown, "window.down", "move to the window below: a session's prompt lies below its console").bar("Window Below", Icon::SquareArrowDown),
    S::new(A::WindowUp, "window.up", "move to the window above: a session's console lies above its prompt").bar("Window Above", Icon::SquareArrowUp),
    S::new(A::WindowRight, "window.right", "move to the window to the right: the details panel lies right of the content's windows").bar("Right Window", Icon::SquareArrowRight),
    S::new(A::WindowNext, "window.next", "move to the window after the one the keys are in, from the tree through the content's windows, each console before its prompt, to the details panel, going round at the end").bar("Next Window", Icon::ArrowBarRight),
    S::new(A::WindowPrev, "window.prev", "move to the window before the one the keys are in, in the order `window.next` goes, going round at the start").bar("Previous Window", Icon::ArrowBarLeft),
    S::new(A::WindowTop, "window.top", "move to the first window, the tree when shown").bar("First Window", Icon::ArrowUpLeft),
    S::new(A::WindowBottom, "window.bottom", "move to the last window, the details panel when shown").bar("Last Window", Icon::ArrowDownRight),
    S::new(A::WindowBack, "window.back", "move back to the window the keys were in before").bar("Back to Window Before", Icon::ArrowBackUp),
    S::new(A::WindowSplit, "window.split", "split the focused window one above the other, the new window, below it, showing the same ([content](content.md))").bar("Split One Above the Other", Icon::LayoutRows),
    S::new(A::WindowVsplit, "window.vsplit", "split the focused window side by side, the new window, right of it, showing the same ([content](content.md))").bar("Split Side by Side", Icon::LayoutColumns),
    S::new(A::WindowNew, "window.new", "split the focused window one above the other and start writing a new session in the new one, in the worktree the focused window shows").bar("Split for a New Session", Icon::SquarePlus),
    S::new(A::WindowQuit, "window.quit", "close the focused window, and its tab when it was the last; what it showed runs on").bar("Close Split", Icon::SquareMinus),
    S::new(A::WindowOnly, "window.only", "close every other window of the tab").bar("Close Other Splits", Icon::Square),
    S::new(A::WindowMoveLeft, "window.move_left", "move the focused window to the left of its tab, as tall as the tab")
        .bar("Move Window to the Left", Icon::ArrowBarToLeft),
    S::new(A::WindowMoveDown, "window.move_down", "move the focused window to the bottom of its tab, as wide as the tab")
        .bar("Move Window to the Bottom", Icon::ArrowBarToDown),
    S::new(A::WindowMoveUp, "window.move_up", "move the focused window to the top of its tab, as wide as the tab").bar("Move Window to the Top", Icon::ArrowBarToUp),
    S::new(A::WindowMoveRight, "window.move_right", "move the focused window to the right of its tab, as tall as the tab")
        .bar("Move Window to the Right", Icon::ArrowBarToRight),
    S::new(A::WindowExchange, "window.exchange", "exchange the focused window with the next one in its split, or the one before the last").bar("Exchange Windows", Icon::SwitchHorizontal),
    S::new(A::WindowRotate, "window.rotate", "rotate the windows of the focused window's split forward, the last becoming the first").bar("Rotate Windows", Icon::RotateClockwise),
    S::new(A::WindowEqualize, "window.equalize", "give the windows of every split the same size").bar("Equal Sizes", Icon::Equal),
    S::new(A::WindowNarrower, "window.narrower", "make the focused window a step narrower").bar("Narrower", Icon::ColumnRemove).repeats(Steps::WindowSize),
    S::new(A::WindowWider, "window.wider", "make the focused window a step wider").bar("Wider", Icon::ColumnInsertRight).repeats(Steps::WindowSize),
    S::new(A::WindowTaller, "window.taller", "make the focused window a step taller").bar("Taller", Icon::RowInsertBottom).repeats(Steps::WindowSize),
    S::new(A::WindowShorter, "window.shorter", "make the focused window a step shorter").bar("Shorter", Icon::RowRemove).repeats(Steps::WindowSize),
    S::new(A::WindowMaximizeHeight, "window.maximize_height", "make the focused window as tall as the others allow")
        .bar("Tallest", Icon::ArrowAutofitHeight),
    S::new(A::WindowMaximizeWidth, "window.maximize_width", "make the focused window as wide as the others allow").bar("Widest", Icon::ArrowAutofitWidth),
    S::new(A::WindowToTab, "window.to_tab", "move the focused window to a new tab, unless it is the only one of its tab").bar("Move Window to a New Tab", Icon::BrowserPlus),
    S::new(A::WindowEdit, "window.edit", "switch the focused window between a document's viewer and a Neovim on the document, which runs on hidden while the window shows the viewer ([content](content.md#viewers))").says("Switch between the viewer and Neovim"),
    S::new(A::TabMergeLast, "tab.merge_last", "merge the content's tab shown into the tab shown before it, or else the tab beside it: every window of it, as it is split, right of every window there, the focused one keeping the focus ([content](content.md))").says("Merge the tab into the tab before"),
    S::new(A::TabNext, "tab.next", "show the next tab of the area the keys are in: the details panel's tabs there, elsewhere the content's tabs").bar("Next Tab", Icon::ChevronRight).repeats(Steps::Tabs),
    S::new(A::TabPrev, "tab.prev", "show the previous tab of the area the keys are in: the details panel's tabs there, elsewhere the content's tabs").bar("Previous Tab", Icon::ChevronLeft).repeats(Steps::Tabs),
    S::new(A::TabLast, "tab.last", "show the content's tab shown before the one shown now").bar("Tab Before", Icon::ArrowBack),
    S::new(A::TabMovePrev, "tab.move_prev", "move the content's tab shown one place toward the first").says("Move the tab left").repeats(Steps::TabOrder),
    S::new(A::TabMoveNext, "tab.move_next", "move the content's tab shown one place toward the last").says("Move the tab right").repeats(Steps::TabOrder),
    S::new(A::TabRecent, "tab.recent", "open the tab switcher on the content's tabs, by when each was shown last or else in their order, and move to the next in the list; pressed again while it is open, move on; the tab moved to is shown once the modifier keys are let go, or on Enter ([content](content.md#tabs))").says("Switch to the recent tab"),
    S::new(A::TabRecentPrev, "tab.recent_prev", "open the tab switcher as `tab.recent` does, and move to the previous in the list instead of the next").says("Switch to the previous tab in the list"),
    S::new(A::TabNextSame, "tab.next_same", "show the next tab of the content that shows the same kind of thing as the tab shown does, going round at the end: sessions, terminals, files, browser pages or the rest").says("Next tab of the same kind"),
    S::new(A::TabPrevSame, "tab.prev_same", "show the previous tab of the content that shows the same kind of thing as the tab shown does, going round at the start").says("Previous tab of the same kind"),
    S::new(A::TabCloseFiles, "tab.close_files", "close every window of the content that shows a file in a viewer, and the tabs left empty; what else is shown stays").says("Close all file tabs"),
    S::new(A::CursorDown, "cursor.down", "move the selection in the tree to the next row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the next row"),
    S::new(A::CursorUp, "cursor.up", "move the selection in the tree to the previous row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the previous row"),
    S::new(A::CursorFirst, "cursor.first", "move the selection in the tree to the first row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the first row"),
    S::new(A::CursorLast, "cursor.last", "move the selection in the tree to the last row; in the details panel, in the list it shows, or scroll what else it shows").says("Select the last row"),
    S::new(A::CursorGo, "cursor.go", "open the selected session, shell, worktree or file: go to a window showing it, in any tab, or else open it in a new tab after the one shown, and move to it, or to the prompt of a session not started yet; on a folded row, a worktree or a session holding others too, unfold it instead; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row, an image of Resources as a file is opened, the keys staying in the panel")
        .says("Open it")
        .icon(Icon::ArrowRight)
        .row("Open", Go),
    S::new(A::CursorSplit, "cursor.split", "open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, below it; in the details panel, the selected image of Resources")
        .icon(Icon::LayoutRows)
        .row("Open in a split below", Go),
    S::new(A::CursorVsplit, "cursor.vsplit", "open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, beside it; in the details panel, the selected image of Resources")
        .icon(Icon::LayoutColumns)
        .row("Open in a split beside", Go),
    S::new(A::CursorTab, "cursor.tab", "open the selected session, shell, worktree or file as `cursor.go` does, in a new tab after the one shown, though a window shows it already; in the details panel, the selected image of Resources")
        .icon(Icon::BrowserPlus)
        .row("Open in a new tab", Go),
    S::new(A::CursorFold, "cursor.fold", "fold the selected node when it is open, or else the row it sits under, going there; in the details panel, go back out to the list what it shows was opened from").says("Fold, or go back"),
    S::new(A::CursorClose, "cursor.close", "close every window showing the selected session, shell or Neovim, or the selected file in its viewer or its Neovim; what they showed runs on")
        .icon(Icon::SquareMinus)
        .row("Close its windows", End),
    S::new(A::CursorCopy, "cursor.copy", "copy the path of the selected file, worktree or workspace, or the name of the selected session, shell or Neovim; in the details panel, the selected link of the overview: its address, or its file's path")
        .icon(Icon::Copy)
        .row("Copy", Go),
    S::new(A::CursorReveal, "cursor.reveal", "in the details panel, show the selected file of the overview in the Finder")
        .icon(Icon::FolderSearch)
        .row("Reveal in Finder", Go),
    S::new(A::CursorBrowse, "cursor.browse", "open the selected pull request or issue on GitHub in the browser")
        .icon(Icon::BrandGithub)
        .row("Open on GitHub", Go),
    S::new(A::FilerDown, "filer.down", "move to the next file or directory, in the tree's files or the picker's directories").says("Next entry"),
    S::new(A::FilerUp, "filer.up", "move to the previous file or directory").says("Previous entry"),
    S::new(A::FilerInto, "filer.into", "open the directory selected and go to its first entry once it is read; on a file of the tree, open it as `cursor.go` does").says("Go into"),
    S::new(A::FilerOut, "filer.out", "close the directory selected when it is open, or else the one it is in, going there").says("Close the directory"),
    S::new(A::FilerOpen, "filer.open", "in the tree, open the file or fold the directory selected as `cursor.go` does; in the picker, start a session in the directory selected").says("Open"),
    S::new(A::FilerHidden, "filer.hidden", "show or hide the files and directories whose names start with `.`").says("Show or hide the hidden"),
    S::new(A::FileNew, "file.new", "in the tree's files, type the name of a file to make in the directory selected, in the one the file selected is in, or at the top of the worktree from its Files heading; a name with `/` makes the directories it passes through too, one ending in `/` makes a directory, and one already there is left as it is")
        .icon(Icon::FilePlus)
        .row("New file…", Start),
    S::new(A::DirectoryNew, "directory.new", "type the name of a directory to make, where `file.new` makes a file")
        .icon(Icon::FolderPlus)
        .row("New directory…", Start),
    S::new(A::WaitingNext, "waiting.next", "open the next session waiting on you as `cursor.go` does, with the keys in its console").bar("Next Waiting", Icon::BellDown).repeats(Steps::Waiting),
    S::new(A::WaitingPrev, "waiting.prev", "open the previous session waiting on you as `cursor.go` does, with the keys in its console").bar("Previous Waiting", Icon::BellUp).repeats(Steps::Waiting),
    S::new(A::SessionNext, "session.next", "open the session after the one shown, in the tree's order over every session, folded away or filtered out too, going round at the end, as `cursor.go` does; the keys stay in the area they are in").bar("Next Session", Icon::MessageDown).repeats(Steps::Sessions),
    S::new(A::SessionPrev, "session.prev", "open the session before the one shown, in the tree's order over every session, folded away or filtered out too, going round at the start, as `cursor.go` does; the keys stay in the area they are in").bar("Previous Session", Icon::MessageUp).repeats(Steps::Sessions),
    S::new(A::WorkspaceNext, "workspace.next", "open a session of the workspace after the one shown, in the tree's order over every workspace holding a session, folded away or filtered out too, going round at the end: the session last shown there since the window opened, or else its first, as `cursor.go` does; the keys stay in the area they are in").bar("Next Workspace", Icon::FolderDown).repeats(Steps::Workspaces),
    S::new(A::WorkspacePrev, "workspace.prev", "open a session of the workspace before the one shown, as `workspace.next` does, going round at the start").bar("Previous Workspace", Icon::FolderUp).repeats(Steps::Workspaces),
    S::new(A::SessionPick, "session.pick", "open the picker, to go to a session or choose where a new one works ([details](tree.md#going-to-a-session-or-starting-one))").bar("Go to or Start Session…", Icon::Search),
    S::new(A::SessionNew, "session.new", "start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the tree, in the one the focused window shows from elsewhere")
        .bar("New Session", Icon::MessagePlus)
        .row("New session here", Start),
    S::new(A::SessionScratch, "session.scratch", "start a scratch session, with the prompt ready: one that runs in the scratch folder, for talking about something no project holds ([details](tree.md#scratch-sessions))").bar("New Scratch Session", Icon::Notes),
    S::new(A::SessionResume, "session.resume", "list the conversations Claude Code and Codex kept, to resume one in a new session ([details](sessions.md#resuming-a-past-conversation))").bar("Resume Conversation…", Icon::TransferIn),
    S::new(A::NoticesOpen, "notices.open", "open the list of what needed you, the sessions that began waiting on you, failed or finished a turn, to go through them, mark them read or unread and go to their sessions ([details](notices.md))").bar("Notices…", Icon::Bell),
    S::new(A::SessionBringBack, "session.bring_back", "list what the terminal host took with it when it went, to bring a session or a shell back ([details](sessions.md#when-the-terminal-host-goes))").bar("Bring Back Sessions…", Icon::Restore),
    S::new(A::SessionRestart, "session.restart", "start a session whose program exited again: the same agent in the same directory, continuing its conversation; the session keeps its row and its tabs ([details](sessions.md#when-a-program-exits))")
        .row("Restart", Go)
        .icon(Icon::Restore),
    S::new(A::SessionStop, "session.stop", "end a session Tsumugi runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later; on a terminal's row, end that terminal")
        .bar("End…", Icon::PlayerStop)
        .row("End…", End),
    S::new(A::SessionRename, "session.rename", "edit the name of a session Tsumugi runs in place; the tree shows it, and the session is sent Claude Code's `/rename` so its own interface says it too")
        .bar("Rename…", Icon::Pencil)
        .row("Rename…", Go),
    S::new(A::SessionMarkUnread, "session.mark_unread", "mark a session unread, as if it had finished out of view: its row is drawn bold and counted in the window's title until a window in front shows it anew ([details](sessions.md#unread-sessions))")
        .row("Mark as unread", Go)
        .icon(Icon::Circle),
    S::new(A::NotificationsMute, "notifications.mute", "silence the notifications outside the window of the workspace or worktree of the selected row, or tell them again; its rows keep showing how its sessions stand ([details](notices.md#outside-the-window))")
        .row("Mute or unmute notifications", Go)
        .icon(Icon::BellOff),
    S::new(A::SessionProfileNext, "session.profile_next", "while a session not started yet is shown, switch the profile it is to start from to the next one ([details](sessions.md#agents))")
        .bar("Next Profile of New Session", Icon::ChevronDown),
    S::new(A::SessionProfilePrev, "session.profile_prev", "while a session not started yet is shown, switch the profile it is to start from to the previous one ([details](sessions.md#agents))")
        .bar("Previous Profile of New Session", Icon::AdjustmentsHorizontal),
    S::new(A::ShellOpen, "shell.open", "open a shell in the worktree")
        .bar("Open Shell", Icon::Terminal2)
        .row("Open a shell here", Start),
    S::new(A::TerminalStop, "terminal.stop", "end the terminal selected in the tree, or else the one shown")
        .bar("Stop Terminal", Icon::SquareX)
        .row("Stop terminal", End),
    S::new(A::NeovimOpen, "neovim.open", "open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](content.md#terminals))")
        .bar("Open Neovim", Icon::Code)
        .row("Open Neovim here", Start),
    S::new(A::BrowserOpen, "browser.open", "open a new page of the environment's browser, listed under the worktree it is opened in")
        .bar("Open Browser", Icon::World)
        .row("Open Browser here", Start),
    S::new(A::BrowserFind, "browser.find", "search the browser page shown for text, in a bar under its toolbar: Enter shows the next match, Shift+Enter the previous and Escape closes the bar ([details](content.md#browsing))")
        .says("Find in the page"),
    S::new(A::BrowserReopen, "browser.reopen", "open again the browser page closed last in the worktree the keys are in, at the address it was closed at ([details](content.md#browsing))")
        .says("Reopen the closed page"),
    S::new(A::TabClose, "tab.close", "close the content's tab shown and every window in it; agents and terminals keep running and remain reachable from the tree")
        .bar("Close Tab", Icon::X)
        .row("Close tab", End),
    S::new(A::EnvironmentAdd, "environment.add", "add an environment, on this computer or on a work machine paired then, which every window lists at once ([details](environments.md))").bar("Add Environment…", Icon::Plus),
    S::new(A::EnvironmentRemove, "environment.remove", "remove the environment selected in the tree from every window after asking, unless the window was started in it; its sessions keep running on its host ([details](environments.md))")
        .icon(Icon::SquareMinus)
        .row("Remove environment…", End),
    S::new(A::EnvironmentRename, "environment.rename", "edit the name of the environment selected in the tree in place, on its heading; every window calls it by the name given ([details](environments.md))")
        .icon(Icon::Pencil)
        .row("Rename…", Go),
    S::new(A::EnvironmentRetry, "environment.retry", "try at once to reach again the work machine of the environment selected in the tree, or else of the tab shown, while the window waits to try it ([details](environments.md))")
        .icon(Icon::Refresh)
        .row("Retry now", Reach),
    S::new(A::EnvironmentPair, "environment.pair", "pair again with the work machine of the environment selected in the tree, or else of the tab shown, as when it no longer accepts this computer or answers at another address ([details](environments.md#create-and-configure))")
        .icon(Icon::Link)
        .row("Pair again…", Reach),
    S::new(A::WorkspaceAdd, "workspace.add", "open the picker on the directories alone, to add the one chosen to the tree with nothing running there ([details](tree.md#going-to-a-session-or-starting-one))")
        .bar("Add to Tree…", Icon::FolderPlus)
        .row("Add to tree…", Start),
    S::new(A::LocalIssues, "local_issues.open", "open local issues for this environment and working directory, to queue work for its default agent ([details](local-issues.md))")
        .bar("Local Issues…", Icon::Bug)
        .row("Local issues…", Start),
    S::new(A::LocalIssuesLeader, "local_issues.leader", "hand the open local issues of this environment to a leader session that carries them through itself within the discretion you set, after confirming the issues, its workers, their models and what it may decide ([details](leader-mode.md))")
        .icon(Icon::UsersGroup)
        .row("Let the leader handle it…", Start),
    S::new(A::SavedCommands, "saved_commands.open", "open the commands the project keeps, to search them, run one in the selected worktree after confirming it, and add, edit or delete one ([details](saved-commands.md))")
        .bar("Saved Commands…", Icon::Terminal)
        .row("Saved commands…", Start),
    S::new(A::ResourceAdd, "resource.add", "open the sheet that keeps a path or an address, with a label and a note, in the project of what the details panel shows, to open from its Resources tab ([details](resources.md#keeping-and-opening-them))")
        .bar("Add Resource…", Icon::FilePlus)
        .row("Add resource…", Start),
    S::new(A::ResourceEdit, "resource.edit", "open the sheet on the pinned resource the details panel's cursor is on, to change its label and note ([details](resources.md#keeping-and-opening-them))")
        .icon(Icon::Pencil)
        .row("Edit label and note…", Go),
    S::new(A::ResourceRemove, "resource.remove", "stop keeping the pinned resource the details panel's cursor is on, which leaves what it points at as it is ([details](resources.md#keeping-and-opening-them))")
        .icon(Icon::Trash)
        .row("Remove from resources", End),
    S::new(A::ResourceKeep, "resource.keep", "keep the address, file or image the details panel's cursor is on in the project's resources, unless it keeps it already ([details](resources.md#keeping-and-opening-them))")
        .icon(Icon::SquarePlus)
        .row("Keep as resource", Go),
    S::new(A::ResourceScreenshot, "resource.screenshot", "take a screenshot of the visible part of the browser page shown and keep the picture, with the page's address as its note, among the project's resources ([details](resources.md#keeping-and-opening-them))")
        .icon(Icon::Photo)
        .row("Screenshot of the page", Start),
    S::new(A::WorkspaceRemove, "workspace.remove", "take the workspace selected in the tree out of it, which lists it while nothing runs there too; where something runs, ask first and end every session and terminal running there. Starting anything there lists it again ([details](tree.md#workspaces-and-worktrees))")
        .icon(Icon::SquareMinus)
        .row("Remove from tree…", End),
    S::new(A::WorktreeClose, "worktree.close", "end, after asking, every session and terminal running in the worktree selected in the tree, which takes its row out of it; the workspace stays listed ([details](tree.md#workspaces-and-worktrees))")
        .icon(Icon::X)
        .row("Close worktree…", End),
    S::new(A::WorktreeNew, "worktree.new", "open the dialog asking for a new worktree of the repository of the workspace or worktree selected in the tree: its name, where it starts, listed from the repository's branches or given as a commit, and optionally the branch it works on; it is made in the background, a row in the tree showing how far it has come, and a new session's draft opens in it once it is made ([details](tree.md#making-and-deleting-worktrees))")
        .icon(Icon::GitBranch)
        .row("New worktree…", Start),
    S::new(A::WorktreeMerge, "worktree.merge", "merge the branch of the worktree selected in the tree into the main checkout with a merge commit, after asking and saying what it brings; a merge that would conflict is never begun there, and uncommitted changes in the main checkout are stashed or committed only as the question says; afterwards the local issue its session was started for is closed and the worktree is offered for deleting ([details](tree.md#merging-a-worktree-into-main))")
        .icon(Icon::GitBranch)
        .row("Merge into main…", Start),
    S::new(A::WorktreeDelete, "worktree.delete", "delete the worktree selected in the tree, its directory and, as the question says, its branch, after asking and saying what is lost: files changed, commits not pushed, commits not merged, and the sessions and terminals that end with it; it is never deleted while it has changes unless the question is answered to discard them ([details](tree.md#making-and-deleting-worktrees))")
        .icon(Icon::Trash)
        .row("Delete worktree…", End),
    S::new(A::WorktreeCleanup, "worktree.cleanup", "list the worktrees of the repository of the workspace or worktree selected in the tree with what is weighed before deleting each: the branch, files changed, commits not pushed, whether the branch is merged into the base branch, the state of its pull request, its size and its last commit; those whose pull request is merged or closed and that lose nothing are chosen beforehand, and the chosen ones are deleted together, after a question that names what each loses wherever it loses anything ([details](tree.md#cleaning-up-worktrees))")
        .icon(Icon::Trash)
        .row("Clean Up Worktrees…", End),
    S::new(A::WorktreeSleep, "worktree.sleep", "put the worktree selected in the tree to sleep: its sessions' programs stop and its terminals end, while its files, its branch and its sessions' rows and tabs stay; one question names what is cut short where a session is working or a terminal runs, and a session that cannot be woken again keeps running ([details](tree.md#putting-a-worktree-to-sleep))")
        .icon(Icon::SunMoon)
        .row("Sleep worktree…", End),
    S::new(A::WorktreeWake, "worktree.wake", "wake the worktree selected in the tree: its sleeping sessions are brought back with their conversations ([details](tree.md#putting-a-worktree-to-sleep))")
        .icon(Icon::SunMoon)
        .row("Wake worktree", End),
    S::new(A::WorkspacePin, "workspace.pin", "pin the workspace or worktree selected in the tree to the top of the tree or of its workspace, or unpin it; what is pinned is never left out by the hide filters and stays pinned across runs ([details](tree.md#arranging-the-tree))")
        .icon(Icon::Highlight)
        .row("Pin or unpin", End),
    S::new(A::TreeSort, "tree.sort", "step the order of a workspace's worktrees below the pinned ones: as the work host lists them, by activity, the worktree a session was written to or started in last first, and by name ([details](tree.md#arranging-the-tree))")
        .icon(Icon::ArrowsExchange)
        .row("Sort worktrees", End),
    S::new(A::TreeHideSleeping, "tree.hide_sleeping", "leave the worktrees whose sessions all sleep out of the tree, or show them again; their sessions stay listed under their workspace ([details](tree.md#arranging-the-tree))")
        .icon(Icon::Eye)
        .row("Hide sleeping worktrees", End),
    S::new(A::TreeHideIdle, "tree.hide_idle", "leave the worktrees where no session works, waits or starts and no terminal runs out of the tree, or show them again; their sessions stay listed under their workspace ([details](tree.md#arranging-the-tree))")
        .icon(Icon::Eye)
        .row("Hide idle worktrees", End),
    S::new(A::TreeHideMain, "tree.hide_main", "leave the main checkout out of a workspace that lists other worktrees, or show it again ([details](tree.md#arranging-the-tree))")
        .icon(Icon::Eye)
        .row("Hide main checkouts", End),
    S::new(A::TreeRevealCurrent, "tree.reveal_current", "select in the tree the row of the session or terminal the focused window shows, unfolding the way to it and showing its worktree again where a hide filter left it out ([details](tree.md#arranging-the-tree))")
        .icon(Icon::Crosshair)
        .row("Reveal current in tree", Go),
    S::new(A::WindowClose, "window.close", "close the window; the instance's other windows stay open, and its sessions run on").bar("Close Window", Icon::AppWindow),
    S::new(A::WindowOpen, "window.open", "open another window of the instance ([windows](sessions.md#windows))").bar("New Window", Icon::CopyPlus),
    S::new(A::Refresh, "refresh", "read the sessions, and what git says of where they run, again now").bar("Refresh", Icon::Refresh),
    S::new(A::Filter, "filter", "narrow the tree to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it; while a browser page has the keys, search the page as `browser.find` does").bar("Filter…", Icon::Filter),
    S::new(A::DetailsTab(1), "details.tab", "show the details panel's tab numbered `n`, from 1 to 9, of those it has for what it describes ([details](details.md#components)), and the panel when it is hidden").says("Show details tab"),
    S::new(A::WorktreesToggle, "worktrees.toggle", "list every worktree git knows, those without sessions too, or only those with sessions again").bar("Toggle All Worktrees", Icon::ListTree),
    S::new(A::ScrollDown, "scroll.down", "scroll the details panel down a step; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off; bound in a viewer, scroll what it shows").says("Scroll down"),
    S::new(A::ScrollUp, "scroll.up", "scroll up a step, what `scroll.down` scrolls").says("Scroll up"),
    S::new(A::ScrollHalfDown, "scroll.half_down", "scroll down half a page, what `scroll.down` scrolls").says("Scroll down half a page"),
    S::new(A::ScrollHalfUp, "scroll.half_up", "scroll up half a page, what `scroll.down` scrolls").says("Scroll up half a page"),
    S::new(A::ScrollTop, "scroll.top", "scroll to the top, what `scroll.down` scrolls").says("Scroll to the top"),
    S::new(A::ScrollBottom, "scroll.bottom", "scroll to the bottom, what `scroll.down` scrolls").says("Scroll to the bottom"),
    S::new(A::ConsoleSendNext, "console.send_next", "send the next key as it is, whatever it is bound to").says("Send the next key as it is"),
    S::new(A::ConsolePassEveryKey, "console.pass_every_key", "send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile").says("Pass every key on"),
    S::new(A::ConsoleRead, "console.read", "read the console with Vim's keys, to move, search, select and yank in it, whatever it shows: a shell or a session showing a dialog too, whose keys are otherwise all its own; from a session's prompt, read the console above it").says("Read the console"),
    S::new(A::PromptFocus, "prompt.focus", "give the prompt the keys").bar("Prompt", Icon::CursorText),
    S::new(A::PromptWrite, "prompt.write", "give the prompt the keys, and the key pressed with them").says("Write in the prompt"),
    S::new(A::PromptSend, "prompt.send", "send what the prompt holds").bar("Send Prompt", Icon::Send),
    S::new(A::PromptImages, "prompt.images", "select an attached image").bar("Select Attached Image", Icon::Photo),
    S::new(A::PromptTakeSuggestion, "prompt.take_suggestion", "on an empty prompt, send the key pressed to the session and then bring the session's input into the prompt, so a prompt Claude Code suggests is taken to edit; an agent whose input cannot be taken back gets the key alone. On a prompt holding text or images, the key, Tab when none, is typed into Neovim").says("Take the suggested prompt"),
    S::new(A::HistoryPrev, "prompt.history_prev", "put the prompt sent before the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the focused window's session works").says("Previous prompt"),
    S::new(A::HistoryNext, "prompt.history_next", "put the prompt sent after the one shown in the prompt, as `prompt.history_prev` does").says("Next prompt"),
    S::new(A::SettingsOpen, "settings.open", "open the Settings window, on the environment of the row selected in the tree or else of the tab shown")
        .bar("Settings…", Icon::Settings)
        .row("Environment settings…", Go),
    S::new(A::DaemonRestart, "daemon.restart", "restart the sessions' daemon after asking, ending every session it holds ([details](sessions.md#the-hosts))").bar("Restart Session Daemon…", Icon::ServerBolt),
    S::new(A::KeysHelp, "keys.help", "show every binding in force, area by area, the areas the keys are in first, each with what its action does and whether it repeats; `Escape`, `q` or the key that showed it closes it, and no other key acts behind it").bar("Keyboard Shortcuts", Icon::Keyboard),
    S::new(A::ZoomIn, "zoom.in", "draw the window a step larger").bar("Zoom In", Icon::ZoomIn),
    S::new(A::ZoomOut, "zoom.out", "draw the window a step smaller").bar("Zoom Out", Icon::ZoomOut),
    S::new(A::ZoomReset, "zoom.reset", "draw the window at the zoom the settings give").bar("Actual Size", Icon::ZoomReset),
    S::new(A::TreeToggle, "tree.toggle", "show or hide the tree").bar("Toggle Tree", Icon::LayoutSidebar),
    S::new(A::TreeFocus, "tree.focus", "show and focus the file tree; from outside the sidebar, two Escapes run it while the first Escape keeps its usual meaning ([keys](keys.md#where-the-keys-go))").says("Focus file tree"),
    S::new(A::SidebarNext, "sidebar.next", "focus the other sidebar section, giving its list more room ([agents](agents-panel.md))").says("Focus other sidebar section"),
    S::new(A::AgentsExpand, "agents.expand", "expand the selected agent family or move to its first child").says("Expand agent family"),
    S::new(A::AgentsCollapse, "agents.collapse", "fold the selected agent family or move to its parent").says("Fold agent family"),
    S::new(A::AgentsReveal, "agents.reveal", "from Agents, show the selected session in Tree, clearing Tree's filter and unfolding its directory path ([agents](agents-panel.md))")
        .icon(Icon::LayoutSidebar)
        .row("Show in Tree", Go),
    S::new(A::DetailsToggle, "details.toggle", "show or hide the details panel").bar("Toggle Details", Icon::LayoutSidebarRight),
    S::new(A::DetailsStage, "details.stage", "in the details panel, stage the selected change of the worktree, or unstage it when it is only staged; a file in conflict is staged to mark it resolved ([details](details.md#changes))").says("Stage or unstage"),
    S::new(A::DetailsStageAll, "details.stage_all", "in the details panel, stage every change of the worktree, untracked files included ([details](details.md#changes))").says("Stage all changes"),
    S::new(A::DetailsDiscard, "details.discard", "in the details panel, discard the selected change of the worktree, staged or not, after asking; a file in conflict cannot be discarded ([details](details.md#changes))").says("Discard change…"),
    S::new(A::DetailsCommit, "details.commit", "in the details panel, write the message of a commit in the prompt: sending it commits what is staged, and leaving the prompt gives it up; a hook that refuses the commit says why and the message stays in the prompt ([details](details.md#changes))").says("Commit staged changes"),
    S::new(A::DetailsAmend, "details.amend", "in the details panel, after asking, write the message of a commit that replaces the last one, starting from its message, as `details.commit` does ([details](details.md#changes))").says("Amend last commit…"),
    S::new(A::DetailsPull, "details.pull", "in the details panel, pull the branch's upstream; diverged branches are merged unless git is configured to rebase ([details](details.md#changes))").says("Pull"),
    S::new(A::DetailsPush, "details.push", "in the details panel, push the branch, setting its upstream when it has none ([details](details.md#changes))").says("Push"),
    S::new(A::DetailsForcePush, "details.force_push", "in the details panel, after asking, push the branch over the remote's, but only if nobody else pushed to it since it was fetched ([details](details.md#changes))").says("Force push…"),
    S::new(A::DetailsCreatePull, "details.create_pull", "in the details panel, write a pull request for the branch in the prompt: the title on its first line, the body after it, and the base branch and whether it is a draft as headers above; sending pushes the branch and opens the pull request, and leaving the prompt gives it up ([details](details.md#changes))").says("Create pull request"),
    S::new(A::DetailsDraftPull, "details.draft_pull", "in the details panel, ask the agent the pull request recipe names to write the title and body of a pull request for the branch, once, and put them in the prompt to read, edit and send; nothing is pushed or opened until you send ([ai actions](ai-actions.md))").says("Write the pull request with an agent"),
    S::new(A::DetailsGenerateCommit, "details.generate_commit", "in the details panel, ask the agent the commit message recipe names to write the message of a commit of what is staged, once, and put it in the prompt to read, edit and send; nothing is committed until you send ([ai actions](ai-actions.md))").says("Write the commit message with an agent"),
    S::new(A::DetailsFixCommit, "details.fix_commit", "in the details panel, after a hook refused a commit, open a draft in the worktree whose prompt asks the agent the hook failure recipe names to fix what it complained of without bypassing the hook, committing or pushing; sending it starts the session ([ai actions](ai-actions.md))").says("Fix the refused commit with an agent"),
    S::new(A::DetailsAbort, "details.abort", "in the details panel, after asking, give up the merge, rebase, cherry-pick or revert in progress and put the worktree back as it was before it began ([details](details.md#branch-and-conflicts))").says("Abort the operation…"),
    S::new(A::DetailsResume, "details.resume", "in the details panel, go on with the merge, rebase, cherry-pick or revert in progress once every conflict is resolved and staged ([details](details.md#branch-and-conflicts))").says("Continue the operation"),
    S::new(A::DetailsBase, "details.base", "in the details panel, write the base the branch is compared with in the prompt, an empty one giving up the choice for the one Tsumugi finds; sending chooses it ([details](details.md#branch-and-conflicts))").says("Choose the base of the branch"),
    S::new(A::DetailsResolve, "details.resolve", "in the details panel, open a draft in the worktree whose prompt asks the agent the conflict recipe names to resolve the files in conflict, without pushing; sending it starts the session ([ai actions](ai-actions.md))").says("Resolve the conflicts with an agent"),
    S::new(A::DetailsNeovim, "details.neovim", "in the details panel, open the diff of the selected change in your Neovim, a file in conflict in its Diffview where it has one ([details](details.md#branch-and-conflicts))").says("Open the change in Neovim"),
    S::new(A::LogCopyHash, "log.copy_hash", "in the details panel, copy the full hash of the commit selected in the log, or opened from it ([details](details.md#log))").says("Copy the commit hash")
        .icon(Icon::Copy)
        .row("Copy commit hash", Go),
    S::new(A::LogOpenRemote, "log.open_remote", "in the details panel, open the commit selected in the log on GitHub, in the browser of the worktree; a repository that is not on GitHub says so ([details](details.md#log))").says("Open the commit on GitHub")
        .icon(Icon::BrandGithub)
        .row("Open commit on GitHub", Go),
    S::new(A::LogExplain, "log.explain", "in the details panel, open a draft in the worktree whose prompt asks an agent to explain the commit selected in the log; sending it starts the session ([details](details.md#log))").says("Explain the commit with an agent")
        .icon(Icon::MessageQuestion)
        .row("Explain with an agent", Go),
    S::new(A::LogBranch, "log.branch", "in the details panel, write the name of a branch to make at the commit selected in the log in the prompt: sending makes it without checking it out, and leaving the prompt gives it up ([details](details.md#log))").says("Create a branch here")
        .icon(Icon::GitBranch)
        .row("Create branch here…", Start),
    S::new(A::LogWorktree, "log.worktree", "in the details panel, open the new worktree dialog starting at the commit selected in the log ([details](details.md#log))").says("Create a worktree here")
        .icon(Icon::FolderPlus)
        .row("Create worktree here…", Start),
    S::new(A::LogCherryPick, "log.cherry_pick", "in the details panel, after asking, cherry-pick the commit selected in the log onto the branch checked out; a dirty worktree, an unfinished operation, a merge commit and a commit already applied are told and change nothing, and a conflict is left to resolve, continue or abort ([details](details.md#log))").says("Cherry-pick the commit…")
        .icon(Icon::GitCommit)
        .row("Cherry-pick…", Go),
    S::new(A::LogRevert, "log.revert", "in the details panel, after asking, revert the commit selected in the log in a commit of its own on the branch checked out, with the same refusals as `log.cherry_pick` ([details](details.md#log))").says("Revert the commit…")
        .icon(Icon::ArrowBackUp)
        .row("Revert…", Go),
    S::new(A::ViewerZoomIn, "viewer.zoom_in", "in a viewer, draw an image, a PDF, an HTML page or a Markdown document larger ([viewers](content.md#viewers))").says("Zoom in"),
    S::new(A::ViewerZoomOut, "viewer.zoom_out", "in a viewer, draw an image, a PDF, an HTML page or a Markdown document smaller").says("Zoom out"),
    S::new(A::ViewerZoomFit, "viewer.zoom_fit", "in a viewer, draw an image, a PDF, an HTML page or a Markdown document fitting the window").says("Fit to the window"),
    S::new(A::ViewerPageNext, "viewer.page_next", "in a viewer, show a PDF's next page").says("Next page"),
    S::new(A::ViewerPagePrev, "viewer.page_prev", "in a viewer, show a PDF's previous page").says("Previous page"),
    S::new(A::ViewerPlay, "viewer.play", "in a viewer, play or pause a movie").says("Play or pause"),
    S::new(A::ViewerBack, "viewer.back", "in a viewer, seek a movie back").says("Back"),
    S::new(A::ViewerForward, "viewer.forward", "in a viewer, seek a movie forward").says("Forward"),
    S::new(A::ViewerCopyPath, "viewer.copy_path", "in a viewer, copy the path of the file it shows").says("Copy the path"),
    S::new(A::ReviewComment, "review.comment", "in a diff of lines, write a comment on the line the cursor is on, or on the lines marked with `review.select`; `Enter` keeps it and `Escape` gives it up ([review](review.md))").says("Comment on the line"),
    S::new(A::ReviewSelect, "review.select", "in a diff of lines, start marking lines from the cursor to comment on them together, or stop").says("Mark lines"),
    S::new(A::ReviewEdit, "review.edit", "in a diff of lines, change the comment on the line the cursor is on").says("Edit the comment"),
    S::new(A::ReviewDelete, "review.delete", "in a diff of lines, delete the comment on the line the cursor is on").says("Delete the comment"),
    S::new(A::ReviewResolve, "review.resolve", "in a diff of lines, resolve the comment on the line the cursor is on, or open it again; only this resolves a comment").says("Resolve the comment"),
    S::new(A::ReviewSend, "review.send", "in a diff of lines, show the prompt that carries every comment not resolved; shown, send it to the session chosen with the movement keys").says("Send the review to an agent"),
    S::new(A::ReviewDraft, "review.draft", "in a diff of lines, or in the prompt shown, write the prompt in a new session's draft in the worktree instead of sending it").says("Start a session on the review"),
    S::new(A::DiffNextHunk, "diff.next_hunk", "in a diff tab, move to the next change of the file ([details](details.md#diff-tabs))").says("Next change"),
    S::new(A::DiffPrevHunk, "diff.prev_hunk", "in a diff tab, move to the previous change of the file").says("Previous change"),
    S::new(A::DiffNextFile, "diff.next_file", "in a diff tab, open the next file of the list of files changed").says("Next file"),
    S::new(A::DiffPrevFile, "diff.prev_file", "in a diff tab, open the previous file of the list of files changed").says("Previous file"),
    S::new(A::DiffWrap, "diff.wrap", "in a diff tab, break long lines to the width of the tab, or scroll across them again; kept for the worktree").says("Wrap lines"),
    S::new(A::DiffFiles, "diff.files", "in a diff tab, show or hide the list of files changed; kept for the worktree").says("Show or hide the files"),
    S::new(A::DiffPictures, "diff.pictures", "in a diff tab on a picture, compare its versions side by side, with a divider swiped across them, or faded over each other; kept for the worktree").says("Compare pictures another way"),
    S::new(A::GithubNext, "github.next", "in a pull request's or an issue's tab, select the next card of its conversation, which `github.reply`, `github.resolve` and `github.react` act on ([tree](tree.md#writing-on-a-pull-request-or-an-issue))").says("Next card"),
    S::new(A::GithubPrev, "github.prev", "in a pull request's or an issue's tab, select the previous card of its conversation").says("Previous card"),
    S::new(A::GithubComment, "github.comment", "in a pull request's or an issue's tab, write a new comment in the prompt under it; sending the prompt posts it, and leaving the prompt gives it up").says("Comment on the pull request or issue"),
    S::new(A::GithubReply, "github.reply", "in a pull request's or an issue's tab, reply to the card selected: in its thread of lines when it belongs to one, otherwise as a comment that quotes it").says("Reply to the card"),
    S::new(A::GithubResolve, "github.resolve", "in a pull request's tab, resolve the thread of lines the card selected belongs to, or open it again").says("Resolve the thread"),
    S::new(A::GithubReact, "github.react", "in a pull request's or an issue's tab, react to the card selected: write `React:` and one of GitHub's eight reactions in the prompt, or `Unreact:` to take one back").says("React to the card"),
    S::new(A::GithubReview, "github.review", "in a pull request's tab, write a review in the prompt: `Review:` and approve, request-changes or comment on its first line, then your words; sending the prompt submits it").says("Review the pull request"),
    S::new(A::GithubFixChecks, "github.fix_checks", "in a pull request's tab, prepare a request to fix its failed checks in the draft of a new session on the pull request, with the end of their logs read afresh; nothing is sent or posted until you send the prompt ([tree](tree.md#asking-an-agent-to-fix-a-pull-request))").says("Ask an agent to fix the failed checks"),
    S::new(A::GithubFixThreads, "github.fix_threads", "in a pull request's tab, prepare a request to fix the unresolved review threads, or the thread of the card selected, in the draft of a new session on the pull request; nothing is sent, replied to or resolved ([tree](tree.md#asking-an-agent-to-fix-a-pull-request))").says("Ask an agent to fix the review threads"),
    S::new(A::GithubMerge, "github.merge", "in a pull request's tab, merge it with the method its repository prefers, after asking; what GitHub would refuse, such as a missing approval, a failed required check or a conflict, is said instead of asked ([tree](tree.md#merging-and-closing-a-pull-request))").says("Merge the pull request"),
    S::new(A::GithubMergeSquash, "github.merge_squash", "in a pull request's tab, squash and merge it, after asking; refused where the repository does not allow it").says("Squash and merge"),
    S::new(A::GithubMergeCommit, "github.merge_commit", "in a pull request's tab, merge it with a merge commit, after asking; refused where the repository does not allow it").says("Create a merge commit"),
    S::new(A::GithubMergeRebase, "github.merge_rebase", "in a pull request's tab, rebase and merge it, after asking; refused where the repository does not allow it").says("Rebase and merge"),
    S::new(A::GithubAutoMerge, "github.auto_merge", "in a pull request's tab, have GitHub merge it once what it waits on is met, or stop it where that is on, after asking; offered only while something holds the merge back").says("Toggle auto-merge"),
    S::new(A::GithubClose, "github.close", "in a pull request's tab, close it, or open it again where it is closed, after asking").says("Close or reopen the pull request"),
    S::new(A::GithubReady, "github.ready", "in a pull request's tab, mark a draft ready for review, or make an open one a draft, after asking").says("Mark ready or make a draft"),
    S::new(A::CheckRerun, "check.rerun", "in a check's log, run the failed jobs of its Actions run again, after asking; a check whose address does not name its Actions run is said instead of asked ([tree](tree.md#reading-a-checks-log))").says("Run the failed jobs again"),
    S::new(A::BoardOpen, "board.open", "open the agent board, a window of every session as a card in the lane of what it asks of you; where it is open, go to it ([board](board.md))").says("Open the agent board"),
    S::new(A::BoardLeft, "board.left", "in the agent board, go to the lane on the left, skipping lanes without cards").says("Previous lane"),
    S::new(A::BoardRight, "board.right", "in the agent board, go to the lane on the right, skipping lanes without cards").says("Next lane"),
    S::new(A::BoardDown, "board.down", "in the agent board, go to the next card of the lane").says("Next card"),
    S::new(A::BoardUp, "board.up", "in the agent board, go to the previous card of the lane").says("Previous card"),
    S::new(A::BoardFirst, "board.first", "in the agent board, go to the first card of the lane").says("First card"),
    S::new(A::BoardLast, "board.last", "in the agent board, go to the last card of the lane").says("Last card"),
    S::new(A::BoardEnter, "board.enter", "in the agent board, open the session of the card").says("Open the card's session"),
    S::new(A::BoardIdle, "board.idle", "in the agent board, show or hide the lane of idle sessions, those that finished and have been quiet for half an hour").says("Show or hide idle"),
    S::new(A::BoardGroup, "board.group", "in the agent board, fold the sessions a session opened under its card, or give each its own card").says("Group by opener"),
    S::new(A::BoardExpand, "board.expand", "in the agent board, list or fold the sessions under the card, while they are grouped").says("Expand or fold the card"),
    S::new(A::BoardProject, "board.project", "in the agent board, narrow it to the next project in turn, and then to none").says("Filter by project"),
    S::new(A::BoardState, "board.state", "in the agent board, narrow it to the next lane in turn, and then to none").says("Filter by state"),
    S::new(A::BoardClear, "board.clear", "in the agent board, drop the search and every filter").says("Clear filters"),
    S::new(A::BoardSearch, "board.search", "in the agent board, type what to search the cards for; Enter keeps it and Esc clears it").says("Search the board"),
    S::new(A::LightboxNext, "lightbox.next", "in the lightbox, show the next image of the session's Resources, going round at the end ([images in the console](content.md#images-in-the-console))").says("Next image"),
    S::new(A::LightboxPrev, "lightbox.prev", "in the lightbox, show the previous image, going round at the start").says("Previous image"),
    S::new(A::LightboxZoomIn, "lightbox.zoom_in", "in the lightbox, draw the image larger").says("Zoom in"),
    S::new(A::LightboxZoomOut, "lightbox.zoom_out", "in the lightbox, draw the image smaller").says("Zoom out"),
    S::new(A::LightboxZoomFit, "lightbox.zoom_fit", "in the lightbox, draw the image whole, in the middle").says("Fit to the lightbox"),
    S::new(A::LightboxPanLeft, "lightbox.pan_left", "in the lightbox, move the view of the image left").says("Move left"),
    S::new(A::LightboxPanRight, "lightbox.pan_right", "in the lightbox, move the view of the image right").says("Move right"),
    S::new(A::LightboxPanUp, "lightbox.pan_up", "in the lightbox, move the view of the image up").says("Move up"),
    S::new(A::LightboxPanDown, "lightbox.pan_down", "in the lightbox, move the view of the image down").says("Move down"),
    S::new(A::LightboxClose, "lightbox.close", "close the lightbox").says("Close"),
    S::new(A::FloatingToggle, "floating.toggle", "open the floating terminal, a shell over the window that every worktree shares; with it open, give it the keys; with it having them, close it, leaving its shells running ([details](content.md#the-floating-terminal))").says("Floating terminal"),
    S::new(A::FloatingNew, "floating.new", "in the floating terminal, open another shell in a tab of its own").says("New floating shell"),
    S::new(A::FloatingCloseTab, "floating.close_tab", "in the floating terminal, end the shell of the tab shown; the panel closes with the last").says("Close floating shell"),
    S::new(A::FloatingNext, "floating.next", "in the floating terminal, show the next tab, going round at the end").says("Next floating shell"),
    S::new(A::FloatingPrev, "floating.prev", "in the floating terminal, show the previous tab, going round at the start").says("Previous floating shell"),
    S::new(A::OverlayClose, "overlay.close", "close what is open over the window: a picker, a menu, the keys help or the sheet adding an environment; a question is answered no ([keys](keys.md#over-the-window))").says("Close"),
    S::new(A::OverlayDown, "overlay.down", "over the window, move to the next row of a picker's list, a menu or the sheet adding an environment, going round at the end; scroll the keys help down a line").says("Move down"),
    S::new(A::OverlayUp, "overlay.up", "over the window, move to the previous row, as `overlay.down` moves to the next; scroll the keys help up a line").says("Move up"),
    S::new(A::OverlayChoose, "overlay.choose", "over the window, choose the row moved to in a picker's list, a menu or the sheet adding an environment; a question is answered yes").says("Choose"),
    S::new(A::ConsoleSend(None), "console.send", "send the key pressed to the session; with a key named after it, as in `console.send Escape`, send that key instead").says("Send the key to the console"),
    S::new(A::TabGo(1), "tab.go", "show the content's tab numbered `n`, from 1 to 8, or the last when there are fewer; 9 shows the last however many there are").says("Go to tab"),
    S::new(A::TabMerge(1), "tab.merge", "merge the content's tab shown into the tab numbered `n`, as `tab.merge_last` does, from 1 to 8, or the last when there are fewer; 9 is the last however many there are").says("Merge the tab into tab"),
];

impl Action {
    /// What the action does, in a few words: what the keys help lists it
    /// as. Empty for [`Action::None`].
    pub fn label(&self) -> String {
        let spec = self.spec();
        let label = spec
            .says
            .or(spec.bar)
            .or(spec.row.map(|r| r.label))
            .unwrap_or_default();
        match self {
            Action::ConsoleSend(Some(c)) => format!("Send {c} to the console"),
            _ => match self.number() {
                Some(n) => format!("{label} {n}"),
                None => label.to_string(),
            },
        }
    }

    /// The tab's number an action that takes one names.
    pub fn number(&self) -> Option<u8> {
        match self {
            Action::TabGo(n) | Action::TabMerge(n) | Action::DetailsTab(n) => Some(*n),
            _ => None,
        }
    }

    /// The same action naming the tab numbered `n`; one that takes no
    /// number stays as it is.
    pub fn numbered(&self, n: u8) -> Action {
        match self {
            Action::TabGo(_) => Action::TabGo(n),
            Action::TabMerge(_) => Action::TabMerge(n),
            Action::DetailsTab(_) => Action::DetailsTab(n),
            a => a.clone(),
        }
    }

    /// The row of [`ACTIONS`] that describes this action.
    pub fn spec(&self) -> &'static ActionSpec {
        ACTIONS
            .iter()
            .find(|s| discriminant(&s.action) == discriminant(self))
            .expect("every action has a row in ACTIONS")
    }

    /// What the action steps through, if its last key runs it again alone.
    pub fn steps(&self) -> Option<Steps> {
        self.spec().steps
    }

    /// The icon the action is shown with; `None` for an action that is only
    /// a key.
    pub fn icon(&self) -> Option<Icon> {
        self.spec().icon
    }

    /// Every action's name that a binding can name alone, as the Settings
    /// window offers them: not `none`, nor those that need a tab's number.
    pub fn names() -> impl Iterator<Item = &'static str> {
        ACTIONS
            .iter()
            .filter(|s| s.action != Action::None && s.action.number().is_none())
            .map(|s| s.name)
    }
}

impl FromStr for Action {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let (name, arg) = match text.split_once(' ') {
            Some((n, a)) => (n, Some(a.trim())),
            None => (text, None),
        };
        let unknown = || Error(format!("{text:?} is not an action"));
        let spec = ACTIONS
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(unknown)?;
        Ok(match (&spec.action, arg) {
            (Action::ConsoleSend(_), Some(k)) => Action::ConsoleSend(Some(Chord::parse(k)?)),
            (a, Some(n)) if a.number().is_some() => match n.parse::<u8>() {
                Ok(n @ 1..=9) => a.numbered(n),
                _ => return Err(Error(format!("{text:?} names no tab from 1 to 9"))),
            },
            (a, None) if a.number().is_some() => return Err(unknown()),
            (Action::ConsoleSend(_), None) => Action::ConsoleSend(None),
            (action, None) => action.clone(),
            (_, Some(_)) => return Err(unknown()),
        })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.spec().name;
        match self {
            Action::ConsoleSend(Some(c)) => write!(f, "{name} {c}"),
            _ => match self.number() {
                Some(n) => write!(f, "{name} {n}"),
                None => f.write_str(name),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Every variant of [`Action`], one value of each. The match makes a
    /// new variant fail to build here until it is listed.
    macro_rules! every_action {
        ($($variant:ident),* $(,)?) => {{
            fn _listed(a: &Action) {
                match a {
                    Action::ConsoleSend(_)
                    | Action::TabGo(_)
                    | Action::TabMerge(_)
                    | Action::DetailsTab(_) => {}
                    $(Action::$variant => {})*
                }
            }
            vec![$(Action::$variant,)* Action::ConsoleSend(None), Action::TabGo(1), Action::TabMerge(1), Action::DetailsTab(1)]
        }};
    }

    fn every_action() -> Vec<Action> {
        every_action![
            None,
            WindowLeft,
            WindowDown,
            WindowUp,
            WindowRight,
            WindowNext,
            WindowPrev,
            WindowTop,
            WindowBottom,
            WindowBack,
            WindowSplit,
            WindowVsplit,
            WindowNew,
            WindowQuit,
            WindowOnly,
            WindowMoveLeft,
            WindowMoveDown,
            WindowMoveUp,
            WindowMoveRight,
            WindowExchange,
            WindowRotate,
            WindowEqualize,
            WindowNarrower,
            WindowWider,
            WindowTaller,
            WindowShorter,
            WindowMaximizeHeight,
            WindowMaximizeWidth,
            WindowToTab,
            WindowEdit,
            TabMergeLast,
            TabNext,
            TabPrev,
            TabLast,
            TabMovePrev,
            TabMoveNext,
            TabRecent,
            TabRecentPrev,
            TabNextSame,
            TabPrevSame,
            TabCloseFiles,
            CursorDown,
            CursorUp,
            CursorFirst,
            CursorLast,
            CursorGo,
            CursorSplit,
            CursorVsplit,
            CursorTab,
            CursorFold,
            CursorClose,
            CursorCopy,
            CursorReveal,
            CursorBrowse,
            FilerDown,
            FilerUp,
            FilerInto,
            FilerOut,
            FilerOpen,
            FilerHidden,
            FileNew,
            DirectoryNew,
            WaitingNext,
            WaitingPrev,
            SessionNext,
            SessionPrev,
            WorkspaceNext,
            WorkspacePrev,
            SessionPick,
            SessionNew,
            SessionScratch,
            SessionResume,
            SessionBringBack,
            SessionRestart,
            NoticesOpen,
            SessionStop,
            SessionRename,
            SessionMarkUnread,
            NotificationsMute,
            SessionProfileNext,
            SessionProfilePrev,
            ShellOpen,
            TerminalStop,
            NeovimOpen,
            BrowserOpen,
            BrowserFind,
            BrowserReopen,
            TabClose,
            EnvironmentAdd,
            EnvironmentRemove,
            EnvironmentRename,
            EnvironmentRetry,
            EnvironmentPair,
            WorkspaceAdd,
            LocalIssues,
            LocalIssuesLeader,
            SavedCommands,
            ResourceAdd,
            ResourceEdit,
            ResourceRemove,
            ResourceKeep,
            ResourceScreenshot,
            WorkspaceRemove,
            WorktreeClose,
            WorktreeNew,
            WorktreeMerge,
            WorktreeDelete,
            WorktreeCleanup,
            WorktreeSleep,
            WorktreeWake,
            WorkspacePin,
            TreeSort,
            TreeHideSleeping,
            TreeHideIdle,
            TreeHideMain,
            TreeRevealCurrent,
            WindowClose,
            WindowOpen,
            Refresh,
            Filter,
            WorktreesToggle,
            ScrollDown,
            ScrollUp,
            ScrollHalfDown,
            ScrollHalfUp,
            ScrollTop,
            ScrollBottom,
            ConsoleSendNext,
            ConsolePassEveryKey,
            ConsoleRead,
            PromptFocus,
            PromptWrite,
            PromptSend,
            PromptImages,
            PromptTakeSuggestion,
            HistoryPrev,
            HistoryNext,
            SettingsOpen,
            DaemonRestart,
            KeysHelp,
            ZoomIn,
            ZoomOut,
            ZoomReset,
            TreeToggle,
            TreeFocus,
            SidebarNext,
            AgentsExpand,
            AgentsCollapse,
            AgentsReveal,
            DetailsToggle,
            DetailsStage,
            DetailsStageAll,
            DetailsDiscard,
            DetailsCommit,
            DetailsAmend,
            DetailsPull,
            DetailsPush,
            DetailsForcePush,
            DetailsCreatePull,
            DetailsDraftPull,
            DetailsGenerateCommit,
            DetailsFixCommit,
            DetailsAbort,
            DetailsResume,
            DetailsBase,
            DetailsResolve,
            DetailsNeovim,
            LogCopyHash,
            LogOpenRemote,
            LogExplain,
            LogBranch,
            LogWorktree,
            LogCherryPick,
            LogRevert,
            ViewerZoomIn,
            ViewerZoomOut,
            ViewerZoomFit,
            ViewerPageNext,
            ViewerPagePrev,
            ViewerPlay,
            ViewerBack,
            ViewerForward,
            ViewerCopyPath,
            ReviewComment,
            ReviewSelect,
            ReviewEdit,
            ReviewDelete,
            ReviewResolve,
            ReviewSend,
            ReviewDraft,
            DiffNextHunk,
            DiffPrevHunk,
            DiffNextFile,
            DiffPrevFile,
            DiffWrap,
            DiffFiles,
            DiffPictures,
            GithubNext,
            GithubPrev,
            GithubComment,
            GithubReply,
            GithubResolve,
            GithubReact,
            GithubReview,
            GithubFixChecks,
            GithubFixThreads,
            GithubMerge,
            GithubMergeSquash,
            GithubMergeCommit,
            GithubMergeRebase,
            GithubAutoMerge,
            GithubClose,
            GithubReady,
            CheckRerun,
            BoardOpen,
            BoardLeft,
            BoardRight,
            BoardDown,
            BoardUp,
            BoardFirst,
            BoardLast,
            BoardEnter,
            BoardIdle,
            BoardGroup,
            BoardExpand,
            BoardProject,
            BoardState,
            BoardClear,
            BoardSearch,
            LightboxNext,
            LightboxPrev,
            LightboxZoomIn,
            LightboxZoomOut,
            LightboxZoomFit,
            LightboxPanLeft,
            LightboxPanRight,
            LightboxPanUp,
            LightboxPanDown,
            LightboxClose,
            FloatingToggle,
            FloatingNew,
            FloatingCloseTab,
            FloatingNext,
            FloatingPrev,
            OverlayClose,
            OverlayDown,
            OverlayUp,
            OverlayChoose,
        ]
    }

    #[test]
    fn every_action_has_one_row_in_the_table() {
        for action in every_action() {
            let rows = ACTIONS
                .iter()
                .filter(|s| discriminant(&s.action) == discriminant(&action))
                .count();
            assert_eq!(rows, 1, "{action:?}");
        }
        assert_eq!(ACTIONS.len(), every_action().len());
    }

    #[test]
    fn no_two_actions_share_a_name() {
        let mut seen = HashSet::new();
        for spec in ACTIONS {
            assert!(seen.insert(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn every_action_reads_from_the_name_its_row_gives_and_back() {
        for spec in ACTIONS {
            let action = spec.action.numbered(3);
            assert_eq!(action.to_string().parse::<Action>(), Ok(action.clone()));
            assert!(action.to_string().starts_with(spec.name), "{}", spec.name);
        }
    }

    #[test]
    fn an_action_shown_in_a_menu_has_an_icon_and_one_only_a_key_has_none() {
        for spec in ACTIONS {
            let shown = spec.bar.is_some() || spec.row.is_some();
            assert_eq!(spec.icon.is_some(), shown, "{}", spec.name);
        }
    }

    #[test]
    fn every_action_but_none_says_what_it_does() {
        for action in every_action() {
            let said = action.label();
            assert_eq!(said.is_empty(), action == Action::None, "{action:?}");
        }
    }

    #[test]
    fn every_action_says_in_full_what_it_does_starting_in_lower_case() {
        for spec in ACTIONS {
            let first = spec.does.chars().next();
            assert!(first.is_some_and(|c| !c.is_uppercase()), "{}", spec.name);
            assert!(!spec.does.ends_with('.'), "{}", spec.name);
        }
    }

    #[test]
    fn an_action_on_the_menu_bar_says_what_the_menu_bar_calls_it() {
        assert_eq!(Action::SettingsOpen.label(), "Settings…");
        assert_eq!(Action::KeysHelp.label(), "Keyboard Shortcuts");
    }

    #[test]
    fn an_action_with_an_argument_says_the_argument() {
        let send: Action = "console.send Escape".parse().unwrap();
        assert!(send.label().contains("Escape"), "{}", send.label());
        assert!(Action::TabGo(3).label().contains('3'));
    }

    #[test]
    fn every_action_is_offered_by_its_name() {
        let names: Vec<&str> = Action::names().collect();
        assert!(names.contains(&"cursor.down"));
        assert!(names.contains(&"console.send"));
        assert!(!names.contains(&"none"));
        assert!(!names.contains(&"tab.go"));
        assert!(!names.contains(&"tab.merge"));
        for n in names {
            assert!(n.parse::<Action>().is_ok(), "{n}");
        }
    }

    #[test]
    fn an_action_reads_from_its_name_and_back() {
        let a: Action = "cursor.down".parse().unwrap();
        assert_eq!(a, Action::CursorDown);
        assert_eq!(a.to_string(), "cursor.down");
        assert!("cursor.sideways".parse::<Action>().is_err());
    }

    #[test]
    fn an_action_takes_an_argument_only_where_it_needs_one() {
        assert!("cursor.down 3".parse::<Action>().is_err());
        assert!("none x".parse::<Action>().is_err());
        assert!("tab.go".parse::<Action>().is_err());
        assert!("tab.go 0".parse::<Action>().is_err());
        assert_eq!("tab.go 9".parse::<Action>(), Ok(Action::TabGo(9)));
        assert!("tab.merge".parse::<Action>().is_err());
        assert_eq!("tab.merge 2".parse::<Action>(), Ok(Action::TabMerge(2)));
        assert_eq!("none".parse::<Action>(), Ok(Action::None));
    }

    #[test]
    fn sending_a_key_names_the_key_to_send() {
        let a: Action = "console.send Escape".parse().unwrap();
        assert_eq!(
            a,
            Action::ConsoleSend(Some(Chord::parse("Escape").unwrap()))
        );
        assert_eq!(a.to_string(), "console.send Escape");
        assert_eq!(
            "console.send".parse::<Action>().unwrap(),
            Action::ConsoleSend(None)
        );
    }
}
