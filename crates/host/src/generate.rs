//! Running an agent once for the text of a commit message or a pull request:
//! the prompt is made from what the worktree holds and the recipe's
//! template, the agent answers with text, and the window puts it in the
//! prompt for the user to edit. Nothing is committed, pushed or sent here.

use std::path::{Path, PathBuf};

use tsumugi_model::profile::ProfileId;
use tsumugi_model::{AiAction, answer_text};
use tsumugi_protocol::Answer;

use crate::{programs, settings};

/// The prompt `template` makes for `action` from what the worktree at `path`
/// holds. Only a one-shot action has one.
pub(crate) async fn prompt(
    path: &Path,
    action: AiAction,
    template: &str,
) -> Result<String, String> {
    match action {
        AiAction::CommitMessage => {
            let context = tsumugi_vcs::generate::commit_context(path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(tsumugi_model::commit_message_request(template, &context))
        }
        AiAction::PullRequest => {
            let context = tsumugi_vcs::generate::pull_context(path)
                .await
                .map_err(|e| e.to_string())?;
            Ok(tsumugi_model::pull_request_request(template, &context))
        }
        AiAction::FixCommitFailure | AiAction::ResolveConflicts => Err(format!(
            "{} starts a session; it is not run once for text",
            action.key()
        )),
    }
}

/// Run the agent of `profile` once in `worktree` and answer with its text.
pub(crate) async fn generate(
    worktree: PathBuf,
    action: AiAction,
    profile: Option<ProfileId>,
    args: Vec<String>,
    template: String,
) -> Result<Answer, String> {
    let prompt = prompt(&worktree, action, &template).await?;
    let settings = settings::current();
    let id = profile.unwrap_or_else(|| settings.profile.clone());
    let context = programs::for_profile(&settings, &id)?;
    let text = tsumugi_agents::oneshot::generate(&context, &worktree, &args, &prompt).await?;
    let text = answer_text(&text);
    if text.is_empty() {
        return Err("The agent answered nothing".into());
    }
    Ok(Answer::Told { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}: {out:?}");
        };
        run(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.path().join("a.rs"), "fn old() {}\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "initial"]);
        std::fs::write(dir.path().join("a.rs"), "fn staged() {}\n").unwrap();
        run(&["add", "a.rs"]);
        dir
    }

    #[tokio::test]
    async fn a_commit_message_prompt_is_the_template_filled_with_the_staged_changes() {
        let dir = repo();
        let text = prompt(
            dir.path(),
            AiAction::CommitMessage,
            "Branch {branch}:\n{stagedPatch}",
        )
        .await
        .unwrap();
        assert!(text.starts_with("Branch main:\n"), "{text}");
        assert!(text.contains("+fn staged"));
        let default = prompt(dir.path(), AiAction::CommitMessage, "{basePrompt}")
            .await
            .unwrap();
        assert!(default.contains("Do not commit, push"), "{default}");
    }

    #[tokio::test]
    async fn a_session_action_is_not_run_once_and_nothing_staged_is_said() {
        let dir = repo();
        let wrong = prompt(dir.path(), AiAction::ResolveConflicts, "{basePrompt}")
            .await
            .unwrap_err();
        assert!(wrong.contains("starts a session"), "{wrong}");
        let clean = tempfile::tempdir().unwrap();
        let wrong = prompt(clean.path(), AiAction::CommitMessage, "{basePrompt}")
            .await
            .unwrap_err();
        assert!(!wrong.is_empty());
    }
}
