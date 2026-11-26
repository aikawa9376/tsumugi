use super::*;
use crate::scenario::Scenario;
use crate::testing::{here, listed, session, shell, show, snapshot};
use tsumugi_model::{TerminalProgram, Wait};

fn state() -> ViewState {
    Scenario::new()
        .event(snapshot().listed(listed().session(session("a")).terminal(shell(1, "/r/a"))))
        .send(Msg::ShowSession(here(), "a".into()))
        .send(Msg::PromptRuns(true))
        .send(Msg::Focus(Area::Prompt))
        .state()
        .clone()
}

fn probe() -> Probe {
    Probe {
        pane: true,
        ..Probe::default()
    }
}

#[test]
fn conversation_never_sends_reading_keys_to_the_hidden_console_even_during_a_dialog() {
    let mut state = state();
    crate::update::step(
        &mut state,
        Msg::SessionConversation {
            tab: Tab::Session(here(), "a".into()),
            shown: true,
        },
    );
    state.console.dialog = true;
    state.content.focus = Area::Console;
    state.console.direct = true;
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
    assert!(
        !press(&mut state, Key::new("j"), probe())
            .iter()
            .any(|e| matches!(e, Effect::PaneKey(_)))
    );
    state.content.focus = Area::Prompt;
    assert_eq!(input_target(&state, &probe()), InputTarget::Neovim);
}

#[test]
fn input_target_follows_the_prompt_only_while_it_can_receive_keys() {
    let mut state = state();
    assert_eq!(input_target(&state, &probe()), InputTarget::Neovim);
    for mode in [("insert", "INSERT"), ("normal", "NORMAL")] {
        state.prompt.mode = mode;
        assert_eq!(input_target(&state, &probe()), InputTarget::Neovim);
    }
    state.prompt.selected = Some("image.png".into());
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
    state.prompt.selected = None;
    state.console.prompt_runs = false;
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
}

#[test]
fn input_target_leaves_neovim_for_gui_focus_and_every_overlay() {
    let original = state();
    for area in [Area::Sidebar, Area::Details] {
        let mut state = original.clone();
        state.content.focus = area;
        assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
    }
    let mut state = original.clone();
    state.chrome.focused = false;
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
    state = original;
    state.overlays.adding = true;
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
}

#[test]
fn input_target_prioritizes_direct_keys_then_reader_then_agent_answers() {
    let mut state = state();
    state.console.dialog = true;
    assert_eq!(input_target(&state, &probe()), InputTarget::Terminal);
    state.content.focus = Area::Console;
    state.console.reading = true;
    assert_eq!(input_target(&state, &probe()), InputTarget::Terminal);
    let reader = Probe {
        reader: Some(ReaderAt {
            normal: true,
            edge: Edge {
                line: 0,
                lines: 20,
                page: 10,
            },
        }),
        ..probe()
    };
    assert_eq!(input_target(&state, &reader), InputTarget::Neovim);
    state.console.direct = true;
    assert_eq!(input_target(&state, &reader), InputTarget::Terminal);
    state.console.direct = false;
    state.console.send_next = true;
    assert_eq!(input_target(&state, &reader), InputTarget::Terminal);
    assert_eq!(input_target(&state, &Probe::default()), InputTarget::Gui);
}

#[test]
fn input_target_uses_agent_wait_states_in_both_prompt_and_console() {
    for wait in [Wait::Permission, Wait::Input] {
        let state = Scenario::new()
            .event(snapshot().listed(listed().session(session("a").waiting(wait))))
            .send(Msg::ShowSession(here(), "a".into()))
            .send(Msg::PromptRuns(true));
        for area in [Area::Prompt, Area::Console] {
            let mut state = state.state().clone();
            state.content.focus = area;
            state.prompt.selected = Some("image.png".into());
            assert_eq!(input_target(&state, &probe()), InputTarget::Terminal);
        }
    }
}

#[test]
fn input_target_reports_gui_while_a_window_binding_holds_the_next_key() {
    let mut state = state();
    state.console.pending.push(Key::new("q").ctrl());
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
}

#[test]
fn input_target_identifies_file_neovim_from_terminal_metadata_including_fallback() {
    for program in [
        TerminalProgram::Shell,
        TerminalProgram::Neovim,
        TerminalProgram::Command,
    ] {
        let mut terminal = shell(1, "/r/a");
        terminal.program = program;
        let state = Scenario::new()
            .event(snapshot().listed(listed().terminal(terminal)))
            .send(show(Tab::Terminal(here(), 1)))
            .send(Msg::Focus(Area::Console));
        let want = if program == TerminalProgram::Neovim {
            InputTarget::Neovim
        } else {
            InputTarget::Terminal
        };
        assert_eq!(input_target(state.state(), &probe()), want);
        assert_eq!(
            input_target(state.state(), &Probe::default()),
            InputTarget::Gui
        );
    }
}

#[test]
fn input_target_does_not_call_a_live_agent_console_a_visible_neovim() {
    let mut state = state();
    state.content.focus = Area::Console;
    assert_eq!(input_target(&state, &probe()), InputTarget::Gui);
    // A reader can exist while live output is shown without taking keys.
    let probe = Probe {
        reader: Some(ReaderAt {
            normal: true,
            edge: Edge {
                line: 0,
                lines: 1,
                page: 1,
            },
        }),
        ..probe()
    };
    assert_eq!(input_target(&state, &probe), InputTarget::Gui);
}

#[test]
fn a_conversation_s_keys_that_write_in_the_console_write_in_the_prompt() {
    for name in ["i", "a", "I", "A", "o", "O"] {
        let mut state = state();
        crate::update::step(
            &mut state,
            Msg::SessionConversation {
                tab: Tab::Session(here(), "a".into()),
                shown: true,
            },
        );
        state.content.focus = Area::Console;
        let key = Key::new(name);
        let effects = press(&mut state, key.clone(), probe());
        let Some(Effect::PerformKey(action, _)) = effects.first() else {
            panic!("{name}: {effects:?}");
        };
        let effects = crate::update::step(
            &mut state,
            Msg::Perform {
                action: action.clone(),
                key: Some(key),
                probe: Box::new(probe()),
            },
        );
        assert_eq!(state.content.focus, Area::Prompt, "{name}");
        assert!(
            effects.contains(&Effect::PromptKey(Key::new(name))),
            "{name}: {effects:?}"
        );
    }
}

#[test]
fn text_typed_on_a_conversation_is_written_in_the_prompt_as_on_its_console() {
    for pasted in [false, true] {
        let mut state = state();
        crate::update::step(
            &mut state,
            Msg::SessionConversation {
                tab: Tab::Session(here(), "a".into()),
                shown: true,
            },
        );
        state.content.focus = Area::Console;
        let effects = crate::update::step(
            &mut state,
            Msg::Text {
                text: "直して".into(),
                pasted,
            },
        );
        assert_eq!(state.content.focus, Area::Prompt);
        assert!(effects.contains(&Effect::PromptText {
            text: "直して".into(),
            pasted,
        }));
    }
}

#[test]
fn a_number_pressed_on_a_conversation_answers_the_choice_its_session_waits_on() {
    let conversation = |state: tsumugi_model::State| {
        let mut run = Scenario::new()
            .event(snapshot().listed(listed().session(session("a").state(state))))
            .send(Msg::ShowSession(here(), "a".into()))
            .send(Msg::SessionConversation {
                tab: Tab::Session(here(), "a".into()),
                shown: true,
            })
            .state()
            .clone();
        run.content.focus = Area::Console;
        run
    };
    let mut waiting = conversation(tsumugi_model::State::Waiting(Wait::Input));
    assert_eq!(
        press(&mut waiting, Key::new("2"), probe()),
        [Effect::PaneKey(Key::new("2"))]
    );
    assert!(
        !press(&mut waiting, Key::new("j"), probe())
            .iter()
            .any(|e| matches!(e, Effect::PaneKey(_)))
    );
    let mut working = conversation(tsumugi_model::State::Working);
    assert!(
        !press(&mut working, Key::new("2"), probe())
            .iter()
            .any(|e| matches!(e, Effect::PaneKey(_)))
    );
}
