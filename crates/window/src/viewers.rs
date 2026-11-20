//! Viewers for files the window draws itself: images, movies, HTML pages,
//! Markdown documents and PDFs, each only reading its file.

mod conversation;
mod markdown;
mod pdf;
mod serve;
mod thread;
mod trial;
mod view;

pub use conversation::*;
pub use markdown::*;
pub use pdf::*;
pub use serve::*;
pub use thread::*;
pub use trial::launch as try_in_window;
pub use trial::{Resolved, resolve};
pub use tsumugi_core::viewers::*;
pub use view::*;
