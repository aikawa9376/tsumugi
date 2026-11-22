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
    /// Wait until every condition given holds on the page; with none, only
    /// pause for `timeout`.
    Wait {
        /// An element this CSS selector matches.
        #[serde(default)]
        selector: Option<String>,
        /// Whether the selector's element is to be `visible` (the default) or
        /// `hidden`.
        #[serde(default)]
        state: Option<ElementState>,
        /// Text the page shows.
        #[serde(default)]
        text: Option<String>,
        /// A pattern of the page's address, `*` standing for anything but a
        /// slash and `**` for anything.
        #[serde(default)]
        url: Option<String>,
        /// How far the page has loaded.
        #[serde(default)]
        load: Option<LoadState>,
        /// How long to wait, in milliseconds.
        #[serde(default)]
        timeout: Option<u64>,
    },
    /// What the page wrote to its console, the latest entries.
    Console {
        #[serde(default)]
        limit: Option<usize>,
    },
    /// The requests the page made, the latest entries.
    Network {
        #[serde(default)]
        limit: Option<usize>,
    },
    /// Start recording the page's console and requests afresh.
    CaptureStart,
    /// Stop recording them, keeping what was recorded.
    CaptureStop,
    /// Show the page as a device does, by the name of a size the browser
    /// offers, or as it fills its room again.
    SetDevice { name: String },
    /// Take a picture of the whole page, beyond what is visible.
    FullScreenshot {
        #[serde(default)]
        format: Option<ImageFormat>,
    },
    /// Export the page as a PDF.
    Pdf,
}

/// Whether an element waited for is to be seen or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementState {
    Visible,
    Hidden,
}

/// How far a page has loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoadState {
    /// The document and what it holds have loaded.
    Load,
    /// The document has been read.
    DomContentLoaded,
    /// The page has loaded and has made no request for a moment.
    NetworkIdle,
}

/// What a picture is encoded as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// How long a wait for a condition lasts when the command gives no time.
pub const WAIT_DEFAULT_MS: u64 = 25_000;
/// The longest a command may be told to wait.
pub const WAIT_LONGEST_MS: u64 = 300_000;

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
            Self::Wait { .. } => "wait",
            Self::Console { .. } => "console",
            Self::Network { .. } => "network",
            Self::CaptureStart => "capture start",
            Self::CaptureStop => "capture stop",
            Self::SetDevice { .. } => "set device",
            Self::FullScreenshot { .. } => "full-screenshot",
            Self::Pdf => "pdf",
        }
    }

    /// Whether the command asks for something that can be done: a wait is
    /// not longer than the longest, and a list is not of nothing.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Wait {
                timeout: Some(timeout),
                ..
            } if *timeout > WAIT_LONGEST_MS => Err(format!(
                "timeout takes milliseconds, up to {WAIT_LONGEST_MS}"
            )),
            Self::Console { limit: Some(0) } | Self::Network { limit: Some(0) } => {
                Err("limit takes a number above 0".into())
            }
            Self::SetDevice { name } if name.trim().is_empty() => Err("name the device".into()),
            _ => Ok(()),
        }
    }

    /// How long the window may take over the command before the host stops
    /// waiting for it: a wait outlives its own timeout, so that the window
    /// reports the timeout rather than the host.
    pub fn allowance(&self, base: std::time::Duration) -> std::time::Duration {
        match self {
            Self::Wait { timeout, .. } => {
                let timeout = timeout.unwrap_or(WAIT_DEFAULT_MS).min(WAIT_LONGEST_MS);
                base.max(
                    std::time::Duration::from_millis(timeout) + std::time::Duration::from_secs(30),
                )
            }
            _ => base,
        }
    }
}

/// What came of a command, as the window that carried it out says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserResult {
    /// What it read or did, as the agent is told it.
    Done { value: serde_json::Value },
    /// The picture asked for, in base64: a PNG unless `mime` says otherwise.
    Image {
        data: String,
        #[serde(default = "png")]
        mime: String,
    },
    /// The PDF asked for, in base64.
    Pdf { data: String },
    /// It could not be done, for this reason, which the agent is told.
    Failed { text: String },
}

fn png() -> String {
    "image/png".into()
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
        assert_eq!(BrowserCommand::CaptureStart.name(), "capture start");
        assert_eq!(BrowserCommand::Pdf.name(), "pdf");
    }

    #[test]
    fn a_wait_names_its_conditions_the_way_orca_does_and_leaves_the_rest_out() {
        let wait: BrowserCommand = serde_json::from_str(
            r#"{"action":"wait","text":"Welcome","load":"networkidle","state":"hidden","timeout":500}"#,
        )
        .unwrap();
        assert_eq!(
            wait,
            BrowserCommand::Wait {
                selector: None,
                state: Some(ElementState::Hidden),
                text: Some("Welcome".into()),
                url: None,
                load: Some(LoadState::NetworkIdle),
                timeout: Some(500),
            }
        );
        let load: BrowserCommand =
            serde_json::from_str(r#"{"action":"wait","load":"domcontentloaded"}"#).unwrap();
        assert!(matches!(
            load,
            BrowserCommand::Wait {
                load: Some(LoadState::DomContentLoaded),
                ..
            }
        ));
        assert!(
            serde_json::from_str::<BrowserCommand>(r#"{"action":"wait","load":"idle"}"#).is_err()
        );
    }

    #[test]
    fn a_command_that_asks_for_nothing_doable_is_refused() {
        assert!(BrowserCommand::Console { limit: Some(0) }.check().is_err());
        assert!(BrowserCommand::Network { limit: Some(0) }.check().is_err());
        assert!(BrowserCommand::Console { limit: Some(1) }.check().is_ok());
        assert!(
            BrowserCommand::SetDevice { name: " ".into() }
                .check()
                .is_err()
        );
        let wait = |timeout| BrowserCommand::Wait {
            selector: None,
            state: None,
            text: None,
            url: None,
            load: None,
            timeout,
        };
        assert!(wait(Some(WAIT_LONGEST_MS)).check().is_ok());
        assert!(wait(Some(WAIT_LONGEST_MS + 1)).check().is_err());
        assert!(wait(None).check().is_ok());
    }

    #[test]
    fn a_picture_without_a_kind_is_a_png() {
        let image: BrowserResult =
            serde_json::from_str(r#"{"type":"image","data":"AAAA"}"#).unwrap();
        assert_eq!(
            image,
            BrowserResult::Image {
                data: "AAAA".into(),
                mime: "image/png".into()
            }
        );
        assert_eq!(ImageFormat::Jpeg.mime(), "image/jpeg");
    }

    #[test]
    fn a_wait_outlives_its_own_timeout_by_half_a_minute_and_nothing_else_is_given_more() {
        use std::time::Duration;
        let base = Duration::from_secs(60);
        let wait = |timeout| BrowserCommand::Wait {
            selector: None,
            state: None,
            text: None,
            url: None,
            load: None,
            timeout,
        };
        assert_eq!(wait(None).allowance(base), base);
        assert_eq!(
            wait(Some(120_000)).allowance(base),
            Duration::from_secs(150)
        );
        // A wait is never held longer than the longest it may be told to.
        assert_eq!(
            wait(Some(u64::MAX)).allowance(base),
            Duration::from_millis(WAIT_LONGEST_MS) + Duration::from_secs(30)
        );
        assert_eq!(BrowserCommand::Pdf.allowance(base), base);
    }
}
