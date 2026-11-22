use tsumugi_model::{Rest, SessionId, State};

use crate::actions::Action;
use crate::console::Probe;
use crate::content::Area;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::confirm::Ask;
use crate::scenario::Scenario;
use crate::testing::{here, listed, session, shell, snapshot, worktree};
use crate::tree::nav::Item;

fn wakeable(id: &str) -> tsumugi_model::testing::SessionBuilder {
    session(id).session_id("c1").delegation(SessionId::new())
}

/// The repository `/r` with the worktree `/r/a`, which holds these sessions
/// (`a` with `a.cwd` = `/r/a`) and shells.
fn repository(sessions: Vec<tsumugi_model::testing::SessionBuilder>, shells: &[u128]) -> Scenario {
    let mut listing = listed();
    for s in sessions {
        listing = listing.session(s);
    }
    for id in shells {
        listing = listing.terminal(shell(*id, "/r/a"));
    }
    let listing = listing
        .found("/r/a", Some(worktree("/r/a", "/r", "a")))
        .worktree(worktree("/r", "/r", "main"))
        .worktree(worktree("/r/a", "/r", "a"));
    Scenario::new()
        .event(snapshot().listed(listing))
        .send(Msg::Focus(Area::Sidebar))
}

fn perform(action: Action) -> Msg {
    Msg::Perform {
        action,
        key: None,
        probe: Box::new(Probe::default()),
    }
}

fn on_worktree(run: Scenario, action: Action) -> Scenario {
    run.send(Msg::TreeSelect(Some(Item::Worktree(here(), "/r/a".into()))))
        .send(perform(action))
}

#[test]
fn finished_sessions_are_put_to_sleep_without_a_question() {
    let run = repository(
        vec![
            wakeable("a").cwd("/r/a").state(State::Done),
            wakeable("b").cwd("/r/a").state(State::Idle),
        ],
        &[],
    );

    let run = on_worktree(run, Action::WorktreeSleep);

    assert_eq!(
        run.last_effects(),
        [Effect::Sleep {
            environment: here(),
            ids: vec!["a".into(), "b".into()]
        }]
    );
    assert!(run.state().overlays.confirm.is_none());
}

#[test]
fn a_session_that_works_or_a_terminal_that_runs_is_asked_about_first() {
    for (sessions, shells) in [
        (
            vec![wakeable("a").cwd("/r/a").state(State::Working)],
            vec![],
        ),
        (vec![wakeable("a").cwd("/r/a").state(State::Done)], vec![7]),
    ] {
        let run = repository(sessions, &shells);

        let run = on_worktree(run, Action::WorktreeSleep);

        assert!(run.last_effects().is_empty(), "{:?}", run.last_effects());
        let Some(Ask::SleepWorktree { .. }) = run.state().overlays.confirm.clone() else {
            panic!("{:?}", run.state().overlays.confirm);
        };
    }
}

#[test]
fn the_question_says_what_is_stopped_and_that_the_work_stays() {
    let run = repository(
        vec![wakeable("a").cwd("/r/a").state(State::Working)],
        &[7, 8],
    );
    let run = on_worktree(run, Action::WorktreeSleep);

    let ask = run.state().overlays.confirm.clone().unwrap();
    let (_, title, body, action) = ask.dialog();

    assert_eq!(title, "Put a to sleep?");
    assert!(body.contains("2 terminals end"), "{body}");
    assert!(body.contains("1 session is working"), "{body}");
    assert!(body.contains("files and branch stay"), "{body}");
    assert_eq!(action, "Sleep");
    assert!(ask.destroys(), "terminals end");
}

#[test]
fn answering_yes_ends_the_terminals_and_puts_the_sessions_to_sleep() {
    let run = repository(vec![wakeable("a").cwd("/r/a").state(State::Working)], &[7]);
    let run = on_worktree(run, Action::WorktreeSleep).send(Msg::ConfirmAnswer(true));

    let effects = run.last_effects();

    assert!(
        matches!(effects[0], Effect::EndShell { id: 7, .. }),
        "{effects:?}"
    );
    assert_eq!(
        effects[1],
        Effect::Sleep {
            environment: here(),
            ids: vec!["a".into()]
        }
    );
}

#[test]
fn answering_no_leaves_everything_running() {
    let run = repository(vec![wakeable("a").cwd("/r/a").state(State::Working)], &[7]);

    let run = on_worktree(run, Action::WorktreeSleep).send(Msg::ConfirmAnswer(false));

    assert!(run.last_effects().is_empty());
}

#[test]
fn sessions_that_cannot_be_woken_keep_running_and_are_said_so() {
    let mut cursor = wakeable("b").cwd("/r/a").state(State::Done);
    cursor = cursor.agent(tsumugi_model::Agent::Cursor);
    let run = repository(
        vec![wakeable("a").cwd("/r/a").state(State::Done), cursor],
        &[],
    );

    let run = on_worktree(run, Action::WorktreeSleep);

    assert_eq!(
        run.last_effects(),
        [Effect::Sleep {
            environment: here(),
            ids: vec!["a".into()]
        }]
    );
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("b-name"))
    );
}

#[test]
fn a_worktree_with_nothing_to_put_to_sleep_says_so() {
    let run = repository(
        vec![
            wakeable("a")
                .cwd("/r/a")
                .state(State::Idle)
                .rest(Rest::Hibernated),
        ],
        &[],
    );

    let run = on_worktree(run, Action::WorktreeSleep);

    assert!(
        run.last_effects()
            .iter()
            .all(|e| !matches!(e, Effect::Sleep { .. }))
    );
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("nothing"))
    );
}

#[test]
fn the_main_checkout_sleeps_like_any_worktree() {
    let run = repository(vec![wakeable("a").cwd("/r").state(State::Done)], &[])
        .send(Msg::TreeSelect(Some(Item::Worktree(here(), "/r".into()))));

    let run = run.send(perform(Action::WorktreeSleep));

    assert_eq!(run.last_effects().len(), 1);
}

#[test]
fn waking_brings_back_the_sleeping_sessions_of_the_worktree() {
    let run = repository(
        vec![
            wakeable("a")
                .cwd("/r/a")
                .state(State::Idle)
                .rest(Rest::Hibernated),
            wakeable("b")
                .cwd("/r/a")
                .state(State::Idle)
                .rest(Rest::Exited),
            wakeable("c").cwd("/r/a").state(State::Done),
        ],
        &[],
    );

    let run = on_worktree(run, Action::WorktreeWake);

    assert_eq!(
        run.last_effects(),
        [Effect::BringBack {
            environment: here(),
            ids: vec!["a".into()]
        }]
    );
}

#[test]
fn a_worktree_sleeps_when_it_holds_sessions_that_all_sleep() {
    let run = repository(
        vec![
            wakeable("a")
                .cwd("/r/a")
                .state(State::Idle)
                .rest(Rest::Hibernated),
            wakeable("b")
                .cwd("/r/a")
                .state(State::Idle)
                .rest(Rest::Hibernated),
        ],
        &[],
    );
    let tree = crate::tree::listed(run.state().hosts.work(&here()));
    let w = tree
        .worktrees()
        .find(|w| w.worktree.path == std::path::Path::new("/r/a"))
        .unwrap();
    assert!(w.sleeping());

    let run = repository(
        vec![
            wakeable("a")
                .cwd("/r/a")
                .state(State::Idle)
                .rest(Rest::Hibernated),
            wakeable("b").cwd("/r/a").state(State::Done),
        ],
        &[],
    );
    let tree = crate::tree::listed(run.state().hosts.work(&here()));
    let w = tree
        .worktrees()
        .find(|w| w.worktree.path == std::path::Path::new("/r/a"))
        .unwrap();
    assert!(!w.sleeping(), "one runs");
}

#[test]
fn the_host_said_how_many_slept_and_which_could_not() {
    let run = repository(vec![], &[]).send(Msg::Slept {
        result: Ok((2, vec!["b".into()])),
    });

    let line = run.state().chrome.status.line.clone().unwrap();
    assert!(line.contains("2 sessions"), "{line}");
    assert!(line.contains("b"), "{line}");

    let failed = repository(vec![], &[]).send(Msg::Slept {
        result: Err("terminal host is gone".into()),
    });
    assert!(
        failed
            .state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("terminal host is gone"))
    );
}
