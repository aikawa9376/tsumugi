//! The human conversation, apart from the tools and instructions it ran with.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
    pub at_ms: Option<u64>,
    /// The provider recorded thinking rather than text addressed to the user.
    pub thinking: bool,
    /// The tool the assistant called, when this message is that call rather
    /// than words: `text` is then empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
}

/// One tool call and what it returned, as the provider recorded them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    /// The provider's id for the call, which its result is found by.
    pub id: String,
    pub name: String,
    /// The call in one line: the command run, the file touched.
    pub summary: String,
    /// What the call was given, as far as is worth reading.
    pub input: String,
    /// What it returned, once it did.
    pub output: Option<String>,
    pub failed: bool,
}

/// How much of a call's input or output is kept.
pub const KEPT: usize = 4000;

/// `text` cut to [`KEPT`] characters, saying so when it was cut.
pub fn kept(text: &str) -> String {
    match text.char_indices().nth(KEPT) {
        Some((end, _)) => format!("{}\n… cut", &text[..end]),
        None => text.to_string(),
    }
}
