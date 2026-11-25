//! Making worktrees from the window and deleting them: the dialog that asks
//! for a new one, the rows of those being made, and the question asked
//! before one is deleted.

mod branch;
mod cleanup;
pub(crate) mod composer;
mod delete;
mod merge;
mod pending;
mod sleep;
mod status;

pub use cleanup::{Cleanup, Doomed, Row, size as cleanup_size};
pub use composer::{Composer, Field, Starts};
pub use pending::{PIPELINE, Pending, Progress, Step};

pub(crate) use branch::{diff as branch_diff, draft as branch_draft};
pub(crate) use cleanup::{
    deletions, key as cleanup_key, open as cleanup_open, read as cleanup_read,
    toggle as cleanup_toggle,
};
pub(crate) use composer::{composer_key, cursor_at, focus, open, open_at, read_starts, type_into};
pub(crate) use delete::{confirmed as delete_confirmed, delete, inspected, removed};
pub(crate) use merge::{
    confirmed as merge_confirmed, inspected as merge_inspected, merge, merged, resolved,
};
pub(crate) use pending::{answered, dismiss, made, set_up};
pub(crate) use sleep::{confirmed as slept_confirmed, sleep, slept, wake};
pub(crate) use status::{set as set_status, status_of};

/// The choice of the question open flipped, whichever question it is.
pub(crate) fn toggle(state: &mut crate::state::ViewState) -> Vec<crate::effect::Effect> {
    let mut effects = delete::toggle(state);
    effects.extend(merge::toggle(state));
    effects
}

#[cfg(test)]
mod tests;
