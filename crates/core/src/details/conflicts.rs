//! What the details panel does about files in conflict and the diffs it
//! opens in the user's Neovim.

use tsumugi_model::repository::Change;

use crate::effect::Effect;
use crate::state::ViewState;

/// What an agent is asked in the draft that resolves the conflicts of
/// `files`, while `operation` is in progress.
fn resolving(operation: Option<tsumugi_model::repository::Operation>, files: &[Change]) -> String {
    let list: Vec<String> = files.iter().map(|f| format!("- {}", f.path)).collect();
    let go_on = match operation {
        Some(operation) => format!(
            "Once every file is resolved and staged, continue with `git {} --continue`; do not abort it.",
            operation.name()
        ),
        None => "Once every file is resolved, stage it with `git add`.".into(),
    };
    format!(
        "Resolve the conflicts in this worktree. These files are in conflict:\n\n{}\n\nFor each, read both sides and keep what each intends, remove the conflict markers, and stage the file with `git add`. Do not discard either side wholesale without saying why. {go_on}",
        list.join("\n")
    )
}

/// Open a draft in the worktree shown whose prompt asks an agent to resolve
/// the files in conflict; sending it starts the session. With none in
/// conflict, say so.
pub(crate) fn resolve(state: &mut ViewState) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = crate::content::shown_environment(state);
    let work = state.hosts.work(&environment);
    let files = super::conflicts(&state.details, work);
    if files.is_empty() {
        return vec![Effect::Notice("No file is in conflict".into())];
    }
    let operation = super::branch(&state.details, work).and_then(|b| b.operation);
    let prompt = resolving(operation, &files);
    crate::content::draft_written(state, environment, worktree, prompt)
}

/// Open the diff the cursor is on in the user's Neovim, where it has its
/// Diffview or else a built-in diff.
pub(crate) fn open_in_neovim(state: &mut ViewState) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = crate::content::shown_environment(state);
    let work = state.hosts.work(&environment);
    let Some(spot) = super::selected_spot(&state.details, work) else {
        return vec![Effect::Notice(
            "Select a changed file to open its diff".into(),
        )];
    };
    vec![Effect::StartDiffEditor {
        place: crate::tree::Place::at(&environment, &worktree),
        spot,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::repository::Operation;

    fn conflict(path: &str) -> Change {
        Change {
            status: "UU".into(),
            path: path.into(),
            from: None,
        }
    }

    #[test]
    fn the_ask_names_every_file_and_how_the_operation_goes_on() {
        let text = resolving(
            Some(Operation::Rebase),
            &[conflict("a.rs"), conflict("日本語 b.rs")],
        );
        assert!(text.contains("- a.rs\n- 日本語 b.rs"), "{text}");
        assert!(text.contains("git rebase --continue"), "{text}");
        assert!(!resolving(None, &[conflict("a.rs")]).contains("--continue"));
    }
}
