//! Writing the base a branch is compared with in the prompt: its first line
//! is the ref, and an empty prompt gives the choice up for the base Tsumugi
//! finds.

use std::path::PathBuf;

use tsumugi_model::EnvironmentId;
use tsumugi_protocol::GitOp;

use super::send::{Sending, Taken};
use crate::content::{Area, shows_prompt};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::update::step;

/// A base the prompt writes the ref of.
#[derive(Debug, Clone, PartialEq)]
pub struct Basing {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
}

/// The ref written: its first line, none when it is empty.
fn written(text: &str) -> Option<String> {
    let line = text.lines().next().unwrap_or_default().trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Give the prompt the keys to write the base `worktree`'s branch is compared
/// with, starting from the one chosen or else the one found. Where no prompt
/// is shown there is nowhere to write it.
pub(crate) fn begin(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
) -> Vec<Effect> {
    if !shows_prompt(state.content.active()) {
        return vec![Effect::Notice(
            "Open a session or a draft to write the base in its prompt".into(),
        )];
    }
    let work = state.hosts.work(&environment);
    let current = crate::details::branch(&state.details, work)
        .and_then(|branch| branch.chosen.or(branch.comparison.base))
        .unwrap_or_default();
    state.prompt.committing = None;
    state.prompt.pulling = None;
    state.prompt.writing = None;
    state.prompt.branching = None;
    state.prompt.basing = Some(Basing {
        environment,
        worktree,
    });
    let mut effects = step(state, Msg::Focus(Area::Prompt));
    effects.push(Effect::PromptRestore {
        tab: state.content.active().cloned(),
        text: current,
        images: Vec::new(),
    });
    effects.push(Effect::Notice(
        "Write the base ref; sending chooses it, empty goes back to the one found, leaving the prompt gives up".into(),
    ));
    effects
}

/// The prompt's text taken while a base is being written: the work host is
/// asked to choose it, and says why when it cannot.
pub(crate) fn taken(state: &mut ViewState, basing: Basing, taken: Taken) -> Vec<Effect> {
    let base = written(&taken.text);
    state.prompt.sending = Some(Sending::Handed(taken));
    vec![Effect::Git {
        environment: basing.environment,
        worktree: basing.worktree,
        op: GitOp::ChooseBase { base },
        prompt: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_is_the_first_line_and_an_empty_prompt_gives_the_choice_up() {
        assert_eq!(
            written("  origin/develop  \nignored"),
            Some("origin/develop".into())
        );
        assert_eq!(written(""), None);
        assert_eq!(written("\n  \n"), None);
    }
}
