//! What a window shows, by the kind of thing it shows.

mod board;
mod check_log;
mod conversation;
mod diff;
mod draft;
mod focused;
mod github;
mod session;
mod terminal;
mod viewer;

pub(crate) use board::*;
pub(crate) use check_log::*;
pub(crate) use conversation::*;
pub(crate) use diff::*;
pub(crate) use draft::*;
pub(crate) use focused::*;
pub(crate) use github::*;
pub(crate) use session::*;
pub(crate) use terminal::*;
pub(crate) use viewer::*;
