//! The list of the commands a project keeps to run again: which worktree a
//! chosen one runs in, and what the keys do to it, step by step: choose and
//! confirm to run, add, edit and delete.

use std::path::PathBuf;

use tsumugi_model::{NewSavedCommand, SavedCommand, SessionId};

use super::close;
use super::list::ListState;
use crate::actions::Action;
use crate::effect::Effect;
use crate::host::Hosts;
use crate::keymap::Step;
use crate::state::ViewState;
use crate::tree::Place;

#[cfg(test)]
mod tests;

/// The list open, over the project holding `place`.
#[derive(Debug, Clone, PartialEq)]
pub struct Commands {
    /// The worktree a command chosen runs in, and the environment whose
    /// work host keeps the commands.
    pub place: Place,
    pub list: ListState,
    pub stage: Stage,
}

/// What the list is doing now.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// Searching for a command.
    List,
    /// Showing what choosing `0` would run, for the user to confirm.
    Run(SavedCommand),
    /// Asking whether to delete `0`.
    Delete(SavedCommand),
    /// Writing a command, new or kept.
    Edit(Editing),
}

/// A command being written.
#[derive(Debug, Clone, PartialEq)]
pub struct Editing {
    /// The command it replaces; none for a new one.
    pub id: Option<String>,
    pub label: String,
    pub command: String,
    pub field: Field,
    /// Why it was not kept, as last asked to.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Label,
    Command,
}

/// What is typed into the command being written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    Label(String),
    Command(String),
}

/// What is asked of the world about a saved command.
#[derive(Debug, Clone, PartialEq)]
pub enum Do {
    /// Run `script` in a command terminal in `place`, named `label`.
    Run {
        place: Place,
        label: String,
        script: String,
        /// Whether its tab opens behind the one shown rather than being
        /// shown, because the user is writing in the prompt.
        behind: bool,
    },
    /// Keep `command`, in the project of the environment's host.
    Save {
        environment: tsumugi_model::EnvironmentId,
        command: NewSavedCommand,
    },
    /// Keep the command `id` no more.
    Delete {
        environment: tsumugi_model::EnvironmentId,
        id: String,
    },
}

/// What a command will do once confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub label: String,
    pub command: String,
    pub directory: PathBuf,
    /// The machine it runs on, as the window names the environment.
    pub machine: String,
}

/// The root of the project holding the worktree of `place`: its
/// workspace's, or else the directory itself.
pub fn project(hosts: &Hosts, place: &Place) -> PathBuf {
    hosts
        .work(&place.environment)
        .and_then(|work| work.tree.tree().workspace_of(&place.worktree))
        .map_or_else(
            || place.worktree.clone(),
            |workspace| workspace.root.clone(),
        )
}

/// The commands the project keeps that what is typed answers to, in the
/// order they were kept.
pub fn rows<'a>(commands: &Commands, hosts: &'a Hosts) -> Vec<&'a SavedCommand> {
    let project = project(hosts, &commands.place);
    hosts
        .work(&commands.place.environment)
        .map(|work| {
            work.tree
                .saved_commands
                .iter()
                .filter(|c| c.project == project && c.matches(&commands.list.query))
                .collect()
        })
        .unwrap_or_default()
}

/// What running `command` from the list would do.
pub fn preview(commands: &Commands, command: &SavedCommand, hosts: &Hosts) -> Preview {
    Preview {
        label: command.label.clone(),
        command: command.command.clone(),
        directory: commands.place.worktree.clone(),
        machine: hosts.name(&commands.place.environment),
    }
}

/// Open the list on the worktree the keys are asking about.
pub(crate) fn open(state: &mut ViewState) -> Vec<Effect> {
    state.overlays.menu = None;
    let place = crate::local_issues::chosen_place(state);
    state.picker.open = Some(super::Picking::Commands(Commands {
        place,
        list: ListState::default(),
        stage: Stage::List,
    }));
    Vec::new()
}

/// The text typed into the command being written.
pub(crate) fn type_into(state: &mut ViewState, typed: Typed) -> Vec<Effect> {
    if let Some(Stage::Edit(editing)) = open_stage(state) {
        match typed {
            Typed::Label(text) => editing.label = text,
            Typed::Command(text) => editing.command = text,
        }
        editing.error = None;
    }
    Vec::new()
}

/// The row `at` chosen: what it will run is shown, to confirm.
pub(super) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let Some(super::Picking::Commands(commands)) = &state.picker.open else {
        return Vec::new();
    };
    if commands.stage != Stage::List {
        return Vec::new();
    }
    let chosen = rows(commands, &state.hosts).get(at).copied().cloned();
    if let (Some(chosen), Some(commands)) = (chosen, commands_mut(state)) {
        commands.list.cursor = at;
        commands.stage = Stage::Run(chosen);
    }
    Vec::new()
}

/// A key the query field or the fields being written in do not type with.
pub(super) fn press(state: &mut ViewState, key: &tsumugi_model::Key) -> Vec<Effect> {
    let action = match state
        .chrome
        .keymap
        .resolve(&[crate::keymap::Context::Overlay], &[], key)
    {
        Step::Run(action) => Some(action),
        Step::Wait | Step::Nothing => None,
    };
    let ctrl = key.ctrl && !key.alt && !key.meta && !key.shift;
    let Some(super::Picking::Commands(commands)) = &state.picker.open else {
        return Vec::new();
    };
    match commands.stage.clone() {
        Stage::List => listing(state, action, key, ctrl),
        Stage::Run(command) => match action {
            Some(Action::OverlayChoose) => {
                let place = commands.place.clone();
                let behind = state.content.focus == crate::content::Area::Prompt;
                close(&mut state.picker);
                vec![Effect::SavedCommand(Do::Run {
                    place,
                    label: command.label,
                    script: command.command,
                    behind,
                })]
            }
            Some(Action::OverlayClose) => back(state),
            _ => Vec::new(),
        },
        Stage::Delete(command) => match action {
            Some(Action::OverlayChoose) => {
                let environment = commands.place.environment.clone();
                back(state);
                vec![Effect::SavedCommand(Do::Delete {
                    environment,
                    id: command.id,
                })]
            }
            Some(Action::OverlayClose) => back(state),
            _ => Vec::new(),
        },
        Stage::Edit(_) => editing(state, key, ctrl),
    }
}

fn listing(
    state: &mut ViewState,
    action: Option<Action>,
    key: &tsumugi_model::Key,
    ctrl: bool,
) -> Vec<Effect> {
    let Some(super::Picking::Commands(commands)) = &state.picker.open else {
        return Vec::new();
    };
    let rows_ = rows(commands, &state.hosts);
    let len = rows_.len();
    let under = rows_
        .get(commands.list.cursor.min(len.saturating_sub(1)))
        .copied()
        .cloned();
    let at = commands.list.cursor;
    match (action, ctrl, key.key.as_str()) {
        (Some(Action::OverlayClose), ..) => return close(&mut state.picker),
        (Some(Action::OverlayChoose), ..) => return choose(state, at.min(len.saturating_sub(1))),
        (Some(Action::OverlayDown), ..) => step(state, 1, len),
        (Some(Action::OverlayUp), ..) => step(state, -1, len),
        (_, true, "a") => {
            if let Some(commands) = commands_mut(state) {
                commands.stage = Stage::Edit(Editing {
                    id: None,
                    label: String::new(),
                    command: String::new(),
                    field: Field::Label,
                    error: None,
                });
            }
        }
        (_, true, "e") => {
            if let (Some(command), Some(commands)) = (under, commands_mut(state)) {
                commands.stage = Stage::Edit(Editing {
                    id: Some(command.id),
                    label: command.label,
                    command: command.command,
                    field: Field::Label,
                    error: None,
                });
            }
        }
        (_, true, "d") => {
            if let (Some(command), Some(commands)) = (under, commands_mut(state)) {
                commands.stage = Stage::Delete(command);
            }
        }
        _ => {}
    }
    Vec::new()
}

fn editing(state: &mut ViewState, key: &tsumugi_model::Key, ctrl: bool) -> Vec<Effect> {
    let Some(super::Picking::Commands(commands)) = &mut state.picker.open else {
        return Vec::new();
    };
    let (place, Stage::Edit(editing)) = (commands.place.clone(), &mut commands.stage) else {
        return Vec::new();
    };
    match (key.key.as_str(), ctrl, key.shift) {
        ("Escape", ..) => return back(state),
        ("c", true, _) => return back(state),
        ("Enter" | "Tab", false, false) if editing.field == Field::Label => {
            editing.field = Field::Command;
        }
        ("Tab", false, true) if editing.field == Field::Command => editing.field = Field::Label,
        ("s", true, _) | ("Enter", true, _) => {
            let new = NewSavedCommand {
                id: editing
                    .id
                    .clone()
                    .unwrap_or_else(|| SessionId::new().to_string()),
                dir: place.worktree.clone(),
                label: editing.label.clone(),
                command: editing.command.clone(),
            };
            if let Some(refusal) = tsumugi_model::refuse_command(&new) {
                editing.error = Some(refusal.to_owned());
                return Vec::new();
            }
            back(state);
            return vec![Effect::SavedCommand(Do::Save {
                environment: place.environment,
                command: new,
            })];
        }
        _ => {}
    }
    Vec::new()
}

/// Step back from confirming, deleting or writing to searching.
fn back(state: &mut ViewState) -> Vec<Effect> {
    if let Some(commands) = commands_mut(state) {
        commands.stage = Stage::List;
    }
    Vec::new()
}

fn step(state: &mut ViewState, delta: isize, len: usize) {
    if let Some(commands) = commands_mut(state) {
        commands.list.step(delta, len);
    }
}

pub(super) fn commands_mut(state: &mut ViewState) -> Option<&mut Commands> {
    match &mut state.picker.open {
        Some(super::Picking::Commands(commands)) => Some(commands),
        _ => None,
    }
}

fn open_stage(state: &mut ViewState) -> Option<&mut Stage> {
    commands_mut(state).map(|c| &mut c.stage)
}
