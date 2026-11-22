//! Setting up a new worktree as its repository asks: the gitignored files and
//! directories of the primary checkout it is given, the setup script it runs
//! in a terminal anyone can read, and the terminals it opens.
//!
//! A repository asks in `tsumugi.toml` and `.worktreeinclude` at its root. A
//! file there runs nothing by itself: whether the script runs is the user's
//! [`SetupRunPolicy`] for the repository, kept in the user's settings.

use std::path::Path;
use std::time::Duration;

use tsumugi_model::{
    SetupChoice, SetupOutcome, SetupRunPolicy, TerminalProgram, TerminalResourceId, WorktreeSetup,
    shell_line,
};
use tsumugi_term::Daemon;

use crate::execution::TerminalStartRequest;
use crate::settings::Settings;

/// The file at a repository's root that says how its worktrees are set up.
pub const FILE: &str = "tsumugi.toml";
/// The file at a repository's root that lists the ignored files to copy.
pub const INCLUDE: &str = ".worktreeinclude";
/// What the file may weigh, and what each of its strings.
const FILE_LIMIT: u64 = 256 * 1024;
const FIELD_LIMIT: usize = 64 * 1024;
const TABS: usize = 256;
/// How often a wait for the setup script looks at its terminal.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// What `tsumugi.toml` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Project {
    /// The script run in a new worktree.
    pub setup: Option<String>,
    /// Whether the agent starts only once the setup script ended well.
    pub wait_for_setup: bool,
    /// The terminals a new worktree opens. The first is the agent's own.
    pub tabs: Vec<Tab>,
    /// The ignored directories of the primary checkout linked into a new
    /// worktree.
    pub shared: Vec<String>,
}

/// A terminal a new worktree opens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tab {
    pub title: Option<String>,
    pub command: Option<String>,
    pub color: Option<String>,
}

/// The file's text read: a field that does not hold is left out and the rest
/// stands, but text that is no TOML, or too large, is no configuration at
/// all.
pub fn parse(text: &str) -> Result<Project, String> {
    if text.len() as u64 > FILE_LIMIT {
        return Err(format!("{FILE} is larger than {} KiB", FILE_LIMIT / 1024));
    }
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let string = |value: Option<&toml::Value>| -> Option<String> {
        let text = value?.as_str()?.trim();
        (!text.is_empty() && text.len() <= FIELD_LIMIT).then(|| text.to_string())
    };
    let scripts = table.get("scripts").and_then(toml::Value::as_table);
    let tabs = table
        .get("default_tabs")
        .and_then(toml::Value::as_array)
        .filter(|tabs| tabs.len() <= TABS)
        .map(|tabs| {
            tabs.iter()
                .filter_map(toml::Value::as_table)
                .map(|tab| Tab {
                    title: string(tab.get("title")),
                    command: string(tab.get("command")),
                    color: string(tab.get("color")).filter(|c| colour(c)),
                })
                .filter(|tab| tab.title.is_some() || tab.command.is_some() || tab.color.is_some())
                .collect()
        })
        .unwrap_or_default();
    let shared = table
        .get("worktree")
        .and_then(toml::Value::as_table)
        .and_then(|worktree| worktree.get("shared_directories"))
        .and_then(toml::Value::as_array)
        .map(|paths| {
            let mut kept: Vec<String> = Vec::new();
            for path in paths.iter().take(100) {
                let Some(path) = path.as_str().and_then(tsumugi_vcs::worktree::relative) else {
                    continue;
                };
                if !kept.contains(&path) {
                    kept.push(path);
                }
            }
            kept
        })
        .unwrap_or_default();
    Ok(Project {
        setup: string(scripts.and_then(|s| s.get("setup"))),
        wait_for_setup: table
            .get("setup_agent_startup_policy")
            .and_then(toml::Value::as_str)
            == Some("wait_for_setup"),
        tabs,
        shared,
    })
}

/// `#RGB` or `#RRGGBB`.
fn colour(text: &str) -> bool {
    text.strip_prefix('#')
        .is_some_and(|hex| matches!(hex.len(), 3 | 6) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// What the directory `dir` holds in the file `name`, when it is a plain
/// file of reasonable size: a link is not followed, so a repository cannot
/// have a file of its own place made to be read.
fn read_plain(dir: &Path, name: &str) -> Result<Option<String>, String> {
    let path = dir.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {name}: {error}")),
    };
    if !meta.is_file() {
        return Err(format!("{name} is not a regular file"));
    }
    if meta.len() > FILE_LIMIT {
        return Err(format!("{name} is larger than {} KiB", FILE_LIMIT / 1024));
    }
    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|error| format!("cannot read {name}: {error}"))
}

/// What `dir`'s `tsumugi.toml` says; none of it when there is no file.
pub fn read(dir: &Path) -> Result<Project, String> {
    match read_plain(dir, FILE)? {
        Some(text) => parse(&text).map_err(|error| format!("{FILE}: {error}")),
        None => Ok(Project::default()),
    }
}

/// What a setup script is told, beside the environment of every terminal.
pub fn environment(primary: &Path, worktree: &Path) -> Vec<(String, String)> {
    vec![
        (
            "TSUMUGI_ROOT_PATH".into(),
            primary.to_string_lossy().into_owned(),
        ),
        (
            "TSUMUGI_WORKTREE_PATH".into(),
            worktree.to_string_lossy().into_owned(),
        ),
        (
            "TSUMUGI_WORKSPACE_NAME".into(),
            worktree
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
    ]
}

/// Whether the shell is one that reads `set -e`.
pub(crate) fn posix(shell: &str) -> bool {
    let name = shell.rsplit(['/', '\\']).next().unwrap_or_default();
    matches!(name, "sh" | "bash" | "zsh" | "dash" | "ksh")
}

/// Everything the repository would run in the new worktree, as the person is
/// shown it before it does: the script, then each terminal's command.
fn preview(project: &Project) -> String {
    let mut parts: Vec<String> = project.setup.iter().cloned().collect();
    parts.extend(
        project
            .tabs
            .iter()
            .skip(1)
            .filter_map(|tab| tab.command.clone()),
    );
    parts.join("\n\n")
}

/// Sets up `worktree`, made in the repository holding `repository`.
///
/// The ignored files the primary checkout lists are given to it first. Then
/// what the repository would run, its setup script and its terminals'
/// commands, runs when `choice` says so, or, as `Inherit`, when the user's
/// policy for the repository says so. Policy `ask` answers
/// [`SetupOutcome::Asked`] to be asked again with a choice, or, where no
/// person can be asked, runs nothing. A script that fails is an error; the
/// worktree and what ran in it stay.
pub async fn set_up(
    daemon: &Daemon,
    settings: &Settings,
    repository: &Path,
    worktree: &Path,
    choice: SetupChoice,
    can_ask: bool,
) -> Result<WorktreeSetup, String> {
    let primary = tsumugi_vcs::worktree::primary(repository).await?;
    let mut warnings = give(&primary, worktree).await;
    let project = match read(worktree) {
        Ok(project) => project,
        Err(error) => {
            warnings.push(error);
            Project::default()
        }
    };
    let nothing = |warnings| WorktreeSetup {
        warnings,
        outcome: SetupOutcome::Nothing,
    };
    // The first terminal is the agent's, which the caller starts.
    let commands =
        project.setup.is_some() || project.tabs.iter().skip(1).any(|tab| tab.command.is_some());
    let decided = match choice {
        SetupChoice::Run => SetupRunPolicy::Run,
        SetupChoice::Skip => SetupRunPolicy::Skip,
        SetupChoice::Inherit => settings.worktree_setup.policy_for(&primary),
    };
    let runs = match decided {
        SetupRunPolicy::Run => true,
        SetupRunPolicy::Skip => false,
        SetupRunPolicy::Ask if !commands => false,
        SetupRunPolicy::Ask if can_ask => {
            return Ok(WorktreeSetup {
                warnings,
                outcome: SetupOutcome::Asked {
                    script: preview(&project),
                },
            });
        }
        SetupRunPolicy::Ask => {
            warnings.push(format!(
                "{FILE} has a setup script that was not run: nothing runs until you say so"
            ));
            false
        }
    };
    open_tabs(daemon, settings, &primary, worktree, &project, runs).await?;
    let ran = match (&project.setup, runs) {
        (Some(script), true) => {
            run(
                daemon,
                settings,
                &primary,
                worktree,
                script,
                project.wait_for_setup,
            )
            .await?;
            true
        }
        _ => runs && commands,
    };
    Ok(if ran {
        WorktreeSetup {
            warnings,
            outcome: SetupOutcome::Ran,
        }
    } else {
        nothing(warnings)
    })
}

/// What the primary checkout lists is given to the worktree.
async fn give(primary: &Path, worktree: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let shared = match read(primary) {
        Ok(project) => project.shared,
        Err(error) => {
            warnings.push(error);
            Vec::new()
        }
    };
    let copied = match read_plain(primary, INCLUDE) {
        Ok(text) => {
            let (paths, notes) = tsumugi_vcs::worktree::included(&text.unwrap_or_default());
            warnings.extend(notes.into_iter().map(|note| format!("{INCLUDE}: {note}")));
            paths
        }
        Err(error) => {
            warnings.push(error);
            Vec::new()
        }
    };
    warnings.extend(tsumugi_vcs::worktree::place(primary, worktree, &shared, &copied).await);
    warnings
}

/// The terminals after the first, which are the agent's: each in the
/// worktree, titled as it says, running its command only when `commands`.
async fn open_tabs(
    daemon: &Daemon,
    settings: &Settings,
    primary: &Path,
    worktree: &Path,
    project: &Project,
    commands: bool,
) -> Result<(), String> {
    for tab in project.tabs.iter().skip(1) {
        let command = tab.command.as_deref().filter(|_| commands);
        if command.is_none() && tab.title.is_none() {
            continue;
        }
        let shell = settings.shell.program(&crate::programs::login_shell());
        let mut env = crate::daemon::terminal_env();
        env.extend(environment(primary, worktree));
        crate::terminals::start(
            daemon,
            settings,
            TerminalStartRequest {
                floating: false,
                name: tab.title.clone().unwrap_or_else(|| "Terminal".into()),
                cwd: worktree.to_path_buf(),
                program: if command.is_some() {
                    TerminalProgram::Command
                } else {
                    TerminalProgram::Shell
                },
                env,
                command: command
                    .map(|command| shell_line(&shell, command))
                    .unwrap_or_default(),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}

/// The request that runs `script` in a terminal called Setup in `worktree`,
/// told where the repository and the worktree are.
fn setup_request(
    settings: &Settings,
    primary: &Path,
    worktree: &Path,
    script: &str,
) -> TerminalStartRequest {
    let shell = settings.shell.program(&crate::programs::login_shell());
    // The script stops at its first failing command, as a runner of its own
    // would; a shell that does not read `set -e` runs it as written.
    let script = if posix(&shell) {
        format!("set -e\n{script}")
    } else {
        script.to_string()
    };
    let mut env = crate::daemon::terminal_env();
    env.extend(environment(primary, worktree));
    TerminalStartRequest {
        floating: false,
        name: "Setup".into(),
        cwd: worktree.to_path_buf(),
        program: TerminalProgram::Command,
        env,
        command: shell_line(&shell, &script),
        ..Default::default()
    }
}

/// Runs `script` in a terminal called Setup in `worktree`, and when `wait`,
/// until it ends, which is an error unless it ended well. The terminal stays,
/// with its output, until it is stopped.
async fn run(
    daemon: &Daemon,
    settings: &Settings,
    primary: &Path,
    worktree: &Path,
    script: &str,
    wait: bool,
) -> Result<(), String> {
    let request = setup_request(settings, primary, worktree, script);
    let terminal = crate::terminals::start(daemon, settings, request)
        .await
        .map_err(|error| format!("cannot start the setup script: {error}"))?;
    if !wait {
        return Ok(());
    }
    finished(ended(daemon, terminal.resource_id).await, worktree)
}

/// How a setup script's end, by its status, leaves the worktree it ran in:
/// the worktree stays whatever the status, and a failure says where.
fn finished(status: Option<i32>, worktree: &Path) -> Result<(), String> {
    match status {
        Some(0) => Ok(()),
        Some(status) => Err(format!(
            "the setup script ended with status {status}; its output is in the Setup terminal, and the worktree is kept at {}",
            worktree.display()
        )),
        None => Err(format!(
            "the Setup terminal went before the setup script ended; the worktree is kept at {}",
            worktree.display()
        )),
    }
}

/// The status the command of terminal `resource` ended with, once it has;
/// none when the terminal is gone first.
pub async fn ended(daemon: &Daemon, resource: TerminalResourceId) -> Option<i32> {
    loop {
        let terminal = crate::terminals::list(daemon)
            .await
            .ok()?
            .into_iter()
            .find(|t| t.resource_id == resource)?;
        if let Some(status) = terminal.exit {
            return Some(status);
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_with_nothing_in_it_asks_for_nothing() {
        assert_eq!(parse("").unwrap(), Project::default());
    }

    #[test]
    fn the_scripts_the_wait_the_tabs_and_the_shared_directories_are_read() {
        let project = parse(
            r##"
setup_agent_startup_policy = "wait_for_setup"
[scripts]
setup = "  pnpm install\n"
[[default_tabs]]
title = "Agent"
[[default_tabs]]
title = "Git status"
command = "git status --short"
color = "#a1b2c3"
[worktree]
shared_directories = [".cache", "./node_modules/", ".cache"]
"##,
        )
        .unwrap();

        assert_eq!(project.setup.as_deref(), Some("pnpm install"));
        assert!(project.wait_for_setup);
        assert_eq!(project.tabs.len(), 2);
        assert_eq!(
            project.tabs[1].command.as_deref(),
            Some("git status --short")
        );
        assert_eq!(project.tabs[1].color.as_deref(), Some("#a1b2c3"));
        assert_eq!(project.shared, [".cache", "node_modules"]);
    }

    #[test]
    fn a_field_that_does_not_hold_is_left_out_and_its_siblings_stand() {
        let project = parse(
            r#"
setup_agent_startup_policy = "now"
[scripts]
setup = 3
[[default_tabs]]
color = "red"
[worktree]
shared_directories = ["../out", "/abs", ".git/hooks", "dir", 7]
"#,
        )
        .unwrap();

        assert_eq!(project.setup, None);
        assert!(!project.wait_for_setup);
        assert!(project.tabs.is_empty());
        assert_eq!(project.shared, ["dir"]);
    }

    #[test]
    fn text_that_is_no_toml_or_too_large_configures_nothing() {
        assert!(parse("scripts = [").is_err());
        assert!(parse("a = 1\na = 2").is_err());
        assert!(parse(&format!("# {}", "x".repeat(300 * 1024))).is_err());
    }

    #[test]
    fn the_setup_script_runs_in_the_worktree_stopping_at_its_first_failure_with_the_paths_in_its_environment()
     {
        let mut settings = Settings::default();
        settings.shell.command = vec!["/bin/bash".into()];

        let request = setup_request(
            &settings,
            "/r/api".as_ref(),
            "/r/api-wt/fix".as_ref(),
            "make",
        );

        assert_eq!(request.name, "Setup");
        assert_eq!(request.cwd, Path::new("/r/api-wt/fix"));
        assert_eq!(request.command, ["/bin/bash", "-c", "set -e\nmake"]);
        assert!(
            request
                .env
                .contains(&("TSUMUGI_WORKTREE_PATH".into(), "/r/api-wt/fix".into()))
        );
    }

    #[test]
    fn a_script_that_does_not_end_well_is_an_error_that_says_the_worktree_is_kept() {
        let worktree = Path::new("/r/api-wt/fix");
        assert_eq!(finished(Some(0), worktree), Ok(()));
        for status in [Some(3), None] {
            let error = finished(status, worktree).unwrap_err();
            assert!(error.contains("kept at /r/api-wt/fix"), "{error}");
        }
    }

    #[test]
    fn a_script_is_told_where_the_repository_and_the_worktree_are() {
        let env = environment("/r/api".as_ref(), "/r/api-wt/fix".as_ref());
        assert!(env.contains(&("TSUMUGI_ROOT_PATH".into(), "/r/api".into())));
        assert!(env.contains(&("TSUMUGI_WORKTREE_PATH".into(), "/r/api-wt/fix".into())));
        assert!(env.contains(&("TSUMUGI_WORKSPACE_NAME".into(), "fix".into())));
    }

    #[test]
    fn what_the_person_is_asked_about_is_the_script_and_the_commands_of_the_terminals_after_the_agents()
     {
        let project = parse(
            "[scripts]\nsetup = 'make'\n[[default_tabs]]\ncommand = 'agent-cmd'\n[[default_tabs]]\ncommand = 'git status'\n",
        )
        .unwrap();
        assert_eq!(preview(&project), "make\n\ngit status");
    }
}
