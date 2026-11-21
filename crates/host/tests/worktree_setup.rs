//! Setting up a new worktree: what is given to it, and when its repository's
//! script runs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tsumugi_host::execution::TerminalStartRequest;
use tsumugi_host::settings::{RepositorySetup, Settings};
use tsumugi_host::{terminals, worktree_setup};
use tsumugi_model::{SetupChoice, SetupOutcome, SetupRunPolicy};
use tsumugi_term::Daemon;

async fn daemon() -> (Daemon, tempfile::TempDir) {
    let dir = tempfile::Builder::new()
        .prefix("tsumugi-setup-")
        .tempdir_in(tsumugi_sys::local::temp_dir())
        .unwrap();
    tsumugi_sys::fs::set_mode(dir.path(), 0o700).unwrap();
    let socket = dir.path().join("daemon.sock");
    let serving = socket.clone();
    tokio::spawn(async move {
        Daemon::serve(&serving, Duration::from_millis(20), |_| {})
            .await
            .unwrap()
    });
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            return (daemon, dir);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the test daemon did not start");
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository with `files` committed, `.env` ignored, and a worktree of it.
async fn repository(dir: &Path, files: &[(&str, &str)]) -> (PathBuf, PathBuf) {
    let primary = std::fs::canonicalize(dir).unwrap().join("api");
    std::fs::create_dir(&primary).unwrap();
    git(&primary, &["init", "-q", "-b", "main"]);
    std::fs::write(primary.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(primary.join(".env"), "KEY=1").unwrap();
    for (name, text) in files {
        std::fs::write(primary.join(name), text).unwrap();
    }
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-q", "-m", "first"]);
    let made = tsumugi_vcs::worktree::create(&primary, "review", None)
        .await
        .unwrap();
    (primary, made.path)
}

fn settings(policy: SetupRunPolicy) -> Settings {
    let mut settings = Settings::default();
    settings.shell.command = vec!["/bin/sh".into()];
    settings.worktree_setup.run_policy = policy;
    settings
}

const SETUP: &str = "[scripts]\nsetup = 'echo \"$TSUMUGI_WORKSPACE_NAME\" > ran.txt'\n";

#[tokio::test]
async fn a_repositorys_file_runs_nothing_unless_the_users_policy_says_so() {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(dir.path(), &[("tsumugi.toml", SETUP)]).await;

    for policy in [SetupRunPolicy::Skip, SetupRunPolicy::Ask] {
        let done = worktree_setup::set_up(
            &daemon,
            &settings(policy),
            &primary,
            &worktree,
            SetupChoice::Inherit,
            false,
        )
        .await
        .unwrap();

        assert_eq!(done.outcome, SetupOutcome::Nothing, "{policy:?}");
    }
    assert!(terminals::list(&daemon).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_policy_of_ask_hands_the_script_back_to_be_asked_about_and_runs_nothing() {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(dir.path(), &[("tsumugi.toml", SETUP)]).await;

    let done = worktree_setup::set_up(
        &daemon,
        &settings(SetupRunPolicy::Ask),
        &primary,
        &worktree,
        SetupChoice::Inherit,
        true,
    )
    .await
    .unwrap();

    let SetupOutcome::Asked { script } = done.outcome else {
        panic!("{:?}", done.outcome);
    };
    assert!(script.contains("TSUMUGI_WORKSPACE_NAME"));
    assert!(terminals::list(&daemon).await.unwrap().is_empty());
}

// A command terminal runs through the tsumugi executable, which the test
// binary is not: what a script does and how it ends is read through the tag
// that executable writes, which the tests below write themselves.

#[tokio::test]
async fn a_script_the_policy_runs_is_started_in_a_terminal_and_a_choice_overrides_the_policy() {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(dir.path(), &[("tsumugi.toml", SETUP)]).await;

    for (policy, choice) in [
        (SetupRunPolicy::Run, SetupChoice::Inherit),
        (SetupRunPolicy::Skip, SetupChoice::Run),
        (SetupRunPolicy::Ask, SetupChoice::Run),
    ] {
        let done = worktree_setup::set_up(
            &daemon,
            &settings(policy),
            &primary,
            &worktree,
            choice,
            true,
        )
        .await
        .unwrap();

        assert_eq!(done.outcome, SetupOutcome::Ran, "{policy:?} {choice:?}");
    }
}

#[tokio::test]
async fn a_choice_to_skip_overrides_a_policy_of_run() {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(dir.path(), &[("tsumugi.toml", SETUP)]).await;

    let done = worktree_setup::set_up(
        &daemon,
        &settings(SetupRunPolicy::Run),
        &primary,
        &worktree,
        SetupChoice::Skip,
        true,
    )
    .await
    .unwrap();

    assert_eq!(done.outcome, SetupOutcome::Nothing);
    assert!(terminals::list(&daemon).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_script_s_end_is_read_from_the_status_its_terminal_is_tagged_with() {
    let (daemon, dir) = daemon().await;
    let mut shell = settings(SetupRunPolicy::Run);
    shell.shell.command = vec!["/bin/sh".into(), "-c".into(), "read answer".into()];
    let terminal = terminals::start(
        &daemon,
        &shell,
        TerminalStartRequest {
            name: "Setup".into(),
            cwd: dir.path().into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let waiting = {
        let daemon = daemon.clone();
        tokio::spawn(async move { worktree_setup::ended(&daemon, terminal.resource_id).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!waiting.is_finished());

    daemon
        .tag(
            terminal.id,
            tsumugi_term::tags::tagged("tsumugi.exit", "3"),
            Vec::new(),
        )
        .await
        .unwrap();

    assert_eq!(waiting.await.unwrap(), Some(3));
}

#[tokio::test]
async fn a_terminal_stopped_before_its_script_ended_ends_the_wait_without_a_status() {
    let (daemon, dir) = daemon().await;
    let mut shell = settings(SetupRunPolicy::Run);
    shell.shell.command = vec!["/bin/sh".into(), "-c".into(), "read answer".into()];
    let terminal = terminals::start(
        &daemon,
        &shell,
        TerminalStartRequest {
            name: "Setup".into(),
            cwd: dir.path().into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    terminals::stop(&daemon, terminal.resource_id)
        .await
        .unwrap();

    assert_eq!(
        worktree_setup::ended(&daemon, terminal.resource_id).await,
        None
    );
}

#[tokio::test]
async fn the_policy_of_one_repository_does_not_reach_another() {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(dir.path(), &[("tsumugi.toml", SETUP)]).await;
    let mut settings = settings(SetupRunPolicy::Skip);
    settings.worktree_setup.repositories.push(RepositorySetup {
        path: "/somewhere/else".into(),
        run_policy: SetupRunPolicy::Run,
    });

    worktree_setup::set_up(
        &daemon,
        &settings,
        &primary,
        &worktree,
        SetupChoice::Inherit,
        false,
    )
    .await
    .unwrap();

    assert!(terminals::list(&daemon).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_ignored_files_the_primary_lists_are_copied_and_what_the_worktree_tracks_is_not_replaced()
 {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(
        dir.path(),
        &[(".worktreeinclude", ".env\na.txt\n"), ("a.txt", "tracked")],
    )
    .await;
    std::fs::write(primary.join("a.txt"), "edited in the primary").unwrap();
    std::fs::write(worktree.join("a.txt"), "edited in the worktree").unwrap();

    let done = worktree_setup::set_up(
        &daemon,
        &settings(SetupRunPolicy::Skip),
        &primary,
        &worktree,
        SetupChoice::Inherit,
        false,
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(worktree.join(".env")).unwrap(),
        "KEY=1"
    );
    assert_eq!(
        std::fs::read_to_string(worktree.join("a.txt")).unwrap(),
        "edited in the worktree"
    );
    assert!(
        done.warnings.iter().any(|w| w.contains("a.txt")),
        "{:?}",
        done.warnings
    );
}

#[tokio::test]
async fn a_repository_that_asks_for_nothing_is_set_up_with_nothing_even_where_an_env_file_is_ignored()
 {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(dir.path(), &[]).await;

    for policy in [SetupRunPolicy::Run, SetupRunPolicy::Ask] {
        let done = worktree_setup::set_up(
            &daemon,
            &settings(policy),
            &primary,
            &worktree,
            SetupChoice::Inherit,
            true,
        )
        .await
        .unwrap();

        assert_eq!(done.outcome, SetupOutcome::Nothing, "{policy:?}");
        assert!(done.warnings.is_empty(), "{:?}", done.warnings);
    }
    // The `.env` is ignored and exists, but nothing listed it.
    assert!(!worktree.join(".env").exists());
    assert!(terminals::list(&daemon).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_setup_file_that_is_not_toml_is_told_and_the_worktree_is_still_set_up() {
    let (daemon, dir) = daemon().await;
    let (primary, worktree) = repository(
        dir.path(),
        &[
            ("tsumugi.toml", "scripts = ["),
            (".worktreeinclude", ".env\n"),
        ],
    )
    .await;

    let done = worktree_setup::set_up(
        &daemon,
        &settings(SetupRunPolicy::Run),
        &primary,
        &worktree,
        SetupChoice::Inherit,
        false,
    )
    .await
    .unwrap();

    assert_eq!(done.outcome, SetupOutcome::Nothing);
    assert!(done.warnings.iter().any(|w| w.contains("tsumugi.toml")));
    assert!(worktree.join(".env").is_file());
}
