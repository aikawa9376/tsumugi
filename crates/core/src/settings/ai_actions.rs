//! The recipes of the AI actions of the Changes list: which profile's agent
//! an action asks, the arguments it is run with and the template of the
//! prompt, for every repository and, where it differs, for one.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tsumugi_model::AiAction;
use tsumugi_model::profile::ProfileId;

/// How one AI action asks its agent. What is left out is taken from the
/// recipe for every repository, then from what Tsumugi comes with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Recipe {
    /// The profile whose agent is asked; the profile new sessions start from
    /// when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileId>,
    /// Arguments added to the agent's command when it is run once for text,
    /// split as a shell would, such as `--model sonnet`. They are not given to
    /// the sessions the other actions start.
    pub args: String,
    /// The prompt, in which `{basePrompt}` stands for the built-in prompt of
    /// the action. A template that leaves it out replaces the built-in prompt,
    /// including what it tells the agent not to do. The other variables an
    /// action takes are in the [reference](../ai-actions.md#recipes).
    pub template: String,
}

/// The recipe of one action for one repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RepositoryRecipe {
    /// The repository's primary checkout, the directory holding its `.git`.
    pub path: String,
    /// The action it is the recipe of: `commit_message`, `pull_request`,
    /// `fix_commit_failure` or `resolve_conflicts`.
    pub action: Option<AiAction>,
    /// The profile whose agent is asked; left out, the one for every
    /// repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileId>,
    /// Arguments added to the agent's command; left out, those for every
    /// repository.
    pub args: String,
    /// The prompt; left out, the one for every repository.
    pub template: String,
}

/// The agents asked for commit messages, pull requests, hook failures and
/// conflicts, and how.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AiActionsSettings {
    /// Writes a commit message, run once; the user reads it before it commits.
    pub commit_message: Recipe,
    /// Writes a pull request's title and body, run once; the user reads them
    /// before they are sent.
    pub pull_request: Recipe,
    /// Starts a session that fixes what refused a commit; it does not bypass
    /// the hook, commit or push.
    pub fix_commit_failure: Recipe,
    /// Starts a session that resolves the files in conflict; it does not push.
    pub resolve_conflicts: Recipe,
    /// The recipes that differ in a repository, each
    /// `[[ai_actions.repositories]]` table one action of one repository.
    pub repositories: Vec<RepositoryRecipe>,
}

/// What an action is run with once repository, everywhere and default are
/// laid over each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub profile: Option<ProfileId>,
    pub args: Vec<String>,
    pub template: String,
}

impl AiActionsSettings {
    fn global(&self, action: AiAction) -> &Recipe {
        match action {
            AiAction::CommitMessage => &self.commit_message,
            AiAction::PullRequest => &self.pull_request,
            AiAction::FixCommitFailure => &self.fix_commit_failure,
            AiAction::ResolveConflicts => &self.resolve_conflicts,
        }
    }

    /// The recipe of `action` for the repository whose primary checkout is
    /// `primary`: each of its parts from the repository's own recipe, else
    /// the one for every repository, else what Tsumugi comes with. Arguments
    /// that do not split as a shell would are an error saying so.
    pub fn resolve(&self, action: AiAction, primary: &Path) -> Result<Resolved, String> {
        let global = self.global(action);
        let own = self
            .repositories
            .iter()
            .find(|r| r.action == Some(action) && Path::new(&r.path) == primary);
        let profile = own
            .and_then(|r| r.profile.clone())
            .or_else(|| global.profile.clone());
        let args = own
            .map(|r| r.args.as_str())
            .filter(|a| !a.trim().is_empty())
            .unwrap_or(&global.args);
        let args = shell_words::split(args)
            .map_err(|e| format!("The arguments of the {} recipe: {e}", action.key()))?;
        let template = own
            .map(|r| r.template.as_str())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(&global.template);
        let template = if template.trim().is_empty() {
            tsumugi_model::BASE_PROMPT.to_string()
        } else {
            template.to_string()
        };
        Ok(Resolved {
            profile,
            args,
            template,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn id(text: &str) -> ProfileId {
        text.parse().unwrap()
    }

    #[test]
    fn with_nothing_written_an_action_asks_the_default_agent_with_its_built_in_prompt() {
        let settings = Settings::parse("").unwrap();
        let resolved = settings
            .ai_actions
            .resolve(AiAction::CommitMessage, "/r".as_ref())
            .unwrap();
        assert_eq!(
            resolved,
            Resolved {
                profile: None,
                args: Vec::new(),
                template: "{basePrompt}".into()
            }
        );
    }

    #[test]
    fn a_recipe_for_every_repository_names_the_profile_arguments_and_template() {
        let settings = Settings::parse(
            "[ai_actions.commit_message]\nprofile = 'codex'\nargs = \"--model 'gpt 5'\"\n\
             template = 'Short. {basePrompt}'\n",
        )
        .unwrap();
        let resolved = settings
            .ai_actions
            .resolve(AiAction::CommitMessage, "/any".as_ref())
            .unwrap();
        assert_eq!(resolved.profile, Some(id("codex")));
        assert_eq!(resolved.args, ["--model", "gpt 5"]);
        assert_eq!(resolved.template, "Short. {basePrompt}");
        // Another action is left as it was.
        let other = settings
            .ai_actions
            .resolve(AiAction::PullRequest, "/any".as_ref())
            .unwrap();
        assert_eq!(other.profile, None);
    }

    #[test]
    fn a_repository_s_recipe_overrides_part_by_part_and_only_for_its_action() {
        let settings = Settings::parse(
            "[ai_actions.pull_request]\nprofile = 'codex'\nargs = '--fast'\ntemplate = 'G {basePrompt}'\n\
             [[ai_actions.repositories]]\npath = '/r/api'\naction = 'pull_request'\n\
             template = 'R {basePrompt}'\n\
             [[ai_actions.repositories]]\npath = '/r/api'\naction = 'commit_message'\nprofile = 'pi'\n",
        )
        .unwrap();
        let api = settings
            .ai_actions
            .resolve(AiAction::PullRequest, "/r/api".as_ref())
            .unwrap();
        assert_eq!(api.template, "R {basePrompt}");
        assert_eq!(api.profile, Some(id("codex")));
        assert_eq!(api.args, ["--fast"]);
        let web = settings
            .ai_actions
            .resolve(AiAction::PullRequest, "/r/web".as_ref())
            .unwrap();
        assert_eq!(web.template, "G {basePrompt}");
        let commit = settings
            .ai_actions
            .resolve(AiAction::CommitMessage, "/r/api".as_ref())
            .unwrap();
        assert_eq!(commit.profile, Some(id("pi")));
        assert_eq!(
            settings,
            Settings::parse(&toml::to_string(&settings).unwrap()).unwrap()
        );
    }

    #[test]
    fn arguments_that_do_not_split_are_refused_by_name() {
        let settings =
            Settings::parse("[ai_actions.resolve_conflicts]\nargs = \"--x 'unclosed\"\n").unwrap();
        let wrong = settings
            .ai_actions
            .resolve(AiAction::ResolveConflicts, "/r".as_ref())
            .unwrap_err();
        assert!(wrong.contains("resolve_conflicts"), "{wrong}");
    }

    #[test]
    fn an_unknown_action_a_bad_profile_or_a_stray_key_is_refused() {
        assert!(
            Settings::parse("[[ai_actions.repositories]]\npath = '/r'\naction = 'push'\n").is_err()
        );
        assert!(Settings::parse("[ai_actions.commit_message]\nprofile = 'Not Valid'\n").is_err());
        assert!(Settings::parse("[ai_actions.commit_message]\nmodel = 'x'\n").is_err());
        assert!(Settings::parse("[ai_actions]\npush = true\n").is_err());
    }
}
