//! The Android app's core: everything below its interface, from pairing and
//! the pinned connection to the work protocol, which its Kotlin reaches
//! through the bindings uniffi generates.

mod ffi;
pub mod link;
pub mod machines;
pub mod pairing;
pub mod reply;
pub mod waits;

pub use ffi::*;

uniffi::setup_scaffolding!();
