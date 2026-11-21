use tsumugi_model::{Key, TerminalSession};

use super::*;
use crate::actions::Action;
use crate::console::Probe;
use crate::content::Area;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{here, listed, session, shell, snapshot, there};

/// A shell floating over the window, listed with `id`.
fn floating(id: u128) -> TerminalSession {
    TerminalSession {
        floating: true,
        ..shell(id, "/home/me")
    }
}

/// A window listing one session and the floating shells `ids`, with the
/// session's prompt having the keys.
fn window(ids: &[u128]) -> Scenario {
    let mut listing = listed().session(session("a")).terminal(shell(90, "/r/a"));
    for id in ids {
        listing = listing.terminal(floating(*id));
    }
    Scenario::new()
        .event(snapshot().listed(listing))
        .send(Msg::ShowSession(here(), "a".into()))
        .send(Msg::Focus(Area::Prompt))
}

fn toggle() -> Msg {
    Msg::Window(Action::FloatingToggle)
}

fn press(key: Key) -> Msg {
    Msg::Key {
        key,
        probe: Box::new(Probe {
            pane: true,
            ..Probe::default()
        }),
    }
}

fn starts(run: &Scenario) -> usize {
    run.last_effects()
        .iter()
        .filter(|e| matches!(e, Effect::StartFloating { .. }))
        .count()
}

#[test]
fn the_toggle_key_opens_the_floating_terminal_with_the_keys_and_starts_its_first_shell() {
    let run = window(&[]).send(toggle());
    let floating = &run.state().floating;
    assert!(floating.open && floating.focused);
    assert_eq!(starts(&run), 1);
    assert!(matches!(
        run.last_effects(),
        [Effect::StartFloating { environment }] if *environment == here()
    ));
}

#[test]
fn opening_with_a_shell_already_floating_shows_it_and_starts_no_other() {
    let run = window(&[5]).send(toggle());
    assert_eq!(starts(&run), 0);
    assert_eq!(
        shown(run.state()),
        Some(Tab::Terminal(here(), 5)),
        "the shell the host listed is the panel's tab"
    );
}

#[test]
fn the_same_key_gives_an_open_panel_the_keys_and_closes_a_panel_that_has_them() {
    let run = window(&[5]).send(toggle());
    // The keys moved elsewhere while the panel stayed open.
    let run = run.send(Msg::FloatingBlur);
    assert!(run.state().floating.open && !run.state().floating.focused);
    let run = run.send(toggle());
    assert!(run.state().floating.open && run.state().floating.focused);
    assert_eq!(starts(&run), 0);
    let run = run.send(toggle());
    assert!(!run.state().floating.open && !run.state().floating.focused);
}

#[test]
fn closing_and_opening_again_keeps_the_shell_running_and_asks_for_no_end() {
    let run = window(&[5]).send(toggle()).send(toggle());
    assert!(
        !run.effects()
            .iter()
            .any(|e| matches!(e, Effect::EndShell { .. })),
        "hiding the panel ends nothing"
    );
    let run = run.send(toggle());
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 5)));
    assert_eq!(starts(&run), 0);
}

#[test]
fn opening_and_closing_leave_the_keys_of_the_window_where_they_were() {
    let run = window(&[5]);
    let before = run.state().content.focus;
    let run = run.send(toggle());
    assert_eq!(run.state().content.focus, before);
    let run = run.send(toggle());
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert_eq!(run.state().content.focus, before);
}

#[test]
fn a_shell_that_started_after_the_panel_was_closed_does_not_open_it_or_take_the_keys() {
    let run = window(&[]).send(toggle()).send(toggle());
    let run = run
        .event(snapshot().listed(listed().session(session("a")).terminal(floating(8))))
        .send(Msg::FloatingStarted {
            environment: here(),
            id: 8,
        });
    let floating = &run.state().floating;
    assert!(!floating.open && !floating.focused);
    assert_eq!(run.state().content.focus, Area::Prompt);
}

#[test]
fn keys_go_to_the_shell_but_those_the_window_binds() {
    let run = window(&[5]).send(toggle());
    let run = run.send(press(Key::new("l")));
    assert_eq!(run.last_effects(), [Effect::FloatingKey(Key::new("l"))]);
    let run = run.send(press(Key::new("Escape")));
    assert_eq!(
        run.last_effects(),
        [Effect::FloatingKey(Key::new("Escape"))],
        "a shell takes Escape"
    );
    assert!(run.state().floating.open);
    let toggle_key = Key::new("a").ctrl().alt();
    let run = run.send(press(toggle_key.clone()));
    assert_eq!(
        run.last_effects(),
        [Effect::PerformKey(Action::FloatingToggle, toggle_key)],
        "the toggle key still runs"
    );
}

#[test]
fn text_typed_and_pasted_goes_to_the_shell_while_the_panel_has_the_keys() {
    let run = window(&[5]).send(toggle());
    let run = run.send(Msg::Text {
        text: "ls".into(),
        pasted: false,
    });
    assert_eq!(
        run.last_effects(),
        [Effect::FloatingText {
            text: "ls".into(),
            pasted: false
        }]
    );
    let run = run.send(toggle()).send(Msg::Text {
        text: "ls".into(),
        pasted: false,
    });
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::FloatingText { .. })),
        "a closed panel takes no text"
    );
}

#[test]
fn an_overlay_over_the_window_takes_the_keys_before_the_panel() {
    let run = window(&[5]).send(toggle()).send(Msg::PickerOpen);
    let run = run.send(press(Key::new("x")));
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::FloatingKey(_)))
    );
}

#[test]
fn the_panel_and_its_shells_survive_going_to_another_worktree_or_environment() {
    let run = window(&[5]).send(toggle());
    let run = run.send(Msg::ShowSession(here(), "a".into()));
    assert!(run.state().floating.open);
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 5)));
}

#[test]
fn each_environment_has_its_own_floating_shells() {
    let run = crate::testing::shows_there(window(&[5]), [session("b")]);
    let run = run.send(toggle());
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 5)));
    let run = run
        .send(Msg::ShowSession(there(), "b".into()))
        .send(Msg::FloatingBlur)
        .send(toggle());
    assert_eq!(
        shown(run.state()),
        None,
        "the other environment holds no floating shell yet"
    );
    assert!(
        run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::StartFloating { environment } if *environment == there())),
        "so one is started there: {:?}",
        run.last_effects()
    );
}

#[test]
fn a_new_tab_asks_for_another_shell_and_the_one_that_starts_is_shown() {
    let run = window(&[5])
        .send(toggle())
        .send(Msg::Window(Action::FloatingNew));
    assert_eq!(starts(&run), 1);
    let run = run
        .event(
            snapshot().listed(
                listed()
                    .session(session("a"))
                    .terminal(floating(5))
                    .terminal(floating(6)),
            ),
        )
        .send(Msg::FloatingStarted {
            environment: here(),
            id: 6,
        });
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 6)));
    assert_eq!(tabs(run.state()).len(), 2);
}

#[test]
fn a_shell_is_asked_for_once_while_it_is_starting() {
    let run = window(&[]).send(toggle()).send(toggle()).send(toggle());
    assert_eq!(starts(&run), 0, "the first is still starting");
    let run = run
        .send(Msg::FloatingFailed {
            environment: here(),
        })
        .send(toggle())
        .send(toggle());
    assert_eq!(starts(&run), 1, "a failed one is asked for again");
}

#[test]
fn the_tabs_step_to_the_next_and_the_previous_going_round() {
    let run = window(&[5, 6, 7]).send(toggle());
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 5)));
    let run = run.send(Msg::Window(Action::FloatingNext));
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 6)));
    let run = run
        .send(Msg::Window(Action::FloatingNext))
        .send(Msg::Window(Action::FloatingNext));
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 5)));
    let run = run.send(Msg::Window(Action::FloatingPrev));
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 7)));
}

#[test]
fn closing_a_tab_ends_its_shell_and_shows_the_one_beside_it() {
    let run = window(&[5, 6])
        .send(toggle())
        .send(Msg::Window(Action::FloatingNext));
    let run = run.send(Msg::Window(Action::FloatingCloseTab));
    assert!(matches!(
        run.last_effects(),
        [Effect::EndShell { id: 6, .. }]
    ));
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 5)));
    assert!(run.state().floating.open);
}

#[test]
fn closing_the_last_tab_closes_the_panel() {
    let run = window(&[5]).send(toggle());
    let run = run.send(Msg::Window(Action::FloatingCloseTab));
    assert!(matches!(
        run.last_effects(),
        [Effect::EndShell { id: 5, .. }]
    ));
    assert!(!run.state().floating.open);
    assert_eq!(run.state().content.focus, Area::Prompt);
}

#[test]
fn the_panel_s_own_button_closes_it_without_ending_a_shell() {
    let run = window(&[5]).send(toggle()).send(Msg::FloatingClose);
    assert!(!run.state().floating.open && !run.state().floating.focused);
    assert!(run.last_effects().is_empty());
}

#[test]
fn clicking_where_the_keys_belong_takes_them_from_the_panel_and_leaves_it_open() {
    let run = window(&[5]).send(toggle()).send(Msg::Focus(Area::Prompt));
    assert!(run.state().floating.open && !run.state().floating.focused);
    let run = run.send(Msg::FloatingFocus);
    assert!(run.state().floating.focused);
}

#[test]
fn an_action_of_the_window_that_is_not_the_panel_s_takes_the_keys_to_where_it_acts() {
    let run = window(&[5]).send(toggle()).send(Msg::Perform {
        action: Action::WindowRight,
        key: Some(Key::new("l")),
        probe: Box::default(),
    });
    assert!(!run.state().floating.focused);
    assert!(run.state().floating.open);
}

#[test]
fn a_panel_with_no_keys_takes_no_key_of_the_window() {
    let run = window(&[5]).send(toggle()).send(Msg::FloatingBlur);
    let run = run.send(press(Key::new("l")));
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::FloatingKey(_)))
    );
}

#[test]
fn a_tab_clicked_is_shown_and_the_panel_takes_the_keys() {
    let run = window(&[5, 6])
        .send(toggle())
        .send(Msg::FloatingBlur)
        .send(Msg::FloatingSelect {
            environment: here(),
            id: 6,
        });
    assert_eq!(shown(run.state()), Some(Tab::Terminal(here(), 6)));
    assert!(run.state().floating.focused);
}
