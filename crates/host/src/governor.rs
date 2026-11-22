//! What sessions may use of the work machine: how many agents run at once,
//! how their builds share, and how large a repository's worktrees have grown.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tsumugi_core::settings::{BuildCache, GovernorSettings};

const GIB: u64 = 1024 * 1024 * 1024;

/// How often a waiting start looks again.
pub const LOOK_EVERY: Duration = Duration::from_secs(2);

/// The limits in force, as the settings say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    max_sessions: u32,
    /// How long a start waits for room before it is refused.
    pub patience: Duration,
    build_jobs: u32,
    cache: BuildCache,
    warn_bytes: u64,
}

impl From<&GovernorSettings> for Limits {
    fn from(settings: &GovernorSettings) -> Self {
        Self {
            max_sessions: settings.max_sessions,
            patience: Duration::from_secs(u64::from(settings.wait_seconds)),
            build_jobs: settings.build_jobs,
            cache: settings.build_cache,
            warn_bytes: u64::from(settings.worktree_warn_gib) * GIB,
        }
    }
}

/// Whether another session may start now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Room,
    Full { running: usize, max: u32 },
}

impl Admission {
    /// What a start that waited in vain is told.
    pub fn refusal(self) -> Option<String> {
        match self {
            Self::Room => None,
            Self::Full { running, max } => Some(format!(
                "{running} of {max} agent sessions run, the most the user allows at once; \
                 try again when one has ended, or ask the user"
            )),
        }
    }
}

impl Limits {
    /// Whether a session may start while `running` agent sessions run.
    pub fn admit(&self, running: usize) -> Admission {
        match self.max_sessions {
            0 => Admission::Room,
            max if running < max as usize => Admission::Room,
            max => Admission::Full { running, max },
        }
    }

    /// The variables the builds of a session working in a worktree of the
    /// repository whose primary checkout is `primary` are given. `shared` is
    /// where Tsumugi keeps what worktrees build together.
    pub fn environment(&self, primary: &Path, shared: &Path) -> Vec<(String, String)> {
        let mut env = Vec::new();
        if self.build_jobs > 0 {
            env.push(("CARGO_BUILD_JOBS".into(), self.build_jobs.to_string()));
        }
        match self.cache {
            BuildCache::Own => {}
            BuildCache::Shared => env.push((
                "CARGO_TARGET_DIR".into(),
                folder_of(shared, primary).display().to_string(),
            )),
            BuildCache::Sccache => env.push(("RUSTC_WRAPPER".into(), "sccache".into())),
        }
        env
    }

    /// The warning a repository's worktrees earn by their `sizes`, if any.
    pub fn warning(&self, sizes: &[(PathBuf, u64)]) -> Option<String> {
        let total: u64 = sizes.iter().map(|(_, size)| size).sum();
        if self.warn_bytes == 0 || total <= self.warn_bytes {
            return None;
        }
        let mut largest: Vec<&(PathBuf, u64)> = sizes.iter().collect();
        largest.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
        let named: Vec<String> = largest
            .iter()
            .take(3)
            .map(|(path, size)| format!("{} ({})", path.display(), gib(*size)))
            .collect();
        Some(format!(
            "the worktrees take {} of the {} allowed; the largest are {}",
            gib(total),
            gib(self.warn_bytes),
            named.join(", ")
        ))
    }
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
}

/// The folder in `shared` that belongs to the repository at `primary`: one named for the repository, told from another of the same
/// name by a hash of where it is.
pub fn folder_of(shared: &Path, primary: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in primary.to_string_lossy().bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    let name = primary
        .file_name()
        .map_or_else(|| "repository".into(), |n| n.to_string_lossy().into_owned());
    shared.join(format!("{name}-{hash:016x}"))
}

/// The bytes the files under `path` take, links not followed.
pub fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().map(|e| size_of(&e.path())).sum())
        .unwrap_or(0)
}

/// Where Tsumugi keeps what the worktrees of a repository build together.
pub fn shared_root() -> PathBuf {
    crate::paths::data_dir().join("build")
}

/// How many agent sessions run in `listed`.
pub fn running(listed: &[tsumugi_term::Session]) -> usize {
    listed
        .iter()
        .filter(|session| !session.exited)
        .filter(|session| {
            matches!(
                crate::sessions::read(session),
                Some(crate::sessions::Kept::Agent(_))
            )
        })
        .count()
}

/// Waits until `limits` have room for a session, looking again every `every`
/// as `count` tells how many run, and gives up after `patience` with what the
/// start is refused with.
pub async fn wait_for_room<F, Fut>(
    limits: &Limits,
    patience: Duration,
    every: Duration,
    mut count: F,
) -> Result<(), String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<usize, String>>,
{
    let deadline = tokio::time::Instant::now() + patience;
    loop {
        let admission = limits.admit(count().await?);
        let Some(refusal) = admission.refusal() else {
            return Ok(());
        };
        if tokio::time::Instant::now() + every > deadline {
            return Err(refusal);
        }
        tokio::time::sleep(every).await;
    }
}

#[cfg(test)]
mod tests;
