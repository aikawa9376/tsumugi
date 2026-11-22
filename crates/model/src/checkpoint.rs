//! The one line a worktree shows of how its work stands, and the status it
//! is in, which its agent or the user sets.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where the work of a worktree stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeStatus {
    Todo,
    InProgress,
    InReview,
    Completed,
}

impl WorktreeStatus {
    pub const ALL: [Self; 4] = [
        Self::Todo,
        Self::InProgress,
        Self::InReview,
        Self::Completed,
    ];

    /// The word that sets it and filters by it.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in-progress",
            Self::InReview => "in-review",
            Self::Completed => "completed",
        }
    }

    /// What the user reads.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Todo => "To do",
            Self::InProgress => "In progress",
            Self::InReview => "In review",
            Self::Completed => "Completed",
        }
    }

    /// The status a word names, ignoring case, with `_` for `-` and the
    /// short forms `progress`, `review` and `done`.
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase().replace('_', "-");
        Self::ALL
            .into_iter()
            .find(|status| status.word() == word)
            .or(match word.as_str() {
                "progress" => Some(Self::InProgress),
                "review" => Some(Self::InReview),
                "done" => Some(Self::Completed),
                _ => None,
            })
    }
}

/// What a change does to one part of a checkpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Set<T> {
    /// Leaves it as it is.
    #[default]
    Keep,
    /// Leaves it empty.
    Clear,
    To(T),
}

/// The longest a comment is kept, in characters: it is one line.
pub const COMMENT_LIMIT: usize = 200;

/// What a worktree shows beside its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub worktree: PathBuf,
    /// One line of how the work stands; empty for none.
    #[serde(default)]
    pub comment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<WorktreeStatus>,
    /// When it was last set, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub updated_ms: u64,
}

impl Checkpoint {
    /// Whether it says nothing, so that it need not be kept.
    pub fn is_empty(&self) -> bool {
        self.comment.is_empty() && self.status.is_none()
    }

    /// What the worktree's row shows: the status and the comment.
    pub fn line(&self) -> String {
        match (self.status, self.comment.as_str()) {
            (Some(status), "") => status.label().to_string(),
            (Some(status), comment) => format!("{} · {comment}", status.label()),
            (None, comment) => comment.to_string(),
        }
    }
}

/// `text` as a comment: its first line, trimmed and cut to the limit.
pub fn comment_of(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .take(COMMENT_LIMIT)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_named_by_its_word_in_any_case_or_a_short_form() {
        for status in WorktreeStatus::ALL {
            assert_eq!(WorktreeStatus::parse(status.word()), Some(status));
        }
        for (word, status) in [
            ("In_Review", WorktreeStatus::InReview),
            (" DONE ", WorktreeStatus::Completed),
            ("progress", WorktreeStatus::InProgress),
            ("review", WorktreeStatus::InReview),
        ] {
            assert_eq!(WorktreeStatus::parse(word), Some(status), "{word}");
        }
        assert_eq!(WorktreeStatus::parse("blocked"), None);
        assert_eq!(WorktreeStatus::parse(""), None);
    }

    #[test]
    fn the_line_a_row_shows_leads_with_the_status_and_ends_with_the_comment() {
        let at = |comment: &str, status| Checkpoint {
            worktree: "/w".into(),
            comment: comment.into(),
            status,
            updated_ms: 0,
        };
        assert_eq!(
            at("tests are red", Some(WorktreeStatus::InProgress)).line(),
            "In progress · tests are red"
        );
        assert_eq!(at("", Some(WorktreeStatus::Todo)).line(), "To do");
        assert_eq!(at("only words", None).line(), "only words");
        assert!(at("", None).is_empty());
        assert!(!at("x", None).is_empty());
    }

    #[test]
    fn a_comment_is_the_first_line_trimmed_and_cut_to_the_limit() {
        assert_eq!(comment_of("  waiting on CI \nsecond line"), "waiting on CI");
        assert_eq!(comment_of(""), "");
        assert_eq!(comment_of(&"é".repeat(300)).chars().count(), COMMENT_LIMIT);
    }
}
