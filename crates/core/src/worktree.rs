//! Making worktrees from the window and deleting them: the dialog that asks
//! for a new one, the rows of those being made, and the question asked
//! before one is deleted.

mod composer;
mod delete;
mod pending;

pub use composer::{Composer, Field, Starts};
pub use pending::{PIPELINE, Pending, Progress, Step};

pub(crate) use composer::{composer_key, cursor_at, focus, open, read_starts, type_into};
pub(crate) use delete::{confirmed, delete, inspected, removed, toggle};
pub(crate) use pending::{dismiss, made};

#[cfg(test)]
mod tests;
