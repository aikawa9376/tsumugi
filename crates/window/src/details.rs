//! The details panel as the window draws it: its tabs, the components of
//! the tab shown and of its foot, and the rows opened from them.

pub(crate) mod changes;
mod diff_text;
mod github;
mod log;
mod message;
mod overview;
mod panel;
mod parts;
mod process;
mod subagent;
mod timeline;
mod usage;

pub(crate) use panel::*;
pub(crate) use tsumugi_core::details::*;
pub(crate) use usage::Gauges;
