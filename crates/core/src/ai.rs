//! The four AI actions of the Changes list, which ask an agent as the
//! recipe of the repository says. Two write text into the prompt for the user
//! to read, edit and send: a commit message and a pull request. Two put a new
//! session's prompt in a draft in the worktree: fixing what a hook refused
//! and resolving the files in conflict. Nothing here commits, pushes or
//! sends: the user does, once they have read what was written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tsumugi_model::repository::Change;
use tsumugi_model::{AiAction, CommitFailure, EnvironmentId};

use crate::content::{DraftOptions, shows_prompt};
use crate::effect::Effect;
use crate::settings::Resolved;
use crate::state::ViewState;
use crate::tree;

/// What the AI actions remember between one press and the next.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiView {
    /// The commit last refused in each worktree, kept until one succeeds
    /// there: what a hook said, which Fix with AI is made from.
    failures: HashMap<(EnvironmentId, PathBuf), CommitFailure>,
    /// The text being written, so that one press is not asked twice.
    writing: Option<(EnvironmentId, PathBuf, AiAction)>,
}

impl AiView {
    pub fn failure(&self, environment: &EnvironmentId, worktree: &Path) -> Option<&CommitFailure> {
        self.failures
            .get(&(environment.clone(), worktree.to_path_buf()))
    }

    pub fn is_writing(&self) -> bool {
        self.writing.is_some()
    }
}

/// The recipe of `action` for the repository of `worktree`, or why it cannot
/// be used: arguments that do not split, or a profile that is not defined.
fn recipe(
    state: &ViewState,
    environment: &EnvironmentId,
    worktree: &Path,
    action: AiAction,
) -> Result<Resolved, String> {
    let work = state.hosts.work(environment);
    let primary = tree::listed(work)
        .workspace_of(worktree)
        .map_or_else(|| worktree.to_path_buf(), |w| w.root.clone());
    let settings = &state.config.settings;
    let resolved = settings.ai_actions.resolve(action, &primary)?;
    if let Some(id) = &resolved.profile
        && settings.find_profile(id).is_none()
    {
        return Err(format!(
            "The {} recipe names the profile `{id}`, which is not defined",
            action.key()
        ));
    }
    Ok(resolved)
}

/// What the user is told is being asked, by the agent's profile.
fn asking(state: &ViewState, resolved: &Resolved) -> String {
    let settings = &state.config.settings;
    let id = resolved
        .profile
        .clone()
        .unwrap_or_else(|| settings.profile.clone());
    settings
        .find_profile(&id)
        .map_or_else(|| id.to_string(), |p| p.label().to_string())
}

/// The files of the working copy as last read, staged ones and the rest.
fn working_files(state: &ViewState, environment: &EnvironmentId) -> Vec<Change> {
    let work = state.hosts.work(environment);
    match crate::details::working_copy(&state.details, work).map(|v| v.listing) {
        Some(crate::details::Listing::Files(files)) => files,
        _ => Vec::new(),
    }
}

/// The user asked for a commit message or a pull request to be written by
/// the recipe's agent: run it once, in the background, for text that is put in
/// the prompt, where it waits to be read.
pub(crate) fn generate(state: &mut ViewState, action: AiAction) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = state.details.shown.environment.clone();
    let what = match action {
        AiAction::CommitMessage => "commit message",
        AiAction::PullRequest => "pull request",
        _ => return Vec::new(),
    };
    if !shows_prompt(state.content.active()) {
        return vec![Effect::Notice(format!(
            "Open a session or a draft to write the {what} in its prompt"
        ))];
    }
    if state.ai.is_writing() {
        return vec![Effect::Notice(
            "An agent is still writing; wait for it".into(),
        )];
    }
    if action == AiAction::CommitMessage
        && !working_files(state, &environment)
            .iter()
            .any(Change::is_staged)
    {
        return vec![Effect::Notice(
            "Nothing is staged to write a commit message for".into(),
        )];
    }
    let resolved = match recipe(state, &environment, &worktree, action) {
        Ok(resolved) => resolved,
        Err(why) => return vec![Effect::Notice(why)],
    };
    let who = asking(state, &resolved);
    state.ai.writing = Some((environment.clone(), worktree.clone(), action));
    vec![
        Effect::Notice(format!(
            "Asking {who} to write the {what}; it is put in the prompt for you to read, nothing is committed or sent"
        )),
        Effect::GenerateText {
            environment,
            worktree,
            action,
            profile: resolved.profile,
            args: resolved.args,
            template: resolved.template,
        },
    ]
}

/// The work host answered with the text, or why there is none: put it in the
/// prompt as the message or pull request to read and send, or say why not.
pub(crate) fn generated(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    action: AiAction,
    result: Result<String, String>,
) -> Vec<Effect> {
    let asked = (environment.clone(), worktree.clone(), action);
    if state.ai.writing.as_ref() == Some(&asked) {
        state.ai.writing = None;
    }
    let text = match result {
        Ok(text) => text,
        Err(why) => {
            return vec![Effect::Notice(format!(
                "Could not write the {}: {why}",
                match action {
                    AiAction::CommitMessage => "commit message",
                    _ => "pull request",
                }
            ))];
        }
    };
    match action {
        AiAction::CommitMessage => {
            let mut effects =
                crate::prompt::begin_commit_from(state, environment, worktree, false, Some(text));
            effects.push(Effect::Notice(
                "Wrote the commit message in the prompt; read it, edit it and send it to commit"
                    .into(),
            ));
            effects
        }
        AiAction::PullRequest => {
            crate::prompt::pull_request::begin_from(state, environment, worktree, Some(text))
        }
        _ => Vec::new(),
    }
}

/// A commit was refused with `output` for the message `message`, or made.
pub(crate) fn commit_refused(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    message: String,
    output: String,
) -> Vec<Effect> {
    state
        .ai
        .failures
        .insert((environment, worktree), CommitFailure { message, output });
    Vec::new()
}

pub(crate) fn commit_made(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
) -> Vec<Effect> {
    state.ai.failures.remove(&(environment, worktree));
    Vec::new()
}

/// Open a draft in the worktree shown whose prompt asks the recipe's agent
/// to fix what refused the last commit. Sending it starts the session.
pub(crate) fn fix_commit_failure(state: &mut ViewState) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = state.details.shown.environment.clone();
    let Some(failure) = state.ai.failure(&environment, &worktree).cloned() else {
        return vec![Effect::Notice(
            "No commit was refused in this worktree; commit first".into(),
        )];
    };
    let resolved = match recipe(state, &environment, &worktree, AiAction::FixCommitFailure) {
        Ok(resolved) => resolved,
        Err(why) => return vec![Effect::Notice(why)],
    };
    let staged: Vec<Change> = working_files(state, &environment)
        .into_iter()
        .filter(Change::is_staged)
        .collect();
    let base = tsumugi_model::fix_commit_failure_prompt(
        &worktree.display().to_string(),
        &failure,
        &staged,
    );
    let prompt = tsumugi_model::session_request(&resolved.template, &base);
    session_draft(state, environment, worktree, resolved, prompt)
}

/// Open a draft in the worktree shown whose prompt asks the recipe's agent to
/// resolve the files in conflict; sending it starts the session. With none in
/// conflict, say so.
pub(crate) fn resolve_conflicts(state: &mut ViewState) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let environment = state.details.shown.environment.clone();
    let work = state.hosts.work(&environment);
    let files = crate::details::conflicts(&state.details, work);
    if files.is_empty() {
        return vec![Effect::Notice("No file is in conflict".into())];
    }
    let operation = crate::details::branch(&state.details, work).and_then(|b| b.operation);
    let resolved = match recipe(state, &environment, &worktree, AiAction::ResolveConflicts) {
        Ok(resolved) => resolved,
        Err(why) => return vec![Effect::Notice(why)],
    };
    let base =
        tsumugi_model::resolve_conflicts_prompt(&worktree.display().to_string(), operation, &files);
    let prompt = tsumugi_model::session_request(&resolved.template, &base);
    session_draft(state, environment, worktree, resolved, prompt)
}

/// A new session's draft in `worktree`, started from the recipe's profile,
/// with `prompt` written in it for the user to read and send.
fn session_draft(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    resolved: Resolved,
    prompt: String,
) -> Vec<Effect> {
    if resolved.profile.is_some() {
        state.content.drafts.next = Some(DraftOptions {
            profile: resolved.profile,
            ..DraftOptions::default()
        });
    }
    crate::content::draft_written(state, environment, worktree, prompt)
}
