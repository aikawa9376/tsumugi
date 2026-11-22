//! Issues run on the work host, even without an open window.
#![cfg(unix)]

mod support {
    pub mod isolated;
}

use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use support::isolated::{Hosts, isolated};
use tsumugi_model::{LocalIssue, LocalIssueState, NewLocalIssue, SessionId};
use tsumugi_protocol::{Answer, Ask, Event, Request, SPOKEN, decode, encode};

fn link(root: &Path) -> std::io::BufReader<UnixStream> {
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

fn event(reader: &mut std::io::BufReader<UnixStream>) -> Event {
    let mut line = String::new();
    assert!(
        reader.read_line(&mut line).unwrap() > 0,
        "the host disconnected"
    );
    decode(&line).unwrap()
}

fn ask(reader: &mut std::io::BufReader<UnixStream>, ask: Ask) -> Answer {
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

fn waiting_for(
    reader: &mut std::io::BufReader<UnixStream>,
    ready: impl Fn(&[LocalIssue]) -> bool,
) -> Vec<LocalIssue> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        reader
            .get_mut()
            .write_all(encode(&Request::Snapshot).as_bytes())
            .unwrap();
        let issues = loop {
            if let Event::Snapshot(work) = event(reader) {
                break work.tree.local_issues;
            }
        };
        if ready(&issues) {
            return issues;
        }
        assert!(
            Instant::now() < deadline,
            "issues did not progress: {issues:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn saved_issues_wait_for_start_then_run_in_order_and_survive_a_work_host_restart() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let agent = root.join("agent.sh");
    let runs = root.join("runs");
    let release = root.join("release");
    std::fs::write(
        &agent,
        format!(
            "case \" $* \" in *' --plugin-dir '*) ;; *) printf '[]\\n'; exit 0;; esac\n\
         printf '%s ' \"$@\" | tr '\\n' ' ' >> '{}'\necho >> '{}'\n\
         printf '\\033]7501;state=working\\033\\\\'\n\
         while [ ! -f '{}' ]; do sleep 0.05; done\n\
         printf '\\033]7501;state=done\\033\\\\'\nexec sleep 600\n",
            runs.display(),
            runs.display(),
            release.display(),
        ),
    )
    .unwrap();
    let config = root.join("config/tsumugi");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("settings.toml"), format!(
        "profile = \"fake\"\n[[profiles]]\nid = \"fake\"\nagent = \"claude\"\n[profiles.launch]\ncommand = [\"/bin/sh\", \"{}\"]\nvia_shell = false\n",
        agent.display(),
    )).unwrap();
    hosts.run(&["terminal", "list", "--json"]);
    let mut connection = link(root);
    let mut ids = Vec::new();
    for title in ["first request", "second request"] {
        let issue = NewLocalIssue {
            id: SessionId::new().to_string(),
            dir: root.into(),
            title: title.into(),
            body: "Do the requested work.".into(),
        };
        ids.push(issue.id.clone());
        assert_eq!(
            ask(&mut connection, Ask::CreateLocalIssue(issue)),
            Answer::Done
        );
    }
    let saved = waiting_for(&mut connection, |issues| issues.len() == 2);
    assert!(
        saved
            .iter()
            .all(|issue| issue.state == LocalIssueState::Open && issue.session.is_none())
    );
    assert!(!runs.exists());
    for id in ids {
        assert_eq!(
            ask(&mut connection, Ask::StartLocalIssue { id }),
            Answer::Done
        );
    }
    let first = waiting_for(&mut connection, |issues| {
        issues.len() == 2 && issues[0].state == LocalIssueState::Running
    });
    assert_eq!(first[1].state, LocalIssueState::Queued);
    let session = first[0].session.clone();
    assert!(session.is_some());
    drop(connection);
    let restarted = isolated(root).args(["host", "restart"]).output().unwrap();
    assert!(
        restarted.status.success(),
        "{}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    let mut connection = link(root);
    let resumed = waiting_for(&mut connection, |issues| {
        issues.len() == 2 && issues[0].state == LocalIssueState::Running
    });
    assert_eq!(resumed[0].session, session);
    assert_eq!(resumed[1].state, LocalIssueState::Queued);
    assert_eq!(std::fs::read_to_string(&runs).unwrap().lines().count(), 1);
    // No window connection is needed for either issue to finish and the
    // next one to start.
    drop(connection);
    std::fs::write(&release, "continue").unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let mut connection = link(root);
    let done = waiting_for(&mut connection, |issues| {
        issues.len() == 2 && issues.iter().all(|i| i.state == LocalIssueState::Done)
    });
    assert_ne!(done[0].session, done[1].session);
    let launches = std::fs::read_to_string(&runs).unwrap();
    let launches: Vec<_> = launches.lines().collect();
    assert_eq!(launches.len(), 2);
    assert!(launches[0].contains("first request"));
    assert!(launches[1].contains("second request"));

    let id = done[0].id.clone();
    assert_eq!(
        ask(&mut connection, Ask::ReopenLocalIssue { id: id.clone() }),
        Answer::Done
    );
    let reopened = waiting_for(&mut connection, |issues| {
        issues[0].state == LocalIssueState::Open
    });
    assert!(reopened[0].session.is_none());
    assert_eq!(std::fs::read_to_string(&runs).unwrap().lines().count(), 2);
    assert_eq!(
        ask(&mut connection, Ask::CloseLocalIssue { id: id.clone() }),
        Answer::Done
    );
    waiting_for(&mut connection, |issues| {
        issues[0].state == LocalIssueState::Closed
    });
    assert_eq!(
        ask(&mut connection, Ask::DeleteLocalIssue { id }),
        Answer::Done
    );
    waiting_for(&mut connection, |issues| {
        issues.len() == 1 && issues[0].id == done[1].id
    });
    assert_eq!(
        ask(
            &mut connection,
            Ask::DeleteLocalIssue {
                id: done[1].id.clone()
            }
        ),
        Answer::Done
    );
    waiting_for(&mut connection, |issues| issues.is_empty());
    drop(connection);
    let restarted = isolated(root).args(["host", "restart"]).output().unwrap();
    assert!(restarted.status.success());
    let mut connection = link(root);
    waiting_for(&mut connection, |issues| issues.is_empty());
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        connection
            .get_mut()
            .write_all(encode(&Request::Snapshot).as_bytes())
            .unwrap();
        let work = loop {
            if let Event::Snapshot(work) = event(&mut connection) {
                break work;
            }
        };
        if work.tree.listed.as_ref().is_some_and(|listed| {
            done.iter().all(|issue| {
                listed
                    .sessions
                    .iter()
                    .any(|session| Some(&session.id) == issue.session.as_ref())
            })
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "deleting issues must leave their sessions intact"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn the_issue_command_changes_the_queue_the_running_work_host_owns() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    hosts.run(&["terminal", "list", "--json"]);
    let mut window = link(root);
    let dir = root.to_str().unwrap();
    let created = hosts.run(&[
        "issue",
        "create",
        "--title",
        "Check the docs",
        "--body",
        "Read them.",
        "--dir",
        dir,
        "--json",
    ]);
    assert_eq!(created["ok"], true);
    assert_eq!(created["result"]["issue"]["number"], 1);
    assert_eq!(created["result"]["issue"]["state"], "open");
    // The window learns of it without being asked.
    waiting_for(&mut window, |issues| {
        issues.len() == 1 && issues[0].title == "Check the docs"
    });

    let closed = hosts.run(&["issue", "close", "1", "--json"]);
    assert_eq!(closed["result"]["issue"]["state"], "closed");
    waiting_for(&mut window, |issues| {
        issues[0].state == LocalIssueState::Closed
    });
    let listed = hosts.run(&["issue", "list", "--state", "closed", "--dir", dir, "--json"]);
    assert_eq!(listed["result"]["issues"].as_array().unwrap().len(), 1);
    let none = hosts.run(&["issue", "list", "--state", "open", "--json"]);
    assert!(none["result"]["issues"].as_array().unwrap().is_empty());

    let id = created["result"]["issue"]["id"].as_str().unwrap();
    let reopened = hosts.run(&["issue", "reopen", &id[..8], "--json"]);
    assert_eq!(reopened["result"]["issue"]["state"], "open");
    let shown = hosts.run(&["issue", "show", "#1", "--json"]);
    assert_eq!(shown["result"]["issue"]["body"], "Read them.");

    let missing = isolated(root)
        .args(["issue", "show", "9", "--json"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    let said: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(said["ok"], false);
    assert_eq!(said["error"]["code"], "not_found");
    let plain = isolated(root)
        .args(["issue", "delete", "9"])
        .output()
        .unwrap();
    assert_eq!(plain.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&plain.stderr).contains("No local issue"));

    let deleted = hosts.run(&["issue", "delete", "1", "--json"]);
    assert_eq!(deleted["result"]["deleted"], true);
    waiting_for(&mut window, |issues| issues.is_empty());
    let usage = isolated(root).args(["issue", "close"]).output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
}

#[test]
fn the_schedule_command_keeps_a_timetable_that_queues_issues_on_the_running_work_host() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    hosts.run(&["terminal", "list", "--json"]);
    let mut window = link(root);
    let dir = root.to_str().unwrap();
    let created = hosts.run(&[
        "schedule",
        "create",
        "--name",
        "Weekday triage",
        "--prompt",
        "Triage new issues",
        "--trigger",
        "weekdays",
        "--time",
        "09:00",
        "--timezone",
        "Asia/Tokyo",
        "--dir",
        dir,
        "--disabled",
        "--json",
    ]);
    assert_eq!(created["ok"], true, "{created}");
    let schedule = &created["result"]["schedule"];
    assert_eq!(schedule["number"], 1);
    assert_eq!(schedule["enabled"], false, "a schedule is made paused");
    assert_eq!(schedule["cadence"]["kind"], "weekdays");
    assert!(
        schedule["next_ms"].is_null(),
        "a paused schedule is never due"
    );

    // Nothing is queued by a paused schedule, and an unknown zone is refused.
    let bad = isolated(root)
        .args(["schedule", "edit", "1", "--timezone", "Mars/Base", "--json"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1));
    let none = hosts.run(&["issue", "list", "--json"]);
    assert!(none["result"]["issues"].as_array().unwrap().is_empty());

    let enabled = hosts.run(&["schedule", "edit", "1", "--enabled", "--json"]);
    assert_eq!(enabled["result"]["schedule"]["enabled"], true);
    assert!(enabled["result"]["schedule"]["next_ms"].is_number());

    let run = hosts.run(&["schedule", "run", "1", "--json"]);
    assert_eq!(run["result"]["run"]["outcome"], "queued", "{run}");
    let issues = hosts.run(&["issue", "list", "--json"]);
    let issue = &issues["result"]["issues"][0];
    assert_eq!(issue["title"], "Weekday triage");
    assert_eq!(issue["body"], "Triage new issues");
    assert_eq!(issue["schedule"], created["result"]["schedule"]["id"]);

    // Deleting the schedule leaves the issue it queued.
    let removed = hosts.run(&["schedule", "remove", "1", "--json"]);
    assert_eq!(removed["result"]["removed"], true);
    let listed = hosts.run(&["schedule", "list", "--json"]);
    assert!(listed["result"]["schedules"].as_array().unwrap().is_empty());
    waiting_for(&mut window, |issues| {
        issues.len() == 1 && issues[0].schedule.is_some()
    });
    let usage = isolated(root).args(["schedule", "edit"]).output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
}
