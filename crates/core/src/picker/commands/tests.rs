use tsumugi_model::{Key, SavedCommand};
use tsumugi_protocol::{Change, TreeChange};

use super::*;
use crate::content::Area;
use crate::msg::Msg;
use crate::picker::Picking;
use crate::scenario::Scenario;
use crate::testing::{here, listing, session, shows_there, there};

fn saved(id: &str, project: &str, label: &str, command: &str) -> SavedCommand {
    SavedCommand {
        id: id.into(),
        project: project.into(),
        label: label.into(),
        command: command.into(),
    }
}

fn keeping(commands: Vec<SavedCommand>) -> Change {
    Change::Tree(TreeChange::SavedCommands { commands })
}

/// A window showing the worktrees `/r/a` and `/r/b` of the repository `/r`,
/// in the first of which the commands are kept.
fn window() -> Scenario {
    Scenario::new()
        .event(listing([session("a"), session("b")]))
        .tell(keeping(vec![
            saved("1", "/r", "Checks", "just ci"),
            saved(
                "2",
                "/r",
                "Server",
                "cd 'web app'\nnpm run dev -- --port 4000",
            ),
            saved("3", "/elsewhere", "Other project", "make"),
        ]))
}

/// The window showing the session `id`, whose worktree the list opens on.
fn showing(run: Scenario, id: &str) -> Scenario {
    run.send(Msg::ShowSession(here(), id.into()))
}

fn key(name: &str, ctrl: bool) -> Msg {
    Msg::PickerKey(Key {
        ctrl,
        ..Key::new(name)
    })
}

fn open(run: Scenario) -> Scenario {
    run.send(Msg::CommandsOpen)
}

fn commands(run: &Scenario) -> &Commands {
    match &run.state().picker.open {
        Some(Picking::Commands(c)) => c,
        other => panic!("the list is not open: {other:?}"),
    }
}

fn labels(run: &Scenario) -> Vec<String> {
    rows(commands(run), &run.state().hosts)
        .into_iter()
        .map(|c| c.label.clone())
        .collect()
}

fn did(run: &Scenario) -> Vec<Do> {
    run.last_effects()
        .iter()
        .filter_map(|e| match e {
            Effect::SavedCommand(d) => Some(d.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_list_holds_the_commands_of_the_project_whichever_worktree_it_opens_on() {
    for worktree in ["a", "b"] {
        let run = open(showing(window(), worktree));
        assert_eq!(labels(&run), ["Checks", "Server"], "{worktree}");
    }
}

#[test]
fn typing_narrows_the_list_by_label_and_by_command() {
    let typed =
        |text: &str| labels(&open(showing(window(), "a")).send(Msg::PickerQuery(text.into())));
    assert_eq!(typed(""), ["Checks", "Server"]);
    assert_eq!(typed("ci"), ["Checks"]);
    assert_eq!(typed("npm port"), ["Server"]);
    assert!(typed("make").is_empty());
}

#[test]
fn opening_the_list_and_moving_in_it_runs_nothing() {
    let run = open(showing(window(), "a"))
        .send(key("ArrowDown", false))
        .send(key("ArrowUp", false));
    assert!(
        run.effects()
            .iter()
            .all(|e| !matches!(e, Effect::SavedCommand(_)))
    );
}

#[test]
fn a_command_chosen_is_shown_with_its_directory_and_machine_before_it_runs() {
    let run = open(showing(window(), "b"))
        .send(key("ArrowDown", false))
        .send(key("Enter", false));
    assert!(did(&run).is_empty());
    let Stage::Run(chosen) = &commands(&run).stage else {
        panic!("{:?}", commands(&run).stage);
    };
    let shown = preview(commands(&run), chosen, &run.state().hosts);
    assert_eq!(shown.label, "Server");
    assert_eq!(shown.command, "cd 'web app'\nnpm run dev -- --port 4000");
    assert_eq!(shown.directory, PathBuf::from("/r/b"));
    assert_eq!(shown.machine, run.state().hosts.name(&here()));
}

#[test]
fn a_command_saved_from_one_worktree_runs_in_the_one_selected_when_it_is_chosen() {
    let run = open(showing(window(), "b"))
        .send(key("Enter", false))
        .send(key("Enter", false));
    assert_eq!(
        did(&run),
        [Do::Run {
            place: Place::at(&here(), "/r/b"),
            label: "Checks".into(),
            script: "just ci".into(),
            behind: false,
        }]
    );
    assert!(run.state().picker.open.is_none());
}

#[test]
fn a_multi_line_command_is_run_as_it_was_written() {
    let run = open(showing(window(), "a"))
        .send(Msg::PickerQuery("server".into()))
        .send(Msg::PickerChoose(0))
        .send(key("Enter", false));
    let done = did(&run);
    let [Do::Run { script, .. }] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(script, "cd 'web app'\nnpm run dev -- --port 4000");
}

#[test]
fn a_worktree_with_a_space_and_non_ascii_characters_is_where_it_runs() {
    let run = Scenario::new()
        .event(listing([session("w").cwd("/r/作業 ディレクトリ")]))
        .tell(keeping(vec![saved("1", "/r", "Checks", "just ci")]))
        .send(Msg::ShowSession(here(), "w".into()))
        .send(Msg::CommandsOpen)
        .send(key("Enter", false))
        .send(key("Enter", false));
    let done = did(&run);
    let [Do::Run { place, .. }] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(place.worktree, PathBuf::from("/r/作業 ディレクトリ"));
}

#[test]
fn going_back_from_the_confirmation_runs_nothing_and_keeps_the_list_open() {
    let run = open(showing(window(), "a"))
        .send(key("Enter", false))
        .send(key("Escape", false));
    assert!(did(&run).is_empty());
    assert_eq!(commands(&run).stage, Stage::List);
}

#[test]
fn closing_the_list_runs_nothing() {
    for close in [key("Escape", false), key("c", true), Msg::PickerClose] {
        let run = open(showing(window(), "a")).send(close);
        assert!(run.state().picker.open.is_none());
        assert!(did(&run).is_empty());
    }
}

#[test]
fn a_command_runs_where_its_environment_is_whichever_machine_that_is() {
    let run = shows_there(Scenario::new(), [session("a")])
        .tell_from(
            there(),
            keeping(vec![saved("1", "/r", "Checks", "just ci")]),
        )
        .send(Msg::ShowSession(there(), "a".into()))
        .send(Msg::CommandsOpen);
    assert_eq!(commands(&run).place.environment, there());
    let run = run.send(key("Enter", false));
    let Stage::Run(chosen) = &commands(&run).stage else {
        panic!("{:?}", commands(&run).stage);
    };
    assert_eq!(
        preview(commands(&run), chosen, &run.state().hosts).machine,
        "There"
    );
    let run = run.send(key("Enter", false));
    let done = did(&run);
    let [Do::Run { place, .. }] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(place.environment, there());
}

#[test]
fn another_environment_s_commands_are_not_listed() {
    let run = shows_there(Scenario::new(), [session("a")])
        .tell(keeping(vec![saved("1", "/r", "Here only", "make")]))
        .send(Msg::ShowSession(there(), "a".into()))
        .send(Msg::CommandsOpen);
    assert!(labels(&run).is_empty());
}

#[test]
fn the_keys_stay_where_they_were_and_a_prompt_being_written_is_not_covered() {
    for (focus, behind) in [
        (Area::Prompt, true),
        (Area::Console, false),
        (Area::Sidebar, false),
    ] {
        let run = showing(window(), "a").send(Msg::Focus(focus));
        let run = open(run)
            .send(key("Enter", false))
            .send(key("Enter", false));
        assert_eq!(run.state().content.focus, focus);
        let done = did(&run);
        let [Do::Run { behind: got, .. }] = done.as_slice() else {
            panic!("{done:?}");
        };
        assert_eq!(*got, behind, "{focus:?}");
    }
}

#[test]
fn a_command_is_deleted_only_once_confirmed() {
    let run = open(showing(window(), "a")).send(key("d", true));
    assert!(did(&run).is_empty());
    let run = run.send(key("Escape", false));
    assert!(did(&run).is_empty());
    assert_eq!(commands(&run).stage, Stage::List);
    let run = run.send(key("d", true)).send(key("Enter", false));
    assert_eq!(
        did(&run),
        [Do::Delete {
            environment: here(),
            id: "1".into()
        }]
    );
    assert_eq!(commands(&run).stage, Stage::List);
}

#[test]
fn editing_a_command_starts_from_what_it_says_and_keeps_its_identity() {
    let run = open(showing(window(), "b")).send(key("e", true));
    let Stage::Edit(editing) = &commands(&run).stage else {
        panic!("{:?}", commands(&run).stage);
    };
    assert_eq!(editing.id.as_deref(), Some("1"));
    assert_eq!((&*editing.label, &*editing.command), ("Checks", "just ci"));
    let run = run
        .send(Msg::CommandsTyped(Typed::Label("Full checks".into())))
        .send(Msg::CommandsTyped(Typed::Command(
            "just ci\njust doc".into(),
        )))
        .send(key("s", true));
    assert_eq!(
        did(&run),
        [Do::Save {
            environment: here(),
            command: NewSavedCommand {
                id: "1".into(),
                dir: "/r/b".into(),
                label: "Full checks".into(),
                command: "just ci\njust doc".into(),
            }
        }]
    );
    assert_eq!(commands(&run).stage, Stage::List);
}

#[test]
fn a_new_command_is_written_label_first_and_saved_without_running() {
    let run = open(showing(window(), "a"))
        .send(key("a", true))
        .send(Msg::CommandsTyped(Typed::Label("Setup".into())))
        .send(key("Enter", false));
    let Stage::Edit(editing) = &commands(&run).stage else {
        panic!("{:?}", commands(&run).stage);
    };
    assert_eq!(editing.field, Field::Command);
    let run = run
        .send(Msg::CommandsTyped(Typed::Command("./setup.sh".into())))
        .send(key("Enter", true));
    let done = did(&run);
    let [Do::Save { command, .. }] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(
        (&*command.label, &*command.command),
        ("Setup", "./setup.sh")
    );
    assert!(command.id.len() > 8);
    assert!(!did(&run).iter().any(|d| matches!(d, Do::Run { .. })));
}

#[test]
fn a_command_that_cannot_be_kept_says_why_and_stays_open_for_writing() {
    let run = open(showing(window(), "a"))
        .send(key("a", true))
        .send(Msg::CommandsTyped(Typed::Command("make".into())))
        .send(key("s", true));
    assert!(did(&run).is_empty());
    let Stage::Edit(editing) = &commands(&run).stage else {
        panic!("{:?}", commands(&run).stage);
    };
    assert!(editing.error.is_some());
    let run = run.send(Msg::CommandsTyped(Typed::Label("Build".into())));
    let Stage::Edit(editing) = &commands(&run).stage else {
        panic!();
    };
    assert_eq!(editing.error, None);
}

#[test]
fn escape_leaves_the_command_being_written_without_keeping_it() {
    let run = open(showing(window(), "a"))
        .send(key("a", true))
        .send(Msg::CommandsTyped(Typed::Label("Setup".into())))
        .send(key("Escape", false));
    assert!(did(&run).is_empty());
    assert_eq!(commands(&run).stage, Stage::List);
}

#[test]
fn editing_or_deleting_with_nothing_listed_does_nothing() {
    let run = open(showing(Scenario::new().event(listing([session("a")])), "a"))
        .send(key("e", true))
        .send(key("d", true))
        .send(key("Enter", false));
    assert_eq!(commands(&run).stage, Stage::List);
    assert!(did(&run).is_empty());
}
