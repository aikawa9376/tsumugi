//! Writing the name of a branch to make at a commit in the prompt: its first
//! line is the name, and what is sent from it makes the branch without
//! checking it out.

use std::path::PathBuf;

use tsumugi_model::EnvironmentId;
use tsumugi_protocol::GitOp;

use super::send::{Sending, Taken};
use crate::content::{Area, shows_prompt};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::update::step;

/// A branch the prompt writes the name of, to be made at `commit`.
#[derive(Debug, Clone, PartialEq)]
pub struct Branching {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
    /// The full id of the commit the branch is made at.
    pub commit: String,
}

/// The name written: its first line, none when it is empty.
fn written(text: &str) -> Option<String> {
    let line = text.lines().next().unwrap_or_default().trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Give the prompt the keys to write the name of the branch to make at
/// `commit`, whose short id is `short`. Where no prompt is shown there is
/// nowhere to write it.
pub(crate) fn begin(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    commit: String,
    short: &str,
) -> Vec<Effect> {
    if !shows_prompt(state.content.active()) {
        return vec![Effect::Notice(
            "Open a session or a draft to write the branch name in its prompt".into(),
        )];
    }
    state.prompt.committing = None;
    state.prompt.pulling = None;
    state.prompt.basing = None;
    state.prompt.writing = None;
    state.prompt.branching = Some(Branching {
        environment,
        worktree,
        commit,
    });
    let mut effects = step(state, Msg::Focus(Area::Prompt));
    effects.push(Effect::PromptRestore {
        tab: state.content.active().cloned(),
        text: String::new(),
        images: Vec::new(),
    });
    effects.push(Effect::Notice(format!(
        "Write the name of the branch to make at {short}; sending makes it, leaving the prompt gives up"
    )));
    effects
}

/// The prompt's text taken while a branch name is being written: the work
/// host is asked to make the branch, and says why when it cannot. A name
/// that is empty is not asked for.
pub(crate) fn taken(state: &mut ViewState, branching: Branching, taken: Taken) -> Vec<Effect> {
    let Some(name) = written(&taken.text) else {
        state.prompt.sending = None;
        return vec![Effect::Notice("Write the name of the branch first".into())];
    };
    state.prompt.sending = Some(Sending::Handed(taken));
    vec![Effect::Git {
        environment: branching.environment,
        worktree: branching.worktree,
        op: GitOp::CreateBranch {
            name,
            commit: branching.commit,
        },
        prompt: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_the_first_line_and_an_empty_prompt_names_nothing() {
        assert_eq!(written("  topic/x  \nignored"), Some("topic/x".into()));
        assert_eq!(written(""), None);
        assert_eq!(written("\n  \n"), None);
    }
}
