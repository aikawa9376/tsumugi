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
}
