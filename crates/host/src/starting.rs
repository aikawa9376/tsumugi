//! Starting an agent session headless, as `tsumugi session start` or another
//! session asks: its name, the worktree made for it, its launch, and what a
//! start that failed leaves behind.

use std::path::{Path, PathBuf};

use tsumugi_model::{SessionId, SetupChoice};
use tsumugi_term::Daemon;
use tsumugi_vcs::worktree::Worktree;

use crate::delegation::app::Recorded;
use crate::execution::StartRequest;
use crate::settings::Settings;

/// How far a failed start got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// Refused before its agent was asked to run.
    Refused,
    /// Its agent was asked to run and did not.
    NotStarted,
    /// Its agent may be running on the work machine all the same.
    MayHaveRun,
    /// Its worktree was made but its setup failed, so its agent was not
    /// asked to run. The worktree stays, with what the setup left in it.
    SetupFailed,
}

/// Why a start failed.
#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) stage: Stage,
    pub(crate) message: String,
}

impl Failure {
    pub(crate) fn new(stage: Stage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }
}

/// `label`, numbered when an agent session in `listed` other than the one
/// in terminal `except` goes by it.
pub(crate) fn fresh_name(
    label: &str,
    listed: &[tsumugi_term::Session],
    except: Option<u128>,
) -> String {
    let names: Vec<String> = crate::sessions::agents(listed)
        .filter(|s| except.is_none_or(|terminal| crate::sessions::id_of(&s.id) != Some(terminal)))
        .map(|s| s.name)
        .collect();
    let taken: Vec<&str> = names.iter().map(String::as_str).collect();
    crate::content::fresh_name(label, &taken)
}

/// The name a session goes by beside the sessions `listed`: the one given,
/// or one made from its prompt, numbered when taken.
pub(crate) fn name_for(
    given: Option<&str>,
    prompt: &str,
    listed: &[tsumugi_term::Session],
) -> String {
    let label = given
        .and_then(crate::content::label_of)
        .unwrap_or_else(|| crate::content::label_from(prompt));
    fresh_name(&label, listed, None)
}

/// Where a session starts: in `dir`, or in a new worktree of it made from
/// `base`.
pub(crate) struct Place {
    pub(crate) dir: PathBuf,
    pub(crate) worktree: bool,
    pub(crate) base: Option<String>,
}

/// What runs where the work host keeps its records and finds itself.
#[derive(Clone, Copy)]
pub(crate) struct Host<'a> {
    pub(crate) daemon: &'a Daemon,
    pub(crate) root: &'a Path,
    pub(crate) program: &'a Path,
}

/// A session started.
pub(crate) struct Started {
    pub(crate) recorded: Recorded,
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
}

/// Starts the session `request` asks for in `place`, opened by `opener`,
/// headless at the console's size. A worktree made for it is taken back when
/// its agent never ran.
pub(crate) async fn start(
    host: Host<'_>,
    settings: &Settings,
    opener: Option<SessionId>,
    place: Place,
    mut request: StartRequest,
) -> Result<Started, Failure> {
    let worktree = if place.worktree {
        let made = tsumugi_vcs::worktree::create(&place.dir, &request.name, place.base.as_deref())
            .await
            .map_err(|error| Failure::new(Stage::Refused, error))?;
        Some(made)
    } else {
        None
    };
    if let Some(made) = &worktree {
        // No person is there to ask, so a policy of `ask` runs nothing.
        let set_up = crate::worktree_setup::set_up(
            host.daemon,
            settings,
            &place.dir,
            &made.path,
            SetupChoice::Inherit,
            false,
        )
        .await;
        match set_up {
            Ok(done) => {
                for warning in done.warnings {
                    tracing::warn!(worktree = %made.path.display(), "{warning}");
                }
            }
            Err(error) => {
                return Err(Failure::new(
                    Stage::SetupFailed,
                    format!("cannot start {}: {error}", request.name),
                ));
            }
        }
    }
    request.cwd = worktree
        .as_ref()
        .map_or(place.dir, |made| made.path.clone());
    let launched = launch(host, settings, opener, request).await;
    settled(worktree, launched).await
}

async fn launch(
    host: Host<'_>,
    settings: &Settings,
    opener: Option<SessionId>,
    request: StartRequest,
) -> Result<Started, Failure> {
    let refused = |error| Failure::new(Stage::Refused, error);
    let (name, dir) = (request.name.clone(), request.cwd.clone());
    let palette = crate::themes::palette_in_force(settings).map_err(refused)?;
    let (resolved, recorded) =
        crate::delegation::app::prepare(host.root, settings, request, opener, host.program)
            .await
            .map_err(refused)?;
    let mut spawn = resolved.spawn;
    spawn.cols = crate::execution::COLS;
    spawn.rows = crate::execution::ROWS;
    let attach = recorded.clone();
    let started = crate::execution::spawn_headless_prepared(
        host.daemon,
        spawn,
        &palette,
        move |terminal| async move { attach.attach(terminal).await },
    )
    .await;
    let Err(error) = started else {
        return Ok(Started {
            recorded,
            name,
            dir,
        });
    };
    let message = format!(
        "cannot start {name}: {}",
        crate::execution::spawn_failure(&error)
    );
    if crate::remote::active() && crate::execution::spawn_may_have_happened(&error) {
        return Err(Failure::new(Stage::MayHaveRun, message));
    }
    recorded.failed().await;
    Err(Failure::new(Stage::NotStarted, message))
}

/// `launched`, with the worktree made for it taken back unless its agent may
/// have run or its setup failed, so a start that never ran leaves no
/// directory or branch behind, and what a setup that stopped left in a
/// worktree is the user's to look at.
pub(crate) async fn settled<T>(
    worktree: Option<Worktree>,
    launched: Result<T, Failure>,
) -> Result<T, Failure> {
    match launched {
        Err(mut failure) if !matches!(failure.stage, Stage::MayHaveRun | Stage::SetupFailed) => {
            if let Some(worktree) = worktree
                && let Err(left) = tsumugi_vcs::worktree::remove(&worktree).await
            {
                failure.message = format!(
                    "{}; the worktree made for it is left at {}: {left}",
                    failure.message,
                    worktree.path.display()
                );
            }
            Err(failure)
        }
        launched => launched,
    }
}

#[cfg(test)]
mod tests;
