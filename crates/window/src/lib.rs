//! The tsumugi window.

pub use tsumugi_core::actions;
mod adding;
mod alerts;
mod bridge;
mod browser;
mod build;
mod bundled_font;
mod client_keys;
mod client_settings;
mod clipboard;
use tsumugi_core::completion;
mod complete;
mod components;
mod connection;
mod console;
mod console_read;
mod content;
pub use tsumugi_core::file_icons;
use tsumugi_core::fonts;
pub use tsumugi_model::files;
mod details;
pub mod editor;
mod environments;
mod feed;
mod hosts;
mod icons;
mod input;
#[cfg(target_os = "linux")]
mod input_context;
mod installed_fonts;
pub use tsumugi_core::keymap;
mod keys;
use tsumugi_core::keymap::by_action as keys_by_action;
use tsumugi_core::keymap::edit as keys_edit;
use tsumugi_core::keymap::help as keys_help;
mod keys_help_view;
mod keys_pane;
mod layout;
mod link;
mod local_issues;
mod logins;
pub mod menu_bar;
#[cfg(target_os = "linux")]
mod native_layer;
mod native_terminal;
mod resources;
mod schedules;
#[cfg(target_os = "linux")]
pub fn kitty_display(args: &[String]) -> i32 {
    native_terminal::kitty_display(args)
}
mod picker;
pub use tsumugi_core::tree::nav;
use tsumugi_model::paths;
mod panes;
mod pictures;
pub mod prompts;
pub mod rename;
mod review;
use tsumugi_core::settings;
mod scaling;
mod serving_pane;
mod settings_window;
mod shell;
mod sidebar;
mod style;
mod themes;
mod tree;
mod view;
pub mod viewers;
pub use tsumugi_core::zoom;
mod window_socket;

pub use build::Build;
pub use shell::launch;
pub use window_socket::open_window;

#[cfg(test)]
#[path = "../../host/tests/support/daemon.rs"]
mod native_test_daemon;
