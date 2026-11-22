//! The work host: the work it holds and serves over its socket, the
//! terminal host it starts and the sessions and shells it starts there,
//! delegation between sessions, the commands that ask it, and what is kept
//! on disk from one run to the next.

mod appearance;
mod asks;
pub mod awake;
pub mod cleanup;
pub mod cli;
mod command;
mod completion;
pub mod compose;
pub mod config;
pub mod daemon;
pub mod delegation;
pub mod details;
pub mod drafts;
pub mod editor;
pub mod env;
pub mod environment;
pub mod execution;
mod explain;
pub mod gate;
mod gate_cli;
pub mod gateway;
mod generate;
pub mod governor;
mod held;
mod issue_cli;
mod jobs;
pub mod kept;
mod leader;
mod local_issues;
mod making;
mod mutation;
mod neovim;
mod notices;
pub mod pasted;
pub mod paths;
pub mod pictures;
pub mod ports;
mod presences;
pub mod processes;
pub mod programs;
pub mod remote;
mod resources;
pub mod restore;
pub mod review_comments;
mod saved_commands;
mod schedule_cli;
mod schedule_time;
mod schedules;
pub mod scratch;
pub mod serve;
pub mod service;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod standing;
pub mod start;
mod starting;
pub mod system_usage;
mod terminal_host;
pub mod terminals;
#[cfg(test)]
mod testing;
pub mod themes;
pub mod watch;
pub mod work;
pub mod work_host;
pub mod workspaces;
pub mod worktree_setup;
mod worktrees;

use tsumugi_core::content;

pub use daemon::cli as daemon_cli;
pub use gate_cli::cli as gate_cli;
pub use issue_cli::cli as issue_cli;
pub use kept::{KEEP, keep};
pub use ports::cli as ports_cli;
pub use schedule_cli::cli as schedule_cli;
pub use start::cli as session_cli;
pub use terminals::cli as terminal_cli;
pub use tsumugi_term::gate::run as startup_lease;
#[cfg(windows)]
pub use tsumugi_term::{HOLD, hold};
pub use work_host::cli as host_cli;

#[cfg(test)]
mod review_tests;
