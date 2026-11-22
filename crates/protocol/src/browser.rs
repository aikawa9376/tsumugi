//! What a session's agent asks of the browser pages of its worktree. The pages
//! render in the window, so the work host relays each command to a window and
//! carries its result back to the agent.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One thing an agent does with the browser of its worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserCommand {
    /// Point the page at an address, opening a page when the worktree has none.
    Goto { url: String },
    /// Read the page as a tree of what is on it, naming the elements that can
    /// be acted on by references.
    Snapshot {
        /// Only the controls and links.
        #[serde(default)]
        interactive: bool,
        /// Only what changed since the last snapshot taken this way.
        #[serde(default)]
        delta: bool,
        /// Only the part of the page below the first element this CSS selector
        /// matches.
        #[serde(default)]
        selector: Option<String>,
        /// Only this many levels of the tree.
        #[serde(default)]
        depth: Option<u32>,
    },
    /// Click the element a snapshot referred to.
    Click { element: String },
    /// Replace what the field a snapshot referred to holds.
    Fill { element: String, value: String },
    /// Take a picture of the part of the page that is visible.
    Screenshot,
    /// List the worktree's pages.
    TabList,
    /// Open a page, behind the one shown.
    TabCreate {
        #[serde(default)]
        url: Option<String>,
    },
    /// Make the page counted `index` in the list the one commands act on.
    TabSwitch { index: usize },
}

impl BrowserCommand {
    /// The command's name as the agent knows it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Goto { .. } => "goto",
            Self::Snapshot { .. } => "snapshot",
            Self::Click { .. } => "click",
            Self::Fill { .. } => "fill",
            Self::Screenshot => "screenshot",
            Self::TabList => "tab list",
            Self::TabCreate { .. } => "tab create",
            Self::TabSwitch { .. } => "tab switch",
        }
    }
}

/// What came of a command, as the window that carried it out says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserResult {
    /// What it read or did, as the agent is told it.
    Done { value: serde_json::Value },
    /// The picture asked for: a PNG, in base64.
    Image { data: String },
    /// It could not be done, for this reason, which the agent is told.
    Failed { text: String },
}

/// A command the work host hands a window: `call` numbers it for the answer,
/// and `dir` is where the session that asked works, which names the worktree
/// whose pages it may act on and no other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserCall {
    pub call: u64,
    pub dir: PathBuf,
    pub command: BrowserCommand,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_written_by_its_action_and_leaves_out_what_it_does_not_say() {
        let read: BrowserCommand = serde_json::from_str(r#"{"action":"snapshot"}"#).unwrap();
        assert_eq!(
            read,
            BrowserCommand::Snapshot {
                interactive: false,
                delta: false,
                selector: None,
                depth: None
            }
        );
        let open: BrowserCommand = serde_json::from_str(r#"{"action":"tab_create"}"#).unwrap();
        assert_eq!(open, BrowserCommand::TabCreate { url: None });
        assert!(serde_json::from_str::<BrowserCommand>(r#"{"action":"eval"}"#).is_err());
    }

    #[test]
    fn every_command_has_a_name() {
        assert_eq!(BrowserCommand::Screenshot.name(), "screenshot");
        assert_eq!(BrowserCommand::TabSwitch { index: 1 }.name(), "tab switch");
    }
}
