use tsumugi_model::{EnvironmentId, Key, Notice, NoticeKind, State, Wait};
use tsumugi_protocol::TreeChange;
use tsumugi_protocol::testing::{patch, snapshot};

use super::*;
use crate::config::testing::configured;
use crate::content::Tab;
use crate::msg::Msg;
use crate::picker::Picking;
use crate::scenario::Scenario;
use crate::testing::{here, listing, session, shows_there, there};

fn notice(id: u64, kind: NoticeKind, session: &str, at_ms: u64) -> Notice {
    Notice {
        id,
        kind,
        session: session.into(),
        name: format!("{session}-name"),
        workspace: "r".into(),
        at_ms,
        reason: "the reason".into(),
    }
}

/// A window whose host lists session `a` as `state`, and keeps `notices`.
fn keeping(state: State, notices: Vec<Notice>) -> Scenario {
    Scenario::new().event(
        snapshot()
            .listed(
                tsumugi_protocol::testing::listed().session(session("a").cwd("/r").state(state)),
            )
            .with(move |w| w.tree.notices = notices.clone()),
    )
}

fn told(notices: Vec<Notice>) -> TreeChange {
    TreeChange::Notices { notices }
}

fn ids(run: &Scenario, query: &str) -> Vec<u64> {
    let state = run.state();
    rows(&state.notices, &state.hosts, query)
        .iter()
        .map(|r| r.notice.id)
        .collect()
}

fn ctrl(name: &str) -> Msg {
    Msg::PickerKey(Key::new(name).ctrl())
}

fn open_list(run: Scenario) -> Scenario {
    run.send(Msg::NoticesOpen)
}

fn kept(run: &Scenario) -> Vec<Reads> {
    run.last_effects()
        .iter()
        .filter_map(|e| match e {
            Effect::KeepReads(reads) => Some(reads.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_list_puts_what_still_stands_first_then_the_unread_and_the_newest() {
    let run = keeping(
        State::Done,
        vec![
            notice(0, NoticeKind::Failed, "gone", 10),
            notice(1, NoticeKind::Done, "a", 20),
            notice(2, NoticeKind::Failed, "gone", 30),
        ],
    );
    assert_eq!(ids(&run, ""), [1, 2, 0]);
    // Read, it goes behind the unread of its kind.
    let run = open_list(run).send(ctrl("u"));
    assert_eq!(ids(&run, ""), [1, 2, 0]);
    let run = run.send(Msg::PickerKey(Key::new("Escape")));
    assert!(run.state().picker.open.is_none());
}

#[test]
fn a_notice_of_a_session_that_moved_on_or_ended_is_not_current() {
    let run = keeping(
        State::Working,
        vec![
            notice(0, NoticeKind::Waiting, "a", 1),
            notice(1, NoticeKind::Failed, "gone", 2),
        ],
    );
    let state = run.state();
    let standings: Vec<_> = rows(&state.notices, &state.hosts, "")
        .iter()
        .map(|r| (r.notice.id, r.standing))
        .collect();
    assert_eq!(standings, [(1, Standing::Ended), (0, Standing::Past)]);
}

#[test]
fn a_waiting_session_stands_current_and_leads_the_list() {
    let run = keeping(
        State::Waiting(Wait::Input),
        vec![
            notice(0, NoticeKind::Failed, "gone", 50),
            notice(1, NoticeKind::Waiting, "a", 10),
        ],
    );
    assert_eq!(ids(&run, ""), [1, 0]);
}

#[test]
fn typing_narrows_the_list_by_name_kind_and_reason() {
    let run = keeping(
        State::Done,
        vec![
            notice(0, NoticeKind::Failed, "gone", 1),
            notice(1, NoticeKind::Done, "a", 2),
        ],
    );
    assert_eq!(ids(&run, "failed"), [0]);
    assert_eq!(ids(&run, "A-NAME done"), [1]);
    assert_eq!(ids(&run, "the reason"), [1, 0]);
    assert!(ids(&run, "nothing like it").is_empty());
}

#[test]
fn a_key_marks_the_row_under_the_cursor_read_and_unread_and_the_marks_are_kept() {
    let run = open_list(keeping(
        State::Done,
        vec![notice(4, NoticeKind::Done, "a", 1)],
    ));
    assert!(!rows(&run.state().notices, &run.state().hosts, "")[0].read);
    let run = run.send(ctrl("u"));
    assert!(rows(&run.state().notices, &run.state().hosts, "")[0].read);
    assert_eq!(kept(&run).len(), 1);
    assert!(kept(&run)[0].is_read(&here(), 4));
    let run = run.send(ctrl("u"));
    assert!(!rows(&run.state().notices, &run.state().hosts, "")[0].read);
    assert!(!kept(&run)[0].is_read(&here(), 4));
}

#[test]
fn a_key_marks_every_row_shown_read() {
    let run = open_list(keeping(
        State::Done,
        vec![
            notice(0, NoticeKind::Failed, "gone", 1),
            notice(1, NoticeKind::Done, "a", 2),
        ],
    ))
    .send(ctrl("a"));
    assert!(
        rows(&run.state().notices, &run.state().hosts, "")
            .iter()
            .all(|r| r.read)
    );
    // Again changes nothing, so nothing is kept again.
    assert!(kept(&run.send(ctrl("a"))).is_empty());
}

#[test]
fn choosing_a_row_whose_session_runs_shows_it_and_marks_it_read() {
    let run = open_list(keeping(
        State::Done,
        vec![notice(1, NoticeKind::Done, "a", 2)],
    ))
    .send(Msg::PickerChoose(0));
    assert!(run.state().picker.open.is_none());
    assert_eq!(
        run.state().content.active().cloned(),
        Some(Tab::Session(here(), "a".into()))
    );
    assert!(run.state().notices.reads.is_read(&here(), 1));
}

#[test]
fn choosing_a_row_whose_session_ended_marks_it_read_and_stays() {
    let run = open_list(keeping(
        State::Working,
        vec![notice(1, NoticeKind::Failed, "gone", 2)],
    ))
    .send(Msg::PickerChoose(0));
    assert!(matches!(run.state().picker.open, Some(Picking::Notices(_))));
    assert!(run.state().content.active().is_none());
    assert!(run.state().notices.reads.is_read(&here(), 1));
}

#[test]
fn two_environments_keep_their_own_numbers_and_marks() {
    let run = shows_there(
        keeping(State::Done, vec![notice(0, NoticeKind::Done, "a", 1)]),
        [session("b").cwd("/r")],
    )
    .event_from(
        there(),
        patch(2).change(told(vec![notice(0, NoticeKind::Failed, "b", 2)])),
    );
    let run = open_list(run).send(ctrl("u"));
    let state = run.state();
    let all = rows(&state.notices, &state.hosts, "");
    assert_eq!(all.len(), 2);
    assert_eq!(all.iter().filter(|r| r.read).count(), 1);
}

#[test]
fn what_was_read_comes_back_and_what_the_host_no_longer_keeps_is_forgotten() {
    let mut reads = Reads::default();
    reads.set(&here(), 1, true);
    reads.set(&here(), 9, true);
    reads.set(&EnvironmentId::new("elsewhere"), 3, true);
    let text = reads.to_json();
    assert_eq!(Reads::from_json(text.as_bytes()).unwrap(), reads);

    let run = keeping(State::Done, vec![notice(1, NoticeKind::Done, "a", 1)])
        .send(Msg::ReadsRestored(reads))
        .event(patch(2).change(told(vec![notice(1, NoticeKind::Done, "a", 1)])));
    let reads = &run.state().notices.reads;
    assert!(reads.is_read(&here(), 1));
    // 9 is no longer kept by the host; an environment not heard from keeps
    // its marks.
    assert!(!reads.is_read(&here(), 9));
    assert!(reads.is_read(&EnvironmentId::new("elsewhere"), 3));
}

fn told_of(run: &Scenario) -> Vec<Vec<(String, String)>> {
    run.effects()
        .iter()
        .filter_map(|e| match e {
            Effect::NotifyNotices(of) => Some(of.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_failure_and_a_completion_are_told_of_once_as_the_settings_say() {
    let new = vec![
        notice(0, NoticeKind::Done, "a", 1),
        notice(1, NoticeKind::Failed, "a", 2),
        notice(2, NoticeKind::Waiting, "a", 3),
    ];
    for (settings, expected) in [
        (
            "notify_failed = true\nnotify_done = true\n",
            vec![
                ("a-name".to_string(), "done: the reason".to_string()),
                ("a-name".to_string(), "failed: the reason".to_string()),
            ],
        ),
        (
            "notify_failed = true\n",
            vec![("a-name".to_string(), "failed: the reason".to_string())],
        ),
        (
            "notify_done = true\n",
            vec![("a-name".to_string(), "done: the reason".to_string())],
        ),
    ] {
        let run = Scenario::new()
            .event(configured(settings))
            .event(keeping_nothing())
            .event(patch(2).change(told(new.clone())));
        assert_eq!(told_of(&run), [expected], "{settings}");
        // The same notices heard again, as when the link comes back, are
        // not told of again.
        let again = run.event(patch(3).change(told(new.clone())));
        assert_eq!(told_of(&again).len(), 1, "{settings}");
    }
}

fn keeping_nothing() -> tsumugi_protocol::testing::SnapshotBuilder {
    snapshot().listed(tsumugi_protocol::testing::listed().session(session("a").cwd("/r")))
}

#[test]
fn nothing_is_told_by_default() {
    let run = Scenario::new()
        .event(keeping_nothing())
        .event(patch(2).change(told(vec![
            notice(0, NoticeKind::Failed, "a", 1),
            notice(1, NoticeKind::Done, "a", 2),
        ])));
    assert!(told_of(&run).is_empty());
}

#[test]
fn what_a_host_kept_before_the_window_heard_from_it_is_not_told_of() {
    // The host's first word holds its settings and what it kept.
    let run = Scenario::new().event(keeping_nothing().seq(0).with(|w| {
        w.config.settings = Some(crate::config::testing::settings_file(
            "notify_failed = true\nnotify_done = true\n",
        ));
        w.tree.notices = vec![notice(5, NoticeKind::Failed, "a", 1)];
    }));
    assert!(told_of(&run).is_empty());
    let run = run.event(patch(1).change(told(vec![
        notice(5, NoticeKind::Failed, "a", 1),
        notice(6, NoticeKind::Done, "a", 2),
    ])));
    assert_eq!(
        told_of(&run),
        [[("a-name".to_string(), "done: the reason".to_string())]]
    );
}

#[test]
fn a_host_that_kept_none_when_first_heard_tells_of_its_first() {
    let run = Scenario::new()
        .event(configured("notify_failed = true\n"))
        .event(keeping_nothing())
        .event(patch(2).change(told(vec![notice(0, NoticeKind::Failed, "a", 1)])));
    assert_eq!(told_of(&run).len(), 1);
}

#[test]
fn listing_alone_tells_nothing_of_the_notices() {
    let run = Scenario::new()
        .event(configured("notify_failed = true\n"))
        .event(listing([session("a").cwd("/r")]));
    assert!(told_of(&run).is_empty());
}
