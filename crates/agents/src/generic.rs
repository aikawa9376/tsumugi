//! Any other command-line agent: the program, its arguments and its
//! variables are the profile's, and how it stands is read from its screen
//! or the status it reports. Nothing of its conversations is read, and it
//! has no login Tsumugi asks about or keeps.

use super::*;
use tsumugi_model::config::PromptDelivery;
use tsumugi_model::profile::LoginStanding;

pub struct Generic;

impl AgentAdapter for Generic {
    fn agent(&self) -> Agent {
        Agent::Generic
    }

    // A program Tsumugi knows nothing of has no flag for instructions to
    // be added to.
    fn instruction_args(&self, _instructions: &str) -> Vec<String> {
        Vec::new()
    }

    // Nothing says how a program takes an image, so none is named.
    fn image_prompt(&self, text: &str, _images: &[PathBuf]) -> String {
        text.to_string()
    }

    // The program is the profile's `launch.command`; a profile that names
    // none is refused before it starts.
    fn program(&self) -> String {
        String::new()
    }

    fn state_variable(&self) -> &'static str {
        ""
    }

    fn own_login_variables(&self, _home: &Path) -> Vec<(String, String)> {
        Vec::new()
    }

    fn denies(&self, _name: &str) -> bool {
        false
    }

    fn login_args(&self) -> Vec<String> {
        Vec::new()
    }

    fn status_args(&self, _context: &AgentContext) -> Vec<String> {
        Vec::new()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: false,
            usage_limits: false,
            shell_bang: false,
        }
    }

    fn judge_startup(&self, screen: &[String]) -> Judgement<tsumugi_model::StartupReadiness> {
        crate::rules::judge_startup(Agent::Generic, screen)
    }

    fn start_args(
        &self,
        context: &AgentContext,
        _name: &str,
        _conversation: &str,
        prompt: &str,
    ) -> Vec<String> {
        let options = context.generic();
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Vec::new();
        }
        match options.prompt {
            PromptDelivery::Argv => vec![prompt.into()],
            PromptDelivery::Separated => vec!["--".into(), prompt.into()],
            PromptDelivery::Flag if !options.prompt_flag.trim().is_empty() => {
                vec![options.prompt_flag.trim().into(), prompt.into()]
            }
            PromptDelivery::Flag | PromptDelivery::Typed => Vec::new(),
        }
    }
}

/// The program `context` starts, which is what tells whether the agent is
/// installed; none for a profile that names no command.
pub fn program(context: &AgentContext) -> Option<&str> {
    context
        .launch
        .command
        .first()
        .map(String::as_str)
        .filter(|program| !program.trim().is_empty())
}

/// Whether the program of `context` is found: in `path`, the directories a
/// program is looked for in, or at its own path when that names one. Not
/// known when a shell starts it, whose startup files may set `PATH`.
pub fn standing(context: &AgentContext, path: Option<&std::ffi::OsStr>) -> LoginStanding {
    let Some(program) = program(context) else {
        return LoginStanding::Missing;
    };
    if context.launch.via_shell && !cfg!(windows) {
        return LoginStanding::Unknown;
    }
    if installed(program, path) {
        // Such a program keeps no login Tsumugi asks about.
        LoginStanding::LoggedIn
    } else {
        LoginStanding::Missing
    }
}

/// Whether `program` is an executable file, named by a path or found in one
/// of the directories of `path`.
pub fn installed(program: &str, path: Option<&std::ffi::OsStr>) -> bool {
    let program = program.trim();
    if program.is_empty() {
        return false;
    }
    if program.contains(std::path::MAIN_SEPARATOR) || program.contains('/') {
        return executable(Path::new(program));
    }
    let Some(path) = path else {
        return false;
    };
    std::env::split_paths(path).any(|dir| {
        !dir.as_os_str().is_empty()
            && (executable(&dir.join(program))
                || (cfg!(windows)
                    && ["exe", "cmd", "bat"]
                        .iter()
                        .any(|ext| executable(&dir.join(format!("{program}.{ext}"))))))
    })
}

fn executable(file: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(file) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::config::GenericOptions;
    use tsumugi_model::profile::{AgentOptions, AgentProfile};

    fn with(command: &[&str], options: GenericOptions) -> AgentContext {
        let mut profile = AgentProfile::builtin(Agent::Generic);
        profile.launch.command = command.iter().map(|word| word.to_string()).collect();
        profile.options = AgentOptions::Generic(options);
        AgentContext::new(&profile, "/bin/sh")
    }

    fn delivering(prompt: PromptDelivery, flag: &str) -> AgentContext {
        with(
            &["tool"],
            GenericOptions {
                prompt,
                prompt_flag: flag.into(),
                ..GenericOptions::default()
            },
        )
    }

    #[test]
    fn the_first_prompt_reaches_the_program_as_the_profile_says() {
        for (context, expected) in [
            (delivering(PromptDelivery::Argv, ""), vec!["go"]),
            (delivering(PromptDelivery::Separated, ""), vec!["--", "go"]),
            (delivering(PromptDelivery::Flag, " -i "), vec!["-i", "go"]),
            (delivering(PromptDelivery::Flag, ""), vec![]),
            (delivering(PromptDelivery::Typed, "-i"), vec![]),
        ] {
            assert_eq!(Generic.start_args(&context, "n", "", " go\n"), expected);
        }
        let context = delivering(PromptDelivery::Argv, "");
        assert!(Generic.start_args(&context, "n", "", "  ").is_empty());
    }

    #[test]
    fn the_program_is_the_first_word_of_the_launch_command_and_its_arguments_come_first() {
        let context = with(&["kiro-cli", "chat", "--tui"], GenericOptions::default());
        assert_eq!(program(&context), Some("kiro-cli"));
        let (program, args, _) = Generic.launch(&context, vec!["go".into()], Vec::new());
        assert_eq!(program, "kiro-cli");
        assert_eq!(args, ["chat", "--tui", "go"]);
        assert_eq!(super::program(&with(&[], GenericOptions::default())), None);
        assert_eq!(
            super::program(&with(&[" "], GenericOptions::default())),
            None
        );
    }

    #[test]
    fn a_command_agent_is_no_login_and_no_conversation_for_tsumugi() {
        assert!(Generic.conversation().is_empty());
        assert!(Generic.instruction_args("be brief").is_empty());
        assert_eq!(
            Generic.image_prompt("look", &[PathBuf::from("/a.png")]),
            "look"
        );
        let capabilities = Generic.capabilities();
        assert!(!capabilities.command_probe && !capabilities.usage_limits);
        assert!(!capabilities.shell_bang);
        assert!(!Generic.denies("PATH"));
        assert!(Generic.own_login_variables(Path::new("/h")).is_empty());
    }

    #[test]
    fn an_agent_is_installed_when_an_executable_of_its_name_lies_on_the_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tsumugi-generic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plain = dir.join("plain");
        std::fs::write(&plain, "text").unwrap();
        let path = std::env::join_paths([Path::new("/nonexistent"), dir.as_path()]).unwrap();
        assert!(installed("tool", Some(&path)));
        assert!(!installed("plain", Some(&path)), "not executable");
        assert!(!installed("absent", Some(&path)));
        assert!(!installed("tool", None));
        assert!(!installed("", Some(&path)));
        assert!(installed(&tool.display().to_string(), None), "by its path");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_program_or_none_named_is_told_apart_from_one_found_and_one_a_shell_finds() {
        let found = with(&["sh"], GenericOptions::default());
        let path = std::env::var_os("PATH");
        assert_eq!(standing(&found, path.as_deref()), LoginStanding::LoggedIn);
        let missing = with(&["tsumugi-no-such-program"], GenericOptions::default());
        assert_eq!(standing(&missing, path.as_deref()), LoginStanding::Missing);
        let none = with(&[], GenericOptions::default());
        assert_eq!(standing(&none, path.as_deref()), LoginStanding::Missing);
        let mut shelled = with(&["tsumugi-no-such-program"], GenericOptions::default());
        shelled.launch.via_shell = true;
        assert_eq!(standing(&shelled, path.as_deref()), LoginStanding::Unknown);
    }

    #[test]
    fn the_screen_is_idle_once_drawn_working_while_it_hints_to_interrupt_and_unknown_when_blank() {
        use crate::testing::screen;
        use tsumugi_model::State;
        for (shown, state) in [
            ("> ready for a task", Some(State::Idle)),
            ("thinking…\n(esc to interrupt)", Some(State::Working)),
            ("running\nPress Ctrl+C to stop", Some(State::Working)),
            ("", None),
        ] {
            assert_eq!(Generic.screen_state(&screen(shown)), state, "{shown:?}");
        }
    }
}
