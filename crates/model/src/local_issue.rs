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

/// What can be done to a saved issue, by its own word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalIssueAction {
    Start,
    Cancel,
    Retry,
    Close,
    Reopen,
    Delete,
}

impl LocalIssueAction {
    pub const ALL: [Self; 6] = [
        Self::Start,
        Self::Cancel,
        Self::Retry,
        Self::Close,
        Self::Reopen,
        Self::Delete,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Cancel => "cancel",
            Self::Retry => "retry",
            Self::Close => "close",
            Self::Reopen => "reopen",
            Self::Delete => "delete",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.word() == word)
    }
}

impl LocalIssueState {
    pub const ALL: [Self; 9] = [
        Self::Open,
        Self::Queued,
        Self::Starting,
        Self::Running,
        Self::Waiting,
        Self::Done,
        Self::Closed,
        Self::Failed,
        Self::Cancelled,
    ];

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.word() == word)
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_and_action_is_read_back_from_its_word() {
        for state in LocalIssueState::ALL {
            assert_eq!(LocalIssueState::parse(state.word()), Some(state));
        }
        for action in LocalIssueAction::ALL {
            assert_eq!(LocalIssueAction::parse(action.word()), Some(action));
        }
        assert_eq!(LocalIssueState::parse("Open"), None);
        assert_eq!(LocalIssueAction::parse("remove"), None);
    }
}
