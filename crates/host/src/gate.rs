//! The central gate: the one command that decides whether a commit is good,
//! run for the sessions that ask, one at a time, where the user can read it.
//!
//! A session never runs the full gate itself. It asks, and the work host runs
//! the repository's gate command in a worktree of its own at the commit asked,
//! in a terminal that stays with its output. An ask for a commit that is
//! already queued or running joins that run rather than starting another.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::watch;
use tsumugi_core::settings::RepositoryGate;
use tsumugi_model::TerminalProgram;
use tsumugi_term::Daemon;

use crate::execution::TerminalStartRequest;
use crate::settings::Settings;

/// How long an ask waits for its answer before it says how far the run has
/// got; asking again joins the same run.
pub const PATIENCE: Duration = Duration::from_secs(600);
/// The lines of the end of the output an answer carries.
const TAIL_LINES: usize = 40;
/// How much of the end of the output is read back.
const READ_BACK: u64 = 256 * 1024;

/// What is asked to be checked: a commit of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub primary: PathBuf,
    pub commit: String,
}

/// How a run of the gate ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub commit: String,
    /// Whether the commit passes: the command succeeded, or it failed only
    /// in tests the user lists as known failures.
    pub passed: bool,
    pub exit: Option<i32>,
    /// The failed tests the command's output names, without the known ones.
    pub failed: Vec<String>,
    /// The failed tests that were left out as known.
    pub known: Vec<String>,
    /// The terminal that kept the output, as `tsumugi terminal` names it.
    pub terminal: Option<String>,
    /// The end of the output.
    pub tail: String,
}

/// How far an ask for the gate has got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Waits for the runs `ahead` of it, the one running included.
    Queued {
        ahead: usize,
    },
    Running,
    Done(Result<Report, String>),
}

/// What an ask is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub progress: Progress,
    /// Whether the ask joined a run another had started.
    pub joined: bool,
}

type Entries = HashMap<Key, watch::Receiver<Progress>>;

/// The runs asked for on a work host, which go one at a time.
#[derive(Default)]
pub struct Gate {
    entries: Arc<Mutex<Entries>>,
    turn: Arc<tokio::sync::Mutex<()>>,
}

impl Gate {
    /// Asks for `key` to be checked by `run`, and waits up to `patience` for
    /// the answer. An ask for what is already queued or running joins it, and
    /// the runs go in the order they were asked, one at a time. A run goes on
    /// whether or not anyone still waits for it.
    pub async fn request<F, Fut>(&self, key: Key, patience: Duration, run: F) -> Answer
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Report, String>> + Send + 'static,
    {
        let (mut seen, joined) = {
            let mut entries = self.entries.lock().expect("the gate's lock");
            match entries.get(&key) {
                Some(seen) => (seen.clone(), true),
                None => {
                    let ahead = entries.len();
                    let (told, seen) = watch::channel(Progress::Queued { ahead });
                    entries.insert(key.clone(), seen.clone());
                    let (turn, runs) = (self.turn.clone(), self.entries.clone());
                    crate::environment::spawn(async move {
                        let result = {
                            let _turn = turn.lock().await;
                            let _ = told.send(Progress::Running);
                            run().await
                        };
                        runs.lock().expect("the gate's lock").remove(&key);
                        let _ = told.send(Progress::Done(result));
                    });
                    (seen, false)
                }
            }
        };
        let done = tokio::time::timeout(
            patience,
            seen.wait_for(|progress| matches!(progress, Progress::Done(_))),
        )
        .await
        .map(|waited| waited.map(|progress| progress.clone()));
        let progress = match done {
            Ok(Ok(progress)) => progress,
            Ok(Err(_)) => Progress::Done(Err("the gate stopped before it answered".into())),
            Err(_) => seen.borrow().clone(),
        };
        Answer { progress, joined }
    }

    /// The runs that wait or run, as the key of each.
    pub fn pending(&self) -> usize {
        self.entries.lock().expect("the gate's lock").len()
    }
}

/// The gate of this work host.
pub fn gate() -> &'static Gate {
    static GATE: std::sync::LazyLock<Gate> = std::sync::LazyLock::new(Gate::default);
    &GATE
}

/// The tests a command's output says failed, as it says them: cargo's
/// `test name ... FAILED`, Go's `--- FAIL: name`, TAP's `not ok`.
pub fn failed_tests(log: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in log.lines() {
        let line = line.trim();
        let name = if let Some(rest) = line.strip_prefix("test ") {
            rest.strip_suffix(" ... FAILED").map(str::to_string)
        } else if let Some(rest) = line.strip_prefix("--- FAIL: ") {
            Some(rest.split_whitespace().next().unwrap_or(rest).to_string())
        } else {
            line.strip_prefix("not ok ").map(str::to_string)
        };
        if let Some(name) = name
            && !found.contains(&name)
        {
            found.push(name);
        }
    }
    found
}

/// The verdict of a command that ended with `exit`, given `log` and the
/// words of the `known` failures: whether it passes, the failed tests that
/// count, and the ones left out.
pub fn judge(exit: i32, log: &str, known: &[String]) -> (bool, Vec<String>, Vec<String>) {
    if exit == 0 {
        return (true, Vec::new(), Vec::new());
    }
    let (left, counted): (Vec<String>, Vec<String>) =
        failed_tests(log).into_iter().partition(|test| {
            known
                .iter()
                .any(|word| !word.is_empty() && test.contains(word))
        });
    // A failure with no test named is not known: nothing says what failed.
    let passed = counted.is_empty() && !left.is_empty();
    (passed, counted, left)
}

/// The last `lines` lines of `text`.
fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Where the gate keeps its worktrees and the output of its runs.
pub fn folder() -> PathBuf {
    crate::paths::data_dir().join("gate")
}

/// What the gate command is run through: one line that keeps the output and
/// the exit status in files while the terminal shows them.
fn script(command: &str) -> String {
    format!(
        "( ( {command}\n ); echo $? > \"$TSUMUGI_GATE_STATUS\" ) 2>&1 | tee \"$TSUMUGI_GATE_LOG\"\n\
         read gate_status < \"$TSUMUGI_GATE_STATUS\"\n\
         exit \"${{gate_status:-1}}\"\n"
    )
}

/// What a run of the gate needs: where it runs, what it is run with and
/// where it leaves its output.
#[derive(Debug, Clone)]
pub struct Plan {
    pub commit: String,
    /// The worktree the command runs in.
    pub work: PathBuf,
    pub log: PathBuf,
    pub status: PathBuf,
    pub env: Vec<(String, String)>,
    /// The command line, as the shell is given it.
    pub command: Vec<String>,
}

/// Puts the gate's worktree of the repository at `primary` on `commit` and
/// plans the run of `gate`'s command there, keeping what it leaves in `root`
/// and building where `shared` says builds are shared.
pub async fn prepare(
    settings: &Settings,
    gate: &RepositoryGate,
    primary: &Path,
    commit: &str,
    root: &Path,
    shared: &Path,
) -> Result<Plan, String> {
    let shell = settings.shell.program(&crate::programs::login_shell());
    if !crate::worktree_setup::posix(&shell) {
        return Err(format!(
            "the gate runs through a POSIX shell, and {shell} is not one"
        ));
    }
    let work = crate::governor::folder_of(&root.join("worktrees"), primary);
    tsumugi_vcs::worktree::place_gate(primary, &work, commit).await?;
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let short = &commit[..commit.len().min(12)];
    let (log, status) = (
        root.join(format!("{short}.log")),
        root.join(format!("{short}.status")),
    );
    let _ = std::fs::remove_file(&status);
    let _ = std::fs::remove_file(&log);
    let mut env = crate::daemon::terminal_env();
    let limits = crate::governor::Limits::from(&settings.governor);
    env.extend(limits.environment(primary, shared));
    env.push(("TSUMUGI_GATE_LOG".into(), log.display().to_string()));
    env.push(("TSUMUGI_GATE_STATUS".into(), status.display().to_string()));
    env.push(("TSUMUGI_GATE_COMMIT".into(), commit.into()));
    Ok(Plan {
        commit: commit.into(),
        work,
        log,
        status,
        env,
        command: tsumugi_model::shell_line(&shell, &script(&gate.command)),
    })
}

/// How the run of `plan` ended, judged by the output it left and the
/// exit status it wrote, or else `ended`, the status its terminal kept.
pub fn conclude(
    plan: &Plan,
    gate: &RepositoryGate,
    ended: Option<i32>,
    terminal: Option<String>,
) -> Result<Report, String> {
    let exit = std::fs::read_to_string(&plan.status)
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok())
        .or(ended);
    let Some(exit) = exit else {
        return Err("the Gate terminal went before the command ended".into());
    };
    let output = read_end(&plan.log);
    let (passed, failed, known) = judge(exit, &output, &gate.known_failures);
    Ok(Report {
        commit: plan.commit.clone(),
        passed,
        exit: Some(exit),
        failed,
        known,
        terminal,
        tail: tail(&output, TAIL_LINES),
    })
}

/// Runs `gate`'s command on `commit` of the repository at `primary`, in a
/// worktree of the gate's own and a terminal called Gate that keeps the
/// output, and judges how it ended.
pub async fn run(
    daemon: &Daemon,
    settings: &Settings,
    gate: &RepositoryGate,
    primary: &Path,
    commit: &str,
    root: &Path,
    shared: &Path,
) -> Result<Report, String> {
    let plan = prepare(settings, gate, primary, commit, root, shared).await?;
    let request = TerminalStartRequest {
        floating: false,
        name: "Gate".into(),
        cwd: plan.work.clone(),
        program: TerminalProgram::Command,
        env: plan.env.clone(),
        command: plan.command.clone(),
        ..Default::default()
    };
    let terminal = crate::terminals::start(daemon, settings, request)
        .await
        .map_err(|error| format!("cannot start the gate: {error}"))?;
    let ended = crate::worktree_setup::ended(daemon, terminal.resource_id).await;
    conclude(&plan, gate, ended, Some(terminal.resource_id.to_string()))
}

/// The end of the file `path`, which a long run may have made large.
fn read_end(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let size = file.metadata().map_or(0, |meta| meta.len());
    let _ = file.seek(SeekFrom::Start(size.saturating_sub(READ_BACK)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Asks the gate of the repository holding `dir` to check its latest commit,
/// and answers once the run ends or `wait` seconds, 600 when none, have gone.
pub async fn ask(
    daemon: &Daemon,
    settings: Settings,
    dir: &Path,
    wait: Option<u64>,
) -> crate::delegation::Result<serde_json::Value> {
    use crate::delegation::{Code, Error};
    let primary = tsumugi_vcs::worktree::primary(dir)
        .await
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let Some(gate) = settings.gate.of(&primary).cloned() else {
        return Err(Error::new(
            Code::Refused,
            format!(
                "no gate command is set for {}; the user sets one in [[gate.repositories]] of the settings",
                primary.display()
            ),
        ));
    };
    let commit = tsumugi_vcs::worktree::head(dir)
        .await
        .map_err(|error| Error::new(Code::Invalid, error))?;
    let uncommitted = tsumugi_vcs::git::changes(dir)
        .await
        .map(|changes| changes.len())
        .unwrap_or(0);
    let key = Key {
        primary: primary.clone(),
        commit: commit.clone(),
    };
    let patience =
        Duration::from_secs(wait.map_or(PATIENCE.as_secs(), |seconds| seconds.min(3600)));
    let daemon = daemon.clone();
    let answer = self::gate()
        .request(key.clone(), patience, move || async move {
            run(
                &daemon,
                &settings,
                &gate,
                &primary,
                &commit,
                &folder(),
                &crate::governor::shared_root(),
            )
            .await
        })
        .await;
    let mut told = told(&key, &answer);
    if uncommitted > 0 {
        told["uncommitted"] = uncommitted.into();
        told["note"] =
            format!("{uncommitted} uncommitted changes are not in the commit that was checked")
                .into();
    }
    Ok(told)
}

/// What a session that asks is told, as the answer the tool gives.
pub fn told(key: &Key, answer: &Answer) -> serde_json::Value {
    use serde_json::json;
    let state = match &answer.progress {
        Progress::Queued { ahead } => json!({
            "state": "queued",
            "ahead": ahead,
            "note": "the gate runs one check at a time; ask again to wait for this one",
        }),
        Progress::Running => json!({
            "state": "running",
            "note": "the gate is running this commit; ask again to wait for it",
        }),
        Progress::Done(Ok(report)) => json!({"state": "done", "report": report}),
        Progress::Done(Err(error)) => json!({"state": "failed", "error": error}),
    };
    let mut told = state;
    told["commit"] = key.commit.clone().into();
    told["joined"] = answer.joined.into();
    told
}

#[cfg(test)]
mod tests;
