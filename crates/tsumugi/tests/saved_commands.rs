//! Commands a project keeps run in the worktree chosen, on the work host.
#![cfg(unix)]

mod support {
    pub mod isolated;
}

use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use support::isolated::{Hosts, isolated};
use tsumugi_model::{NewSavedCommand, SavedCommand, SessionId, TerminalProgram};
use tsumugi_protocol::{
    Answer, Ask, Event, HostPath, Request, SPOKEN, TerminalStart, decode, encode,
};

type Link = std::io::BufReader<UnixStream>;

fn link(root: &Path) -> Link {
    let stream = UnixStream::connect(root.join("work.sock")).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut reader = std::io::BufReader::new(stream);
    let mut hello = String::new();
    reader.read_line(&mut hello).unwrap();
    reader
        .get_mut()
        .write_all(
            encode(&Request::Hello {
                versions: SPOKEN.to_vec(),
            })
            .as_bytes(),
        )
        .unwrap();
    loop {
        if matches!(event(&mut reader), Event::Snapshot(_)) {
            return reader;
        }
    }
}

fn event(reader: &mut Link) -> Event {
    let mut line = String::new();
    assert!(reader.read_line(&mut line).unwrap() > 0, "the host hung up");
    decode(&line).unwrap()
}

fn ask(reader: &mut Link, ask: Ask) -> Answer {
    reader
        .get_mut()
        .write_all(
            encode(&Request::Ask {
                asked: 1,
                ask: Box::new(ask),
            })
            .as_bytes(),
        )
        .unwrap();
    loop {
        if let Event::Answer { asked: 1, answer } = event(reader) {
            return answer;
        }
    }
}

fn kept(reader: &mut Link) -> Vec<SavedCommand> {
    reader
        .get_mut()
        .write_all(encode(&Request::Snapshot).as_bytes())
        .unwrap();
    loop {
        if let Event::Snapshot(work) = event(reader) {
            return work.tree.saved_commands;
        }
    }
}

/// What the host keeps once `ready` says so of it.
fn kept_by(reader: &mut Link, ready: impl Fn(&[SavedCommand]) -> bool) -> Vec<SavedCommand> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let commands = kept(reader);
        if ready(&commands) {
            return commands;
        }
        assert!(Instant::now() < deadline, "the host kept nothing");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A repository and a second worktree of it, in directories whose names
/// hold a space and non-ASCII characters.
fn repository(root: &Path) -> (PathBuf, PathBuf) {
    let main = root.join("リポジトリ a");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    git(&main, &["commit", "-q", "--allow-empty", "-m", "first"]);
    let other = root.join("作業 b");
    git(
        &main,
        &["worktree", "add", "-q", other.to_str().unwrap(), "-b", "b"],
    );
    (
        std::fs::canonicalize(main).unwrap(),
        std::fs::canonicalize(other).unwrap(),
    )
}

/// Start `script` as a command terminal in `dir`, let it go, and say which
/// terminal it is.
fn run_in(link: &mut Link, dir: &Path, script: &str) -> String {
    let start = TerminalStart {
        name: "run".into(),
        cwd: HostPath::local(dir),
        program: TerminalProgram::Command,
        script: Some(script.into()),
        cols: 100,
        rows: 30,
        ..TerminalStart::default()
    };
    let Answer::Terminal { terminal } = ask(link, Ask::StartTerminal(Box::new(start))) else {
        panic!("the terminal did not start");
    };
    link.get_mut()
        .write_all(encode(&Request::Release { id: terminal.id }).as_bytes())
        .unwrap();
    terminal.resource_id.to_string()
}

/// The screen of the terminal once its command has ended.
fn ended(hosts: &Hosts, resource: &str) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let screen = hosts.run(&["terminal", "screen", "--terminal", resource, "--json"]);
        if !screen["exit"].is_null() {
            return screen;
        }
        assert!(
            Instant::now() < deadline,
            "the command did not end: {screen}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn saved(dir: &Path, label: &str, command: &str) -> NewSavedCommand {
    NewSavedCommand {
        id: SessionId::new().to_string(),
        dir: dir.into(),
        label: label.into(),
        command: command.into(),
    }
}

#[test]
fn a_command_saved_in_one_worktree_is_kept_for_the_project_and_runs_where_it_is_chosen() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let (main, other) = repository(root);
    hosts.run(&["terminal", "list", "--json"]);
    let mut link = link(root);
    let text = "pwd\nprintf 'saw:%s\\n' \"$0\" >/dev/null\nexit 3";
    let new = saved(&main, "Where am I", text);
    assert_eq!(ask(&mut link, Ask::SaveCommand(new.clone())), Answer::Done);

    let commands = kept(&mut link);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].project, main);
    assert_eq!(
        (&*commands[0].label, &*commands[0].command),
        ("Where am I", text)
    );

    // Chosen in the other worktree, it runs there, not where it was saved.
    let resource = run_in(&mut link, &other, &commands[0].command);
    let screen = ended(&hosts, &resource);
    assert_eq!(screen["exit"], 3, "{screen}");
    let shown = screen["lines"].to_string();
    assert!(shown.contains(other.to_str().unwrap()), "{shown}");
    assert!(!shown.contains(main.to_str().unwrap()), "{shown}");
}

#[test]
fn a_saved_command_survives_a_restart_changes_in_place_and_goes_when_deleted() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let (main, other) = repository(root);
    hosts.run(&["terminal", "list", "--json"]);
    let mut first = link(root);
    let new = saved(&main, "Checks", "just ci");
    assert_eq!(ask(&mut first, Ask::SaveCommand(new.clone())), Answer::Done);
    let mut edited = new.clone();
    edited.dir = other;
    edited.label = "Checks again".into();
    edited.command = "just ci\njust test".into();
    assert_eq!(ask(&mut first, Ask::SaveCommand(edited)), Answer::Done);
    let commands = kept(&mut first);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].label, "Checks again");
    assert_eq!(commands[0].project, main);
    drop(first);

    let stopped = isolated(root).args(["host", "stop"]).output().unwrap();
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    hosts.run(&["terminal", "list", "--json"]);
    let mut again = link(root);
    // The host tells what it keeps once it has read its sessions, which a
    // snapshot taken as it starts may come before.
    assert_eq!(kept_by(&mut again, |kept| !kept.is_empty()), commands);

    let id = commands[0].id.clone();
    assert_eq!(
        ask(&mut again, Ask::DeleteSavedCommand { id: id.clone() }),
        Answer::Done
    );
    assert!(kept(&mut again).is_empty());
    assert!(matches!(
        ask(&mut again, Ask::DeleteSavedCommand { id }),
        Answer::Failed { .. }
    ));
}

#[test]
fn a_command_that_holds_together_no_better_is_refused_and_nothing_runs() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let (main, _) = repository(root);
    hosts.run(&["terminal", "list", "--json"]);
    let mut link = link(root);
    for bad in [saved(&main, "", "x"), saved(&main, "x", "  ")] {
        assert!(matches!(
            ask(&mut link, Ask::SaveCommand(bad)),
            Answer::Failed { .. }
        ));
    }
    let missing = saved(&root.join("nowhere"), "x", "x");
    assert!(matches!(
        ask(&mut link, Ask::SaveCommand(missing)),
        Answer::Failed { .. }
    ));
    assert!(kept(&mut link).is_empty());
}
