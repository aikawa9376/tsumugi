//! Leader mode: the language a leader speaks to the user in, how far its
//! discretion goes, the launch of its own session and the tiers it starts its
//! workers by.

use serde::{Deserialize, Serialize};
use tsumugi_model::profile::ProfileId;

/// The size of a worker's task, which settings resolve to a profile and model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// A small task.
    Light,
    /// A task of ordinary size.
    Standard,
    /// A large or hard task.
    Heavy,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Light, Tier::Standard, Tier::Heavy];

    pub fn name(self) -> &'static str {
        match self {
            Tier::Light => "light",
            Tier::Standard => "standard",
            Tier::Heavy => "heavy",
        }
    }
}

/// What a tier starts: the profile, which fixes the agent and its login, and
/// the model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WorkerTier {
    /// The profile it starts from, by id; the profile new sessions start from
    /// when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileId>,
    /// The model it starts, one its profile lists; the agent's own default when
    /// empty.
    pub model: String,
}

/// The tiers workers are started by. A tier that is not written is not
/// defined, and starting by it fails rather than starting something else.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WorkerTiers {
    /// For small tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<WorkerTier>,
    /// For tasks of ordinary size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard: Option<WorkerTier>,
    /// For large or hard tasks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heavy: Option<WorkerTier>,
}

impl WorkerTiers {
    /// What `tier` starts, or why nothing: the tier is not defined, naming
    /// those that are and where to define it.
    pub fn resolve(&self, tier: Tier) -> Result<&WorkerTier, String> {
        let defined = |tier| match tier {
            Tier::Light => &self.light,
            Tier::Standard => &self.standard,
            Tier::Heavy => &self.heavy,
        };
        if let Some(found) = defined(tier) {
            return Ok(found);
        }
        let others: Vec<_> = Tier::ALL
            .into_iter()
            .filter(|other| defined(*other).is_some())
            .map(Tier::name)
            .collect();
        let others = match others.as_slice() {
            [] => "none is defined".to_string(),
            some => format!("defined: {}", some.join(", ")),
        };
        Err(format!(
            "worker tier `{}` is not defined ({others}); the user defines it under [worker_tiers] in settings.toml, or you can name the agent, profile and model yourself",
            tier.name()
        ))
    }
}

/// Whether the leader decides one kind of action itself or asks the user
/// before it.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Discretion {
    /// The leader decides and acts.
    Decide,
    /// The leader asks the user, and acts only once told to.
    #[default]
    Ask,
}

/// How far a leader's discretion goes, kind of action by kind of action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DiscretionSettings {
    /// Starting, resuming and, once their work is done, closing the worker
    /// sessions it supervises.
    pub manage_sessions: Discretion,
    /// Merging a finished branch into `main` while the main checkout is clean.
    pub merge_to_main: Discretion,
    /// Merging into `main` while its checkout has uncommitted changes.
    pub merge_over_changes: Discretion,
    /// Deleting the worktree of a finished branch.
    pub remove_worktrees: Discretion,
    /// Closing an issue whose work is merged.
    pub close_issues: Discretion,
    /// Pushing to a remote.
    pub push: Discretion,
    /// Operations that discard work or rewrite history: forced pushes, hard
    /// resets, discarded changes, ending a session by force.
    pub force: Discretion,
    /// Deleting a branch that is not merged.
    pub delete_unmerged_branches: Discretion,
    /// Starting an issue that is only to be researched.
    pub research_issues: Discretion,
    /// Settling a design question Orca does not settle. Where Orca does, the
    /// leader follows it without asking either way.
    pub design_choices: Discretion,
}

impl Default for DiscretionSettings {
    fn default() -> Self {
        use Discretion::{Ask, Decide};
        Self {
            manage_sessions: Decide,
            merge_to_main: Decide,
            merge_over_changes: Ask,
            remove_worktrees: Decide,
            close_issues: Decide,
            push: Ask,
            force: Ask,
            delete_unmerged_branches: Ask,
            research_issues: Ask,
            design_choices: Ask,
        }
    }
}

/// One kind of action a leader may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    ManageSessions,
    MergeToMain,
    MergeOverChanges,
    RemoveWorktrees,
    CloseIssues,
    Push,
    Force,
    DeleteUnmergedBranches,
    ResearchIssues,
    DesignChoices,
}

impl Act {
    pub const ALL: [Act; 10] = [
        Act::ManageSessions,
        Act::MergeToMain,
        Act::MergeOverChanges,
        Act::RemoveWorktrees,
        Act::CloseIssues,
        Act::Push,
        Act::Force,
        Act::DeleteUnmergedBranches,
        Act::ResearchIssues,
        Act::DesignChoices,
    ];

    /// What it is, as a leader is told it.
    pub fn what(self) -> &'static str {
        match self {
            Act::ManageSessions => {
                "start, resume and, once their work is done, close the worker sessions you supervise"
            }
            Act::MergeToMain => {
                "merge a finished branch into main while the main checkout is clean"
            }
            Act::MergeOverChanges => "merge into main while its checkout has uncommitted changes",
            Act::RemoveWorktrees => "delete the worktree of a finished branch",
            Act::CloseIssues => "close an issue whose work is merged",
            Act::Push => "push to a remote",
            Act::Force => {
                "force anything: a forced push, a hard reset, discarding changes, ending a session by force"
            }
            Act::DeleteUnmergedBranches => "delete a branch that is not merged",
            Act::ResearchIssues => "start an issue that is only to be researched",
            Act::DesignChoices => {
                "settle a design question that Orca does not settle (where Orca settles it, follow Orca without asking)"
            }
        }
    }

    /// What it is called where the user sets it.
    pub fn label(self) -> &'static str {
        match self {
            Act::ManageSessions => "Run worker sessions",
            Act::MergeToMain => "Merge into main",
            Act::MergeOverChanges => "Merge over uncommitted changes in main",
            Act::RemoveWorktrees => "Delete finished worktrees",
            Act::CloseIssues => "Close finished issues",
            Act::Push => "Push",
            Act::Force => "Force operations",
            Act::DeleteUnmergedBranches => "Delete unmerged branches",
            Act::ResearchIssues => "Start research-only issues",
            Act::DesignChoices => "Settle design questions Orca does not",
        }
    }
}

impl DiscretionSettings {
    /// How `act` is held.
    pub fn of(&self, act: Act) -> Discretion {
        match act {
            Act::ManageSessions => self.manage_sessions,
            Act::MergeToMain => self.merge_to_main,
            Act::MergeOverChanges => self.merge_over_changes,
            Act::RemoveWorktrees => self.remove_worktrees,
            Act::CloseIssues => self.close_issues,
            Act::Push => self.push,
            Act::Force => self.force,
            Act::DeleteUnmergedBranches => self.delete_unmerged_branches,
            Act::ResearchIssues => self.research_issues,
            Act::DesignChoices => self.design_choices,
        }
    }

    /// Hold `act` as `how`.
    pub fn set(&mut self, act: Act, how: Discretion) {
        let held = match act {
            Act::ManageSessions => &mut self.manage_sessions,
            Act::MergeToMain => &mut self.merge_to_main,
            Act::MergeOverChanges => &mut self.merge_over_changes,
            Act::RemoveWorktrees => &mut self.remove_worktrees,
            Act::CloseIssues => &mut self.close_issues,
            Act::Push => &mut self.push,
            Act::Force => &mut self.force,
            Act::DeleteUnmergedBranches => &mut self.delete_unmerged_branches,
            Act::ResearchIssues => &mut self.research_issues,
            Act::DesignChoices => &mut self.design_choices,
        };
        *held = how;
    }

    /// The actions held as `how`.
    pub fn listed(&self, how: Discretion) -> Vec<Act> {
        Act::ALL
            .into_iter()
            .filter(|act| self.of(*act) == how)
            .collect()
    }

    /// What a leader is told of its discretion: what it decides, what it asks
    /// the user before doing, and that what is not listed is asked too.
    pub fn instructions(&self) -> String {
        let listed = |held| {
            self.listed(held)
                .into_iter()
                .map(|act| format!("- {}\n", act.what()))
                .collect::<String>()
        };
        let mut told = String::from("Your discretion, as the user set it.\n");
        let decide = listed(Discretion::Decide);
        if !decide.is_empty() {
            told.push_str(&format!(
                "Decide these yourself, without asking the user:\n{decide}"
            ));
        }
        let ask = listed(Discretion::Ask);
        if !ask.is_empty() {
            told.push_str(&format!(
                "Ask the user before you do any of these, and do nothing of them until the user has answered:\n{ask}"
            ));
        }
        told.push_str(
            "Anything not listed is not yours to decide either: ask the user first. \
             The user is the owner; call on them only for what you cannot decide and what cannot be undone.",
        );
        told
    }
}

/// The leader's own session and how many workers it keeps at work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LeaderSettings {
    /// The profile the leader's own session starts from, by id; the profile new
    /// sessions start from when left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileId>,
    /// The model the leader's own session starts, one its profile lists; the
    /// agent's own default when empty.
    pub model: String,
    /// The most workers a leader keeps at work at once.
    pub max_workers: u32,
    /// What the leader decides itself and what it asks the user first.
    pub discretion: DiscretionSettings,
}

impl Default for LeaderSettings {
    fn default() -> Self {
        Self {
            profile: None,
            model: String::new(),
            max_workers: 2,
            discretion: DiscretionSettings::default(),
        }
    }
}

/// The language a leader speaks to the user in: `chosen` when set, else the
/// one the environment's locale names (`LC_ALL`, `LC_MESSAGES`, then `LANG`),
/// else English. A language tag such as `ja`, as the locale writes it.
pub fn language(chosen: &str, environment: impl Fn(&str) -> Option<String>) -> String {
    let chosen = chosen.trim();
    if !chosen.is_empty() {
        return chosen.to_string();
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(environment)
        .map(|locale| locale.trim().to_string())
        .find(|locale| !locale.is_empty())
        .and_then(|locale| {
            let tag = locale.split(['_', '.', '@', '-']).next().unwrap_or("");
            let tag = tag.to_lowercase();
            (!matches!(tag.as_str(), "" | "c" | "posix")).then_some(tag)
        })
        .unwrap_or_else(|| "en".to_string())
}

/// What a leader is told of the language: the user is spoken to in `language`,
/// and everything the user does not read stays English.
pub fn language_instructions(language: &str) -> String {
    let name = match language {
        "ja" => "Japanese",
        "en" => "English",
        "zh" => "Chinese",
        "ko" => "Korean",
        "de" => "German",
        "fr" => "French",
        "es" => "Spanish",
        "pt" => "Portuguese",
        "it" => "Italian",
        "ru" => "Russian",
        other => other,
    };
    format!(
        "Speak to the user in {name}: every report, question and confirmation. \
         Write everything the user does not read in English whatever the language: prompts to sessions, messages to sessions, \
         commit messages, code, commands and paths."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn leader_mode_is_off_the_beaten_path_until_the_user_defines_its_tiers() {
        let settings = Settings::parse("").unwrap();
        assert_eq!(settings.worker_tiers, WorkerTiers::default());
        assert_eq!(settings.leader, LeaderSettings::default());
        assert_eq!(settings.language, "");
        for tier in Tier::ALL {
            assert!(settings.worker_tiers.resolve(tier).is_err());
        }
    }

    #[test]
    fn tiers_are_read_as_tables_or_inline_tables_and_written_back() {
        let text = "[worker_tiers]\nlight = { profile = 'claude', model = 'haiku' }\n\
                    [worker_tiers.heavy]\nprofile = 'codex'\nmodel = 'gpt-5'\n";
        let settings = Settings::parse(text).unwrap();
        let tiers = &settings.worker_tiers;
        let light = tiers.resolve(Tier::Light).unwrap();
        assert_eq!(light.model, "haiku");
        assert_eq!(
            light.profile.as_ref().map(|p| p.to_string()),
            Some("claude".into())
        );
        assert_eq!(tiers.resolve(Tier::Heavy).unwrap().model, "gpt-5");
        assert_eq!(
            Settings::parse(&toml::to_string_pretty(&settings).unwrap()).unwrap(),
            settings
        );
    }

    #[test]
    fn a_tier_that_is_not_defined_fails_naming_the_ones_that_are() {
        let settings = Settings::parse("[worker_tiers.light]\nmodel = 'haiku'\n").unwrap();
        let refused = settings.worker_tiers.resolve(Tier::Standard).unwrap_err();
        assert!(refused.contains("`standard` is not defined"), "{refused}");
        assert!(refused.contains("defined: light"), "{refused}");
        assert!(refused.contains("[worker_tiers]"), "{refused}");
        let none = Settings::parse("")
            .unwrap()
            .worker_tiers
            .resolve(Tier::Light)
            .unwrap_err();
        assert!(none.contains("none is defined"), "{none}");
    }

    #[test]
    fn a_name_that_is_no_tier_or_a_key_that_is_no_setting_is_refused() {
        assert!(Settings::parse("[worker_tiers.huge]\nmodel = 'x'\n").is_err());
        assert!(Settings::parse("[worker_tiers.light]\neffort = 'high'\n").is_err());
        assert!(Settings::parse("[leader.discretion]\nmerge = 'decide'\n").is_err());
        assert!(Settings::parse("[leader.discretion]\npush = 'sometimes'\n").is_err());
    }

    #[test]
    fn by_default_a_leader_decides_the_running_of_sessions_and_asks_for_what_cannot_be_undone() {
        use Discretion::{Ask, Decide};
        let discretion = DiscretionSettings::default();
        assert_eq!(discretion.manage_sessions, Decide);
        assert_eq!(discretion.merge_to_main, Decide);
        assert_eq!(discretion.remove_worktrees, Decide);
        assert_eq!(discretion.close_issues, Decide);
        assert_eq!(discretion.merge_over_changes, Ask);
        assert_eq!(discretion.push, Ask);
        assert_eq!(discretion.force, Ask);
        assert_eq!(discretion.delete_unmerged_branches, Ask);
        assert_eq!(discretion.research_issues, Ask);
        assert_eq!(discretion.design_choices, Ask);
    }

    #[test]
    fn the_leader_is_told_to_ask_before_each_action_outside_its_discretion() {
        let told = DiscretionSettings::default().instructions();
        let (decided, asked) = told.split_once("Ask the user before").unwrap();
        for outside in [
            "uncommitted changes",
            "push to a remote",
            "force anything",
            "not merged",
            "only to be researched",
            "Orca does not settle",
        ] {
            assert!(asked.contains(outside), "{outside}: {told}");
            assert!(!decided.contains(outside), "{outside}: {told}");
        }
        for inside in [
            "start, resume",
            "clean",
            "delete the worktree",
            "close an issue",
        ] {
            assert!(decided.contains(inside), "{inside}: {told}");
            assert!(!asked.contains(inside), "{inside}: {told}");
        }
        assert!(told.contains("do nothing of them until the user has answered"));
        assert!(told.contains("Anything not listed"));
    }

    #[test]
    fn the_discretion_the_user_sets_moves_an_action_between_the_two_lists() {
        let settings =
            Settings::parse("[leader.discretion]\npush = 'decide'\nclose_issues = 'ask'\n")
                .unwrap();
        let told = settings.leader.discretion.instructions();
        let (decided, asked) = told.split_once("Ask the user before").unwrap();
        assert!(decided.contains("push to a remote"), "{told}");
        assert!(asked.contains("close an issue"), "{told}");
        assert!(!asked.contains("push to a remote"), "{told}");
    }

    #[test]
    fn every_kind_of_action_is_held_one_way_and_set_back_the_same() {
        let mut discretion = DiscretionSettings::default();
        for act in Act::ALL {
            let before = discretion.of(act);
            let other = match before {
                Discretion::Decide => Discretion::Ask,
                Discretion::Ask => Discretion::Decide,
            };
            discretion.set(act, other);
            assert_eq!(discretion.of(act), other, "{act:?}");
            assert!(discretion.listed(other).contains(&act), "{act:?}");
            assert!(!discretion.listed(before).contains(&act), "{act:?}");
        }
        let labels: std::collections::HashSet<_> = Act::ALL.iter().map(|a| a.label()).collect();
        assert_eq!(labels.len(), Act::ALL.len(), "each is labelled apart");
    }

    #[test]
    fn a_leader_that_asks_for_nothing_or_decides_nothing_has_no_empty_list() {
        let all = |how| DiscretionSettings {
            manage_sessions: how,
            merge_to_main: how,
            merge_over_changes: how,
            remove_worktrees: how,
            close_issues: how,
            push: how,
            force: how,
            delete_unmerged_branches: how,
            research_issues: how,
            design_choices: how,
        };
        assert!(!all(Discretion::Ask).instructions().contains("Decide these"));
        assert!(
            !all(Discretion::Decide)
                .instructions()
                .contains("Ask the user before")
        );
    }

    #[test]
    fn the_leader_s_own_launch_and_workers_are_read_and_kept_sane() {
        let text = "[leader]\nprofile = 'claude'\nmodel = 'opus'\nmax_workers = 3\n";
        let settings = Settings::parse(text).unwrap();
        assert_eq!(settings.leader.model, "opus");
        assert_eq!(settings.leader.max_workers, 3);
        assert_eq!(
            Settings::parse("[leader]\nmax_workers = 0\n")
                .unwrap()
                .leader
                .max_workers,
            1
        );
    }

    fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn the_language_the_user_set_wins_over_the_locale() {
        let locale = environment(&[("LANG", "en_US.UTF-8")]);
        assert_eq!(language(" ja ", locale), "ja");
    }

    #[test]
    fn without_one_the_language_is_the_environment_s_locale_in_the_order_the_system_reads_it() {
        assert_eq!(language("", environment(&[("LANG", "ja_JP.UTF-8")])), "ja");
        assert_eq!(
            language(
                "",
                environment(&[("LANG", "en_US.UTF-8"), ("LC_ALL", "de_DE@euro")])
            ),
            "de"
        );
        assert_eq!(
            language("", environment(&[("LC_ALL", ""), ("LANG", "pt-BR")])),
            "pt"
        );
    }

    #[test]
    fn a_locale_that_names_no_language_is_english() {
        for locale in [
            &[][..],
            &[("LANG", "C")],
            &[("LANG", "POSIX")],
            &[("LANG", "")],
        ] {
            assert_eq!(language("", environment(locale)), "en", "{locale:?}");
        }
    }

    #[test]
    fn the_leader_speaks_to_the_user_in_their_language_and_writes_the_rest_in_english() {
        let told = language_instructions("ja");
        assert!(told.contains("in Japanese"), "{told}");
        assert!(told.contains("report, question and confirmation"), "{told}");
        assert!(told.contains("prompts to sessions"), "{told}");
        assert!(told.contains("commit messages"), "{told}");
        assert!(
            language_instructions("nl").contains("in nl"),
            "an unknown tag is kept"
        );
    }
}
