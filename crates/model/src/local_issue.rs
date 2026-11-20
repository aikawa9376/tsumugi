//! A request kept locally until an agent session has worked on it.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::SessionId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewLocalIssue {
    pub id: String,
    pub dir: PathBuf,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalIssue {
    pub id: String,
    pub dir: PathBuf,
    pub title: String,
    pub body: String,
    pub state: LocalIssueState,
    pub record: Option<SessionId>,
    /// The terminal of its session, once one has been attached.
    pub session: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalIssueState {
    Open,
    Queued,
    Starting,
    Running,
    Waiting,
    Done,
    Closed,
    Failed,
    Cancelled,
}

impl LocalIssueState {
    pub fn word(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Closed => "closed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn can_close(self) -> bool {
        matches!(
            self,
            Self::Open | Self::Queued | Self::Failed | Self::Cancelled
        )
    }

    pub fn can_reopen(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Closed | Self::Failed | Self::Cancelled
        )
    }

    pub fn can_delete(self) -> bool {
        !matches!(self, Self::Starting | Self::Running | Self::Waiting)
    }
}
