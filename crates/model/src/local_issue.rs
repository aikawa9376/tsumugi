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
    /// The schedule that queued it, by id, when one did.
    pub schedule: Option<String>,
    /// The attempts it was last started as, each in a worktree of its own;
    /// none when one session works it where it is.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<Attempt>,
}

/// One way to try an issue: the profile to start, its default when none is
/// named, and the model, the profile's own when none is named.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TryAs {
    /// The profile it starts from, by id (see `models`); the default profile when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// A model the profile lists (see `models`); its default when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl TryAs {
    /// `<profile>[:<model>]`, the way a command line names an attempt.
    pub fn parse(word: &str) -> Option<Self> {
        let (profile, model) = match word.split_once(':') {
            Some((profile, model)) => (profile.trim(), Some(model.trim())),
            None => (word.trim(), None),
        };
        if profile.is_empty() || model.is_some_and(str::is_empty) {
            return None;
        }
        Some(Self {
            profile: Some(profile.into()),
            model: model.map(Into::into),
        })
    }
}

/// An attempt at an issue: a session started for it in a worktree made from
/// the commit every attempt of the issue starts from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    /// Its place among the issue's attempts, from 1.
    pub number: u32,
    pub tried_as: TryAs,
    /// Only `Starting`, `Running`, `Waiting`, `Done` and `Failed`.
    pub state: LocalIssueState,
    pub record: Option<SessionId>,
    /// The terminal of its session, once one has been attached.
    pub session: Option<String>,
    /// The worktree its session works in, once it has one.
    pub worktree: Option<PathBuf>,
    /// The commit every attempt of the issue was made from.
    pub base: String,
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

    /// How an issue stands that its attempts, standing as `attempts`, work:
    /// worked while any works, waiting on the user while one waits and none
    /// works, done once every one has ended, and failed only when all have.
    pub fn of_attempts(attempts: &[Self]) -> Self {
        let any = |state: Self| attempts.contains(&state);
        if attempts.is_empty() || attempts.iter().all(|s| *s == Self::Failed) {
            Self::Failed
        } else if any(Self::Running) {
            Self::Running
        } else if any(Self::Waiting) {
            Self::Waiting
        } else if any(Self::Starting) {
            Self::Starting
        } else {
            Self::Done
        }
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

    #[test]
    fn an_issue_stands_by_all_its_attempts_together() {
        use LocalIssueState::*;
        let cases: &[(&[LocalIssueState], LocalIssueState)] = &[
            (&[Running, Done], Running),
            (&[Waiting, Running], Running),
            (&[Waiting, Done], Waiting),
            (&[Starting, Done], Starting),
            (&[Starting, Waiting], Waiting),
            (&[Done, Failed], Done),
            (&[Done, Done], Done),
            (&[Failed, Failed], Failed),
            (&[Failed, Running], Running),
        ];
        for (attempts, want) in cases {
            assert_eq!(
                LocalIssueState::of_attempts(attempts),
                *want,
                "{attempts:?}"
            );
        }
    }

    #[test]
    fn an_attempt_is_named_by_its_profile_and_maybe_a_model() {
        let named = |profile: &str, model: Option<&str>| TryAs {
            profile: Some(profile.into()),
            model: model.map(Into::into),
        };
        assert_eq!(TryAs::parse("codex"), Some(named("codex", None)));
        assert_eq!(
            TryAs::parse("claude:opus"),
            Some(named("claude", Some("opus")))
        );
        assert_eq!(
            TryAs::parse("opencode:openai/gpt-5:high"),
            Some(named("opencode", Some("openai/gpt-5:high")))
        );
        assert_eq!(TryAs::parse(""), None);
        assert_eq!(TryAs::parse(":opus"), None);
        assert_eq!(TryAs::parse("codex:"), None);
    }
}
