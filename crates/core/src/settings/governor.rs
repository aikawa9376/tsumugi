//! The limits on what sessions use of the work machine, and the gate command
//! of each repository.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// How the builds of a repository's worktrees share what they build.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BuildCache {
    /// Each worktree builds into its own `target/`, as it would by hand.
    #[default]
    Own,
    /// Every worktree of a repository builds into one folder Tsumugi keeps,
    /// `CARGO_TARGET_DIR`, where cargo's own lock lets one build run at a
    /// time.
    Shared,
    /// Each worktree builds into its own `target/` and `sccache` is the
    /// compiler wrapper (`RUSTC_WRAPPER`), so what one built is not built
    /// again.
    Sccache,
}

/// What sessions may use of the work machine. Every limit is off at `0`, so
/// nothing changes until you set one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GovernorSettings {
    /// The most agent sessions that run at once on the work machine. A session
    /// another opens waits for room, and is refused, with how many run, once
    /// it has waited too long; sessions you open yourself are never held.
    pub max_sessions: u32,
    /// How many seconds a session another opens waits for room under
    /// `max_sessions` before it is refused.
    pub wait_seconds: u32,
    /// How many jobs the builds of the sessions Tsumugi starts run at once
    /// (`CARGO_BUILD_JOBS`).
    pub build_jobs: u32,
    /// How the builds of a repository's worktrees share their work: `own`, `shared` or `sccache`.
    pub build_cache: BuildCache,
    /// The size, in GiB, of a repository's worktrees above which Tsumugi warns.
    pub worktree_warn_gib: u32,
    /// Whether cleaning up a worktree also deletes its branch, which it does
    /// only for a branch the base already holds.
    pub cleanup_branches: bool,
}

impl Default for GovernorSettings {
    fn default() -> Self {
        Self {
            max_sessions: 0,
            wait_seconds: 120,
            build_jobs: 0,
            build_cache: BuildCache::default(),
            worktree_warn_gib: 0,
            cleanup_branches: false,
        }
    }
}

/// The command that is the gate of one repository, and the failures it is
/// known to have.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RepositoryGate {
    /// The repository's primary checkout, the directory holding its `.git`.
    pub path: String,
    /// The command line that runs the gate, for example `just ci`.
    pub command: String,
    /// Failures the gate always has on this machine: a failed test whose line
    /// holds one of these words does not fail the gate.
    pub known_failures: Vec<String>,
}

/// The gate of each repository: a repository is checked only by a command the
/// user wrote, never one the repository lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GateSettings {
    /// The repositories with a gate, each `[[gate.repositories]]` table one.
    pub repositories: Vec<RepositoryGate>,
}

impl GateSettings {
    /// The gate of the repository whose primary checkout is `primary`.
    pub fn of(&self, primary: &Path) -> Option<&RepositoryGate> {
        self.repositories
            .iter()
            .find(|gate| Path::new(&gate.path) == primary && !gate.command.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn nothing_is_limited_until_the_user_says_so() {
        let settings = Settings::parse("").unwrap();
        assert_eq!(settings.governor, GovernorSettings::default());
        assert_eq!(settings.governor.max_sessions, 0);
        assert_eq!(settings.governor.wait_seconds, 120);
        assert_eq!(settings.governor.build_cache, BuildCache::Own);
        assert!(settings.gate.repositories.is_empty());
    }

    #[test]
    fn the_limits_and_the_gates_are_read_and_written_back() {
        let text = "[governor]\nmax_sessions = 3\nwait_seconds = 30\nbuild_jobs = 4\nbuild_cache = 'shared'\n\
                    worktree_warn_gib = 50\ncleanup_branches = true\n\
                    [[gate.repositories]]\npath = '/r/api'\ncommand = 'just ci'\n\
                    known_failures = ['flaky_dns']\n";
        let settings = Settings::parse(text).unwrap();
        assert_eq!(settings.governor.max_sessions, 3);
        assert_eq!(settings.governor.wait_seconds, 30);
        assert_eq!(settings.governor.build_cache, BuildCache::Shared);
        assert!(settings.governor.cleanup_branches);
        let gate = settings.gate.of("/r/api".as_ref()).unwrap();
        assert_eq!(gate.command, "just ci");
        assert_eq!(gate.known_failures, ["flaky_dns"]);
        assert_eq!(
            Settings::parse(&toml::to_string_pretty(&settings).unwrap()).unwrap(),
            settings
        );
    }

    #[test]
    fn a_repository_without_a_command_has_no_gate() {
        let settings =
            Settings::parse("[[gate.repositories]]\npath = '/r/api'\ncommand = '  '\n").unwrap();
        assert!(settings.gate.of("/r/api".as_ref()).is_none());
        assert!(settings.gate.of("/r/web".as_ref()).is_none());
    }

    #[test]
    fn what_is_not_a_setting_or_not_a_kind_of_cache_is_refused() {
        assert!(Settings::parse("[governor]\nmax_sessions = -1\n").is_err());
        assert!(Settings::parse("[governor]\nbuild_cache = 'nfs'\n").is_err());
        assert!(Settings::parse("[governor]\ncpu = 1\n").is_err());
        assert!(Settings::parse("[gate]\ncommand = 'just ci'\n").is_err());
    }
}
