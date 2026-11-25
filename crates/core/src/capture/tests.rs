use std::time::Duration;

use crate::actions::Action;
use crate::content::Area;
use crate::details::panel::tests::{PINNED_TAB, pinned_in, resource};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::{Holder, holder};
use crate::scenario::Scenario;
use tsumugi_model::Key;

use super::Stage;

fn perform(run: Scenario, action: Action) -> Scenario {
    run.send(Msg::Perform {
        action,
        key: None,
        probe: Box::default(),
    })
}

fn asks(run: &Scenario) -> Vec<Effect> {
    run.last_effects()
        .iter()
        .filter(|e| !matches!(e, Effect::Host { .. }))
        .cloned()
        .collect()
}

fn shown() -> Scenario {
    pinned_in(PINNED_TAB, vec![resource("/w", "/etc/a.conf")])
}

fn stage(run: &Scenario) -> Option<&Stage> {
    run.state().overlays.capture.as_ref().map(|c| &c.stage)
}

/// Two screens taken, one wide and one tall, the first shown.
fn choosing() -> Scenario {
    perform(shown(), Action::ResourceCapture)
        .send(Msg::CaptureTaken(vec![[1920, 1080], [1080, 1920]]))
}

fn key(name: &str) -> Msg {
    Msg::HeldKey(Key::new(name))
}

fn drag(from: [f64; 2], to: [f64; 2], done: bool) -> Msg {
    Msg::CaptureDrag { from, to, done }
}

#[test]
fn capturing_now_asks_for_the_screens_at_once_without_holding_the_keys() {
    let run = perform(shown(), Action::ResourceCapture);

    assert!(matches!(
        asks(&run)[..],
        [Effect::CaptureScreen { delay }] if delay == Duration::ZERO
    ));
    assert_eq!(stage(&run), Some(&Stage::Taking));
    assert_eq!(holder(run.state()), None);
}

#[test]
fn capturing_later_waits_the_seconds_the_settings_give() {
    let run = perform(shown(), Action::ResourceCaptureLater);

    assert!(matches!(
        asks(&run)[..],
        [Effect::CaptureScreen { delay }] if delay == Duration::from_secs(5)
    ));
}

#[test]
fn capturing_with_no_worktree_shown_says_so_and_takes_nothing() {
    let run = perform(
        Scenario::new().send(Msg::Focus(Area::Details)),
        Action::ResourceCapture,
    );

    assert!(stage(&run).is_none());
    assert!(matches!(asks(&run)[..], [Effect::Notice(_)]));
}

#[test]
fn a_second_capture_waits_for_the_one_being_taken() {
    let run = perform(
        perform(shown(), Action::ResourceCapture),
        Action::ResourceCapture,
    );

    assert!(matches!(asks(&run)[..], [Effect::Notice(_)]));
}

#[test]
fn the_screens_taken_are_shown_to_choose_from_holding_the_keys() {
    let run = choosing();

    assert_eq!(
        stage(&run),
        Some(&Stage::Choosing {
            screens: vec![[1920, 1080], [1080, 1920]],
            shown: 0,
            part: None,
        })
    );
    assert_eq!(holder(run.state()), Some(Holder::Capture));
}

#[test]
fn a_capture_that_failed_says_why_and_ends() {
    let run = perform(shown(), Action::ResourceCapture)
        .send(Msg::CaptureFailed("no screen to capture".into()));

    assert!(stage(&run).is_none());
    assert!(matches!(&asks(&run)[..], [Effect::Notice(text)] if text == "no screen to capture"));
}

#[test]
fn screens_taken_when_none_was_asked_for_are_let_go() {
    let run = shown().send(Msg::CaptureTaken(vec![[10, 10]]));

    assert!(stage(&run).is_none());
    assert!(matches!(asks(&run)[..], [Effect::DropCapture]));
}

#[test]
fn dragging_marks_the_part_from_either_corner_inside_the_screen() {
    let run = choosing().send(drag([500.0, 400.0], [-20.0, 100.4], false));

    assert!(matches!(
        stage(&run),
        Some(Stage::Choosing {
            part: Some([0, 100, 500, 300]),
            ..
        })
    ));
    assert!(asks(&run).is_empty());
}

#[test]
fn letting_go_of_a_drag_keeps_its_part_of_the_screen_shown() {
    let run = choosing().send(drag([10.0, 20.0], [110.0, 70.0], true));

    assert!(stage(&run).is_none());
    assert!(matches!(
        &asks(&run)[..],
        [Effect::KeepCapture { dir, screen: 0, part: [10, 20, 100, 50], .. }]
            if dir.to_str() == Some("/w")
    ));
}

#[test]
fn a_click_without_a_drag_marks_no_part_and_keeps_nothing() {
    let run = choosing().send(drag([10.0, 20.0], [11.0, 20.0], true));

    assert!(matches!(
        stage(&run),
        Some(Stage::Choosing { part: None, .. })
    ));
    assert!(asks(&run).is_empty());
}

#[test]
fn enter_keeps_the_part_marked() {
    let run = choosing()
        .send(drag([10.0, 20.0], [110.0, 70.0], false))
        .send(key("Enter"));

    assert!(matches!(
        &asks(&run)[..],
        [Effect::KeepCapture {
            part: [10, 20, 100, 50],
            ..
        }]
    ));
}

#[test]
fn enter_with_no_part_marked_keeps_the_whole_screen_shown() {
    let run = choosing().send(key("Tab")).send(key("Enter"));

    assert!(matches!(
        &asks(&run)[..],
        [Effect::KeepCapture {
            screen: 1,
            part: [0, 0, 1080, 1920],
            ..
        }]
    ));
}

#[test]
fn tab_shows_the_next_screen_round_to_the_first_forgetting_the_part() {
    let run = choosing()
        .send(drag([10.0, 20.0], [110.0, 70.0], false))
        .send(key("Tab"));
    assert!(matches!(
        stage(&run),
        Some(Stage::Choosing {
            shown: 1,
            part: None,
            ..
        })
    ));

    let round = run.send(key("Tab"));
    assert!(matches!(
        stage(&round),
        Some(Stage::Choosing { shown: 0, .. })
    ));
}

#[test]
fn escape_keeps_nothing_and_lets_the_screens_go() {
    let run = choosing().send(key("Escape"));

    assert!(stage(&run).is_none());
    assert!(matches!(asks(&run)[..], [Effect::DropCapture]));
}

#[test]
fn cancelling_from_the_stills_keeps_nothing() {
    let run = choosing().send(Msg::CaptureCancel);

    assert!(stage(&run).is_none());
    assert!(matches!(asks(&run)[..], [Effect::DropCapture]));
}
