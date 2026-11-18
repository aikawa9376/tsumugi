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
fn queued_issues_start_the_default_profile_in_order_and_survive_a_work_host_restart() {
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
    for title in ["first request", "second request"] {
        let issue = NewLocalIssue {
            id: SessionId::new().to_string(),
            dir: root.into(),
            title: title.into(),
            body: "Do the requested work.".into(),
        };
        assert_eq!(
            ask(&mut connection, Ask::QueueLocalIssue(issue)),
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
    let launches = std::fs::read_to_string(runs).unwrap();
    let launches: Vec<_> = launches.lines().collect();
    assert_eq!(launches.len(), 2);
    assert!(launches[0].contains("first request"));
    assert!(launches[1].contains("second request"));
}
