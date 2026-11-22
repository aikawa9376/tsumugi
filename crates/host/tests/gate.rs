//! Running the gate: where the command runs, what it is judged by, and what
//! it leaves to read. The terminal that shows the run is Tsumugi's keeper of
//! command terminals, which these tests do not start; they run the line it
//! would run, by hand, in the place and with the environment planned for it.

use std::path::{Path, PathBuf};
use std::process::Command;

use tsumugi_core::settings::RepositoryGate;
use tsumugi_host::gate::{self, Plan};
use tsumugi_host::settings::Settings;

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn repository(dir: &Path) -> (PathBuf, String) {
    let primary = std::fs::canonicalize(dir).unwrap().join("api");
    std::fs::create_dir(&primary).unwrap();
    git(&primary, &["init", "-q", "-b", "main"]);
    std::fs::write(primary.join("a.txt"), "committed").unwrap();
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-q", "-m", "first"]);
    let commit = git(&primary, &["rev-parse", "HEAD"]);
    (primary, commit)
}

fn settings() -> Settings {
    let mut settings = Settings::default();
    settings.shell.command = vec!["/bin/sh".into()];
    settings
}

fn gate_of(command: &str, known: &[&str]) -> RepositoryGate {
    RepositoryGate {
        path: String::new(),
        command: command.into(),
        known_failures: known.iter().map(ToString::to_string).collect(),
    }
}

/// Runs the line of `plan` as its terminal would, and returns its status.
fn run_by_hand(plan: &Plan) -> i32 {
    let (program, args) = plan.command.split_first().unwrap();
    Command::new(program)
        .args(args)
        .current_dir(&plan.work)
        .envs(plan.env.iter().cloned())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .code()
        .unwrap()
}

async fn checked(
    command: &str,
    known: &[&str],
    root: &Path,
    primary: &Path,
    commit: &str,
) -> (Plan, Result<gate::Report, String>) {
    let gate = gate_of(command, known);
    let plan = gate::prepare(
        &settings(),
        &gate,
        primary,
        commit,
        root,
        &root.join("build"),
    )
    .await
    .unwrap();
    let status = run_by_hand(&plan);
    let report = gate::conclude(&plan, &gate, Some(status), Some("t".into()));
    (plan, report)
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs a POSIX shell")]
async fn the_command_runs_on_the_commit_asked_in_a_worktree_of_the_gates_own() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, commit) = repository(dir.path());
    // What the asker has changed and not committed is not what is checked.
    std::fs::write(primary.join("a.txt"), "edited").unwrap();
    let root = dir.path().join("gate");

    let (plan, report) = checked(
        "cat a.txt; pwd; echo \"$TSUMUGI_GATE_COMMIT\"",
        &[],
        &root,
        &primary,
        &commit,
    )
    .await;
    let report = report.unwrap();

    assert!(report.passed, "{report:?}");
    assert_eq!(report.exit, Some(0));
    assert!(report.tail.contains("committed"), "{}", report.tail);
    assert!(!report.tail.contains("edited"), "{}", report.tail);
    assert!(report.tail.contains(&commit), "{}", report.tail);
    assert!(plan.work.starts_with(&root));
    assert!(report.tail.contains(plan.work.to_str().unwrap()));
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs a POSIX shell")]
async fn a_failure_in_a_known_test_alone_passes_and_any_other_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, commit) = repository(dir.path());
    let root = dir.path().join("gate");
    let command =
        "echo 'test net::dns_lookup ... FAILED'; echo 'test core::real_bug ... FAILED'; exit 101";

    let (_, both) = checked(command, &["dns_lookup"], &root, &primary, &commit).await;
    let both = both.unwrap();
    assert!(!both.passed);
    assert_eq!(both.exit, Some(101));
    assert_eq!(both.failed, ["core::real_bug"]);
    assert_eq!(both.known, ["net::dns_lookup"]);

    let (_, only) = checked(
        command,
        &["dns_lookup", "real_bug"],
        &root,
        &primary,
        &commit,
    )
    .await;
    let only = only.unwrap();
    assert!(only.passed);
    assert!(only.failed.is_empty());
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs a POSIX shell")]
async fn a_command_that_fails_reports_its_status_even_when_the_terminal_kept_none() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, commit) = repository(dir.path());
    let gate = gate_of("echo building; exit 3", &[]);
    let plan = gate::prepare(
        &settings(),
        &gate,
        &primary,
        &commit,
        &dir.path().join("g"),
        &dir.path().join("b"),
    )
    .await
    .unwrap();
    run_by_hand(&plan);

    let report = gate::conclude(&plan, &gate, None, None).unwrap();

    assert!(!report.passed);
    assert_eq!(report.exit, Some(3));
    assert!(report.tail.contains("building"));
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs a POSIX shell")]
async fn a_run_that_left_no_status_and_whose_terminal_went_is_an_error_not_a_pass() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, commit) = repository(dir.path());
    let gate = gate_of("true", &[]);
    let plan = gate::prepare(
        &settings(),
        &gate,
        &primary,
        &commit,
        &dir.path().join("g"),
        &dir.path().join("b"),
    )
    .await
    .unwrap();

    assert!(gate::conclude(&plan, &gate, None, None).is_err());
}

#[tokio::test]
async fn a_shell_that_is_not_posix_is_refused_before_anything_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let (primary, commit) = repository(dir.path());
    let mut settings = settings();
    settings.shell.command = vec!["pwsh".into()];
    let root = dir.path().join("gate");

    let refused = gate::prepare(
        &settings,
        &gate_of("x", &[]),
        &primary,
        &commit,
        &root,
        &root,
    )
    .await
    .unwrap_err();

    assert!(refused.contains("POSIX"), "{refused}");
    assert!(!root.exists());
}
