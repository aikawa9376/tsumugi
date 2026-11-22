use tsumugi_model::{Agent, Key};
use tsumugi_protocol::{PastChange, Request};

use super::*;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{listing, session, session_tab};

const HOME: &str = "/Users/me";

fn window() -> Scenario {
    Scenario::new().send(Msg::Opened {
        home: HOME.into(),
        environment: crate::testing::here(),
    })
}

/// The rows the list draws; none while they are read.
fn drawn(run: &Scenario) -> Option<Vec<Row>> {
    let view = &run.state().picker;
    match &view.open {
        Some(super::super::Picking::Resume(list)) => {
            rows(list, &view.home, run.state().hosts.work(&list.environment))
        }
        _ => None,
    }
}

fn past(agent: Agent, id: &str, cwd: &str, title: &str) -> Past {
    Past {
        agent,
        id: id.into(),
        cwd: cwd.into(),
        title: title.into(),
        at_ms: 0,
        ..Default::default()
    }
}

/// The conversations read for `dir`, or for every directory.
fn pasts(dir: Option<&str>, conversations: Vec<Past>) -> PastChange {
    PastChange::Listed {
        sight: PastSight {
            dir: dir.map(Into::into),
        },
        conversations,
    }
}

fn watched(run: &Scenario) -> Option<Option<PastSight>> {
    run.effects()
        .iter()
        .filter_map(|e| match e {
            Effect::Host {
                request: Request::Watch(sight),
                ..
            } => Some(sight.past.clone()),
            _ => None,
        })
        .next_back()
}

fn here(dir: &str) -> Option<Option<PastSight>> {
    Some(Some(PastSight {
        dir: Some(dir.into()),
    }))
}

fn every() -> Option<Option<PastSight>> {
    Some(Some(PastSight { dir: None }))
}

fn key(name: &str, ctrl: bool) -> Msg {
    Msg::PickerKey(Key {
        ctrl,
        ..Key::new(name)
    })
}

fn titles(run: &Scenario) -> Vec<String> {
    drawn(run)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.past.title)
        .collect()
}

/// A window showing the session `a`, which runs in `/r/src`.
fn showing_a() -> Scenario {
    window()
        .event(listing([session("a").cwd("/r/src").session_id("conv-a")]))
        .send(Msg::ShowSession(crate::testing::here(), "a".into()))
}

#[test]
fn the_list_asks_for_the_conversations_of_where_the_focused_window_works() {
    let run = showing_a().send(Msg::ResumeOpen);
    assert_eq!(watched(&run), here("/r/src"));
}

#[test]
fn with_nothing_open_every_directory_s_conversations_are_listed() {
    let run = window().send(Msg::ResumeOpen);
    assert_eq!(watched(&run), every());
    // There is no directory's to go back to.
    let run = run.send(key("a", true));
    assert_eq!(watched(&run), every());
}

#[test]
fn ctrl_a_lists_every_directory_s_and_again_the_directory_s() {
    let run = showing_a().send(Msg::ResumeOpen).send(key("a", true));
    assert_eq!(watched(&run), every());
    let run = run.send(key("a", true));
    assert_eq!(watched(&run), here("/r/src"));
}

#[test]
fn closing_the_list_stops_the_host_listing() {
    for close in [key("Escape", false), key("c", true), Msg::PickerClose] {
        let run = showing_a().send(Msg::ResumeOpen).send(close.clone());
        assert_eq!(watched(&run), Some(None), "{close:?}");
        assert!(run.state().picker.open.is_none());
    }
}

#[test]
fn nothing_is_drawn_until_the_list_asked_for_is_read() {
    let run = showing_a()
        .send(Msg::ResumeOpen)
        .tell(pasts(None, vec![past(Agent::Claude, "x", "/q", "other")]));
    assert_eq!(drawn(&run), None);
}

#[test]
fn typing_narrows_the_list_by_title_and_directory() {
    let listed = vec![
        past(Agent::Claude, "1", "/r/src", "fix the login"),
        past(Agent::Codex, "2", "/r/src", "review the API"),
        past(Agent::Claude, "3", "/r/src/docs", "write the guide"),
    ];
    let cases = [
        (
            "",
            vec!["fix the login", "review the API", "write the guide"],
        ),
        ("login", vec!["fix the login"]),
        ("docs", vec!["write the guide"]),
    ];
    for (typed, want) in cases {
        let run = showing_a()
            .send(Msg::ResumeOpen)
            .tell(pasts(Some("/r/src"), listed.clone()))
            .send(Msg::PickerQuery(typed.into()));
        assert_eq!(titles(&run), want, "{typed}");
    }
}

#[test]
fn a_directory_is_matched_as_drawn_with_home_as_a_tilde() {
    let listed = vec![
        past(Agent::Claude, "1", "/Users/me/w", "one"),
        past(Agent::Claude, "2", "/srv/w", "two"),
    ];
    let cases = [("~/w", vec!["one"]), ("Users", vec![])];
    for (typed, want) in cases {
        let run = showing_a()
            .send(Msg::ResumeOpen)
            .tell(pasts(Some("/r/src"), listed.clone()))
            .send(Msg::PickerQuery(typed.into()));
        assert_eq!(titles(&run), want, "{typed}");
    }
}

#[test]
fn choosing_a_conversation_no_session_holds_resumes_it_and_closes_the_list() {
    let chosen = past(Agent::Codex, "019f", "/r/src", "review the API");
    let run = showing_a()
        .send(Msg::ResumeOpen)
        .tell(pasts(
            Some("/r/src"),
            vec![past(Agent::Claude, "1", "/r/src", "fix"), chosen.clone()],
        ))
        .send(key("ArrowDown", false))
        .send(key("Enter", false));
    assert!(run.last_effects().contains(&Effect::Resume {
        environment: crate::testing::here(),
        past: chosen,
    }));
    assert!(run.state().picker.open.is_none());
}

#[test]
fn choosing_a_conversation_a_session_here_holds_shows_that_session() {
    let run = window()
        .event(listing([
            session("a").cwd("/r/src").session_id("conv-a"),
            session("b").cwd("/r/src").session_id("conv-b"),
        ]))
        .send(Msg::ShowSession(crate::testing::here(), "a".into()))
        .send(Msg::ResumeOpen)
        .tell(pasts(
            Some("/r/src"),
            vec![past(Agent::Claude, "conv-b", "/r/src", "the other")],
        ));
    let got = drawn(&run).unwrap();
    assert_eq!(got[0].session.as_deref(), Some("b"));
    let run = run.send(Msg::PickerChoose(0));
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::Resume { .. }))
    );
    assert_eq!(run.state().content.showing().tab, Some(session_tab("b")));
}

#[test]
fn the_same_id_from_another_agent_is_not_the_session_s() {
    let run = showing_a().send(Msg::ResumeOpen).tell(pasts(
        Some("/r/src"),
        vec![past(Agent::Codex, "conv-a", "/r/src", "codex")],
    ));
    let got = drawn(&run).unwrap();
    assert_eq!(got[0].session, None);
}

#[test]
fn the_cursor_goes_round_the_rows_and_a_row_past_them_chooses_nothing() {
    let run = showing_a().send(Msg::ResumeOpen).tell(pasts(
        Some("/r/src"),
        vec![
            past(Agent::Claude, "1", "/r/src", "one"),
            past(Agent::Claude, "2", "/r/src", "two"),
        ],
    ));
    let up = run.send(key("p", true));
    assert_eq!(up.state().picker.open.as_ref().unwrap().cursor(), 1);
    let round = up.send(key("n", true));
    assert_eq!(round.state().picker.open.as_ref().unwrap().cursor(), 0);
    let nothing = round.send(Msg::PickerChoose(5));
    assert!(nothing.last_effects().is_empty());
    assert!(nothing.state().picker.open.is_some());
}

#[test]
fn another_environment_s_tab_lists_and_resumes_the_conversations_kept_there() {
    use crate::testing::{shows_there, there};
    let chosen = past(Agent::Claude, "t1", "/t", "kept there");
    let run = shows_there(window(), [session("b").cwd("/t")])
        .send(Msg::ShowSession(there(), "b".into()))
        .send(Msg::ResumeOpen);
    let past_watched = |environment| {
        run.state()
            .hosts
            .get(&environment)
            .map(|h| h.watched.past.clone())
    };
    assert_eq!(past_watched(there()), Some(here("/t").flatten()));
    assert_eq!(past_watched(crate::testing::here()), Some(None));
    let run = run
        .tell_from(there(), pasts(Some("/t"), vec![chosen.clone()]))
        .send(key("Enter", false));
    assert!(run.last_effects().contains(&Effect::Resume {
        environment: there(),
        past: chosen,
    }));
}

/// A list of three conversations opened where the session `a` runs, the
/// cursor on the first.
fn listed() -> Scenario {
    let mut kept = past(Agent::Claude, "c1", "/r/src", "fix the login");
    kept.command = "cd /r/src && claude --resume c1".into();
    kept.log = Some("/Users/me/.claude/projects/-r-src/c1.jsonl".into());
    showing_a().send(Msg::ResumeOpen).tell(pasts(
        Some("/r/src"),
        vec![
            kept,
            past(Agent::Codex, "c2", "/r/docs", "review the API"),
            past(Agent::Claude, "c3", "/r/docs", "write the guide"),
        ],
    ))
}

fn headed(run: &Scenario) -> Vec<(Option<String>, String)> {
    drawn(run)
        .unwrap_or_default()
        .into_iter()
        .map(|r| (r.heading, r.past.title))
        .collect()
}

#[test]
fn rows_are_grouped_by_agent_or_by_directory_and_ungrouped_again() {
    let run = listed();
    assert!(headed(&run).iter().all(|(heading, _)| heading.is_none()));
    let by_agent = run.send(key("g", true));
    assert_eq!(
        headed(&by_agent),
        [
            (Some("Claude Code".to_string()), "fix the login".to_string()),
            (None, "write the guide".to_string()),
            (Some("Codex".to_string()), "review the API".to_string()),
        ]
    );
    let by_dir = by_agent.send(key("g", true));
    assert_eq!(
        headed(&by_dir),
        [
            (Some("/r/src".to_string()), "fix the login".to_string()),
            (Some("/r/docs".to_string()), "review the API".to_string()),
            (None, "write the guide".to_string()),
        ]
    );
    let plain = by_dir.send(key("g", true));
    assert!(headed(&plain).iter().all(|(heading, _)| heading.is_none()));
}

#[test]
fn typing_narrows_a_grouped_list_and_keeps_its_groups() {
    let run = listed().send(key("g", true)).send(key("g", true));
    let run = run.send(Msg::PickerQuery("docs".into()));
    assert_eq!(
        headed(&run),
        [
            (Some("/r/docs".to_string()), "review the API".to_string()),
            (None, "write the guide".to_string()),
        ]
    );
}

/// The effects `msg` has, on the list where the cursor is on the row `at`.
fn acting(at: usize, msg: Msg) -> Vec<Effect> {
    let mut run = listed();
    for _ in 0..at {
        run = run.send(key("n", true));
    }
    run.send(msg).last_effects().to_vec()
}

#[test]
fn the_row_under_the_cursor_can_be_copied_as_its_resume_command_its_id_or_its_log_path() {
    let copied = |text: &str, done: &str| {
        vec![Effect::Copy {
            text: text.into(),
            done: done.into(),
        }]
    };
    assert_eq!(
        acting(0, key("y", true)),
        copied(
            "cd /r/src && claude --resume c1",
            "Copied the resume command"
        )
    );
    assert_eq!(
        acting(0, key("s", true)),
        copied("c1", "Copied the session ID")
    );
    assert_eq!(
        acting(0, key("l", true)),
        copied(
            "/Users/me/.claude/projects/-r-src/c1.jsonl",
            "Copied the log path"
        )
    );
}

#[test]
fn copying_keeps_the_list_open_and_says_so_when_there_is_nothing_to_copy() {
    let run = listed().send(key("y", true));
    assert!(run.state().picker.open.is_some());
    // The second row has no log and no command.
    for typed in ["l", "y", "o"] {
        let got = acting(1, key(typed, true));
        assert!(
            matches!(got.as_slice(), [Effect::Notice(_)]),
            "{typed}: {got:?}"
        );
    }
}

#[test]
fn the_log_is_opened_in_the_content_and_the_list_closes() {
    let run = listed().send(key("o", true));
    assert!(run.last_effects().contains(&Effect::OpenFile {
        environment: crate::testing::here(),
        path: "/Users/me/.claude/projects/-r-src/c1.jsonl".into(),
        how: crate::content::Opening::Existing,
    }));
    assert!(run.state().picker.open.is_none());
}

#[test]
fn a_conversation_is_forked_in_a_new_session_even_when_a_session_holds_it() {
    let run = window()
        .event(listing([session("a").cwd("/r/src").session_id("conv-a")]))
        .send(Msg::ShowSession(crate::testing::here(), "a".into()))
        .send(Msg::ResumeOpen)
        .tell(pasts(
            Some("/r/src"),
            vec![past(Agent::Claude, "conv-a", "/r/src", "mine")],
        ))
        .send(key("f", true));
    assert!(run.last_effects().contains(&Effect::Fork {
        environment: crate::testing::here(),
        past: past(Agent::Claude, "conv-a", "/r/src", "mine"),
    }));
    assert!(run.state().picker.open.is_none());
}

#[test]
fn row_actions_with_no_list_read_do_nothing() {
    for typed in ["y", "s", "l", "o", "f"] {
        let run = showing_a().send(Msg::ResumeOpen).send(key(typed, true));
        assert!(run.last_effects().is_empty(), "{typed}");
        assert!(run.state().picker.open.is_some());
    }
}
