//! A window pins, describes and unpins the resources of a project on the work
//! host, and every window is told what the project keeps.
#![cfg(unix)]

mod support {
    pub mod isolated;
}

use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use support::isolated::Hosts;
use tsumugi_model::Resource;
use tsumugi_protocol::{Answer, Ask, Event, Request, SPOKEN, decode, encode};

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
    assert!(
        reader.read_line(&mut line).unwrap() > 0,
        "the host disconnected"
    );
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

fn kept(reader: &mut Link) -> Vec<Resource> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        reader
            .get_mut()
            .write_all(encode(&Request::Snapshot).as_bytes())
            .unwrap();
        if let Event::Snapshot(work) = loop {
            let event = event(reader);
            if matches!(event, Event::Snapshot(_)) {
                break event;
            }
        } {
            return work.tree.resources;
        }
        assert!(Instant::now() < deadline);
    }
}

fn waiting_for(reader: &mut Link, ready: impl Fn(&[Resource]) -> bool) -> Vec<Resource> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let resources = kept(reader);
        if ready(&resources) {
            return resources;
        }
        assert!(
            Instant::now() < deadline,
            "resources did not change: {resources:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn a_project_pins_describes_and_unpins_resources_through_the_work_host() {
    let hosts = Hosts::new();
    let root = hosts.0.path();
    let project = root.join("project");
    std::fs::create_dir(&project).unwrap();
    let project = std::fs::canonicalize(&project).unwrap();
    hosts.run(&["terminal", "list", "--json"]);
    let mut connection = link(root);

    // An address is kept as it is, with its label.
    assert_eq!(
        ask(
            &mut connection,
            Ask::PinResource {
                dir: project.clone(),
                target: "https://example.com/doc".into(),
                label: Some("Guide".into()),
            }
        ),
        Answer::Done
    );
    // A relative path is taken from the directory it is typed in.
    assert_eq!(
        ask(
            &mut connection,
            Ask::PinResource {
                dir: project.clone(),
                target: "conf/app.toml".into(),
                label: None,
            }
        ),
        Answer::Done
    );
    let pinned = waiting_for(&mut connection, |r| r.len() == 2);
    assert_eq!(pinned[0].target, "https://example.com/doc");
    assert_eq!(pinned[0].label.as_deref(), Some("Guide"));
    assert_eq!(pinned[0].project, project);
    assert_eq!(
        pinned[1].target,
        project.join("conf/app.toml").to_string_lossy()
    );

    assert_eq!(
        ask(
            &mut connection,
            Ask::DescribeResource {
                id: pinned[0].id.clone(),
                label: Some("API guide".into()),
                note: Some("Read first".into()),
            }
        ),
        Answer::Done
    );
    let described = waiting_for(&mut connection, |r| {
        r.first().is_some_and(|r| r.note.is_some())
    });
    assert_eq!(described[0].label.as_deref(), Some("API guide"));
    assert_eq!(described[0].note.as_deref(), Some("Read first"));

    assert_eq!(
        ask(
            &mut connection,
            Ask::UnpinResource {
                id: pinned[0].id.clone()
            }
        ),
        Answer::Done
    );
    let left = waiting_for(&mut connection, |r| r.len() == 1);
    assert_eq!(left[0].id, pinned[1].id);

    // What the host refuses it says, and keeps nothing.
    let Answer::Failed { text } = ask(
        &mut connection,
        Ask::PinResource {
            dir: project.clone(),
            target: "two words".into(),
            label: None,
        },
    ) else {
        panic!("a target with spaces is refused");
    };
    assert!(text.contains("spaces"), "{text}");
    let Answer::Failed { .. } = ask(&mut connection, Ask::UnpinResource { id: "gone".into() })
    else {
        panic!("a resource not kept cannot be unpinned");
    };
    assert_eq!(kept(&mut connection).len(), 1);
}
