//! The command-line agents Tsumugi knows how to start without an adapter of
//! their own: what each is called, the program that starts it and how it is
//! handed its first prompt.

use crate::config::{GenericOptions, Launch, PromptDelivery};
use crate::profile::{AgentOptions, AgentProfile, ProfileId};

/// One command-line agent of the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Its profile's id, which no agent's own key is.
    pub key: &'static str,
    pub label: &'static str,
    /// The program that starts it, then the arguments it always gets. The
    /// program is also what tells whether it is installed.
    pub command: &'static [&'static str],
    pub prompt: PromptDelivery,
    /// The flag that takes the prompt, for [`PromptDelivery::Flag`].
    pub prompt_flag: &'static str,
    /// Where its installation is described; empty when not known.
    pub homepage: &'static str,
}

impl CatalogEntry {
    /// The program that starts it.
    pub fn program(&self) -> &'static str {
        self.command[0]
    }

    /// The profile that starts it: the catalog's own for the entry, which
    /// the settings may replace by a profile of the same id.
    pub fn profile(&self) -> AgentProfile {
        AgentProfile {
            id: self
                .key
                .parse::<ProfileId>()
                .expect("a catalog key is a valid profile id"),
            name: self.label.to_string(),
            home: None,
            launch: Launch {
                command: self.command.iter().map(|word| word.to_string()).collect(),
                ..Launch::default()
            },
            options: AgentOptions::Generic(GenericOptions {
                prompt: self.prompt,
                prompt_flag: self.prompt_flag.to_string(),
                ..GenericOptions::default()
            }),
            poll_usage: false,
        }
    }
}

const fn entry(
    key: &'static str,
    label: &'static str,
    command: &'static [&'static str],
    prompt: PromptDelivery,
    prompt_flag: &'static str,
    homepage: &'static str,
) -> CatalogEntry {
    CatalogEntry {
        key,
        label,
        command,
        prompt,
        prompt_flag,
        homepage,
    }
}

use PromptDelivery::{Argv, Flag, Separated, Typed};

/// The agents, in the order a launch menu lists them.
pub const CATALOG: &[CatalogEntry] = &[
    entry(
        "gemini",
        "Gemini CLI",
        &["gemini"],
        Flag,
        "--prompt-interactive",
        "https://github.com/google-gemini/gemini-cli",
    ),
    entry(
        "copilot",
        "GitHub Copilot",
        &["copilot"],
        Flag,
        "-i",
        "https://github.com/github/copilot-cli",
    ),
    entry("amp", "Amp", &["amp"], Typed, "", "https://ampcode.com"),
    entry(
        "aider",
        "Aider",
        &["aider"],
        Typed,
        "",
        "https://aider.chat",
    ),
    entry(
        "goose",
        "Goose",
        &["goose"],
        Typed,
        "",
        "https://github.com/block/goose",
    ),
    entry(
        "kiro",
        "Kiro",
        &["kiro-cli", "chat", "--tui"],
        Typed,
        "",
        "https://kiro.dev/cli",
    ),
    entry("droid", "Droid", &["droid"], Argv, "", "https://factory.ai"),
    entry("kilo", "Kilo", &["kilo"], Typed, "", "https://kilo.ai"),
    entry(
        "crush",
        "Crush",
        &["crush"],
        Typed,
        "",
        "https://github.com/charmbracelet/crush",
    ),
    entry(
        "auggie",
        "Augment",
        &["auggie"],
        Typed,
        "",
        "https://www.augmentcode.com",
    ),
    entry(
        "cline",
        "Cline",
        &["cline"],
        Typed,
        "",
        "https://github.com/cline/cline",
    ),
    entry(
        "continue",
        "Continue",
        &["cn"],
        Typed,
        "",
        "https://www.continue.dev",
    ),
    entry(
        "kimi",
        "Kimi",
        &["kimi"],
        Typed,
        "",
        "https://github.com/MoonshotAI/kimi-cli",
    ),
    entry(
        "vibe",
        "Mistral Vibe",
        &["vibe"],
        Typed,
        "",
        "https://github.com/mistralai/mistral-vibe",
    ),
    entry(
        "qwen-code",
        "Qwen Code",
        &["qwen"],
        Typed,
        "",
        "https://github.com/QwenLM/qwen-code",
    ),
    entry("grok", "Grok", &["grok"], Separated, "", ""),
    entry("devin", "Devin", &["devin"], Separated, "", ""),
    entry("codebuddy", "CodeBuddy", &["codebuddy"], Argv, "", ""),
    entry("openclaude", "OpenClaude", &["openclaude"], Argv, "", ""),
    entry("omp", "OMP", &["omp"], Argv, "", ""),
    entry("trae", "Trae", &["traecli"], Separated, "", ""),
    entry("hermes", "Hermes", &["hermes", "--tui"], Typed, "", ""),
    entry("openclaw", "OpenClaw", &["openclaw"], Typed, "", ""),
    entry("codebuff", "Codebuff", &["codebuff"], Typed, "", ""),
    entry("mimo", "MiMo Code", &["mimo"], Flag, "--prompt", ""),
    entry(
        "antigravity",
        "Antigravity",
        &["agy"],
        Flag,
        "--prompt-interactive",
        "",
    ),
    entry(
        "qoder",
        "Qoder CLI",
        &["qodercli"],
        Flag,
        "--prompt-interactive",
        "",
    ),
];

/// The entry whose profile id is `id`.
pub fn find(id: &ProfileId) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.key == id.as_str())
}

/// The entry whose profile id is `key`.
pub fn entry_of(key: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Agent;
    use std::collections::HashSet;

    #[test]
    fn every_entry_is_a_valid_profile_id_that_no_agent_s_own_key_takes() {
        let mut seen = HashSet::new();
        for entry in CATALOG {
            assert!(entry.key.parse::<ProfileId>().is_ok(), "{}", entry.key);
            assert!(Agent::parse(entry.key).is_none(), "{}", entry.key);
            assert!(seen.insert(entry.key), "{} twice", entry.key);
            assert!(!entry.command.is_empty(), "{}", entry.key);
            assert!(!entry.label.is_empty(), "{}", entry.key);
        }
    }

    #[test]
    fn a_flag_names_its_flag_and_no_other_delivery_does() {
        for entry in CATALOG {
            assert_eq!(
                entry.prompt == PromptDelivery::Flag,
                !entry.prompt_flag.is_empty(),
                "{}",
                entry.key
            );
        }
    }

    #[test]
    fn an_entry_s_profile_starts_its_command_as_a_command_agent() {
        let kiro = entry_of("kiro").unwrap().profile();
        assert_eq!(kiro.id.as_str(), "kiro");
        assert_eq!(kiro.agent(), Agent::Generic);
        assert_eq!(kiro.launch.command, ["kiro-cli", "chat", "--tui"]);
        assert_eq!(kiro.options.generic().unwrap().prompt, Typed);
        assert_eq!(entry_of("kiro").unwrap().program(), "kiro-cli");
        let gemini = entry_of("gemini").unwrap().profile();
        let options = gemini.options.generic().unwrap();
        assert_eq!(
            (options.prompt, options.prompt_flag.as_str()),
            (Flag, "--prompt-interactive")
        );
        assert!(find(&"gemini".parse().unwrap()).is_some());
        assert!(find(&"claude".parse().unwrap()).is_none());
    }
}
