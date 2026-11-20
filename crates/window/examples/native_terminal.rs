//! A Linux trial of a native terminal widget over the desktop webview.
//! Run with VTE_LIBRARY naming libvte-2.91.so.0 and
//! TSUMUGI_NATIVE_TERMINAL_RESOURCE naming a terminal resource to attach.
//!
//! Create that resource with `tsumugi terminal start --background --program
//! command -- /bin/bash --noprofile --norc -i`. Pass its resource id to this
//! example together with the terminal host's TSUMUGI_DAEMON socket.
//! GDK_BACKEND=wayland exercises the native Wayland path;
//! TSUMUGI_NATIVE_AUTOTEST=1 verifies input through VTE's commit signal,
//! resizing, hiding the view and recreating it without replacing the session.
//!
//! This is a rendering/lifetime trial, not a selectable application backend.
//! Mouse reporting, images, all terminal input modes and application shortcuts
//! still need integration before this can replace the production console.

#[cfg(target_os = "linux")]
#[path = "native_terminal/linux.rs"]
mod linux;

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The native terminal trial currently runs on Linux.");
}
