//! Cursor CLI, the `cursor-agent` program: started in its own terminal
//! interface and asked what its login stands at. Its conversations are
//! listed and read from what it keeps; its models and prompts are not read
//! back.

use super::*;

mod past;
mod transcript;

/// Where Cursor, started as `context` says, keeps its conversations.
fn root(context: &AgentContext, home: &Path) -> PathBuf {
    context
        .home
        .clone()
        .or_else(|| context.variable("CURSOR_CONFIG_DIR"))
        .unwrap_or_else(|| home.join(".cursor"))
}

pub struct Cursor;

/// Whether a login of its own removes `name`: the key Cursor takes in place
/// of its login, and what moves where it keeps its state.
fn denied(name: &str) -> bool {
    matches!(
        name,
        "CURSOR_API_KEY" | "CURSOR_CONFIG_DIR" | "AGENT_CLI_CREDENTIAL_STORE"
    )
}

impl AgentAdapter for Cursor {
    fn agent(&self) -> Agent {
        Agent::Cursor
    }

    // Cursor takes no flag for a system prompt, so Tsumugi's instructions
    // are not added to its own.
    fn instruction_args(&self, _instructions: &str) -> Vec<String> {
        Vec::new()
    }

    // Cursor reads a file named with `@` in the prompt; it attaches no path
    // pasted into its input.
    fn image_prompt(&self, text: &str, images: &[PathBuf]) -> String {
        let mut prompt = text.to_string();
        for image in images {
            if !prompt.is_empty() {
                prompt.push(' ');
            }
            prompt.push_str(&format!("@{}", image.display()));
        }
        prompt
    }

    fn program(&self) -> String {
        "cursor-agent".into()
    }

    fn state_variable(&self) -> &'static str {
        "CURSOR_CONFIG_DIR"
    }

    fn denies(&self, name: &str) -> bool {
        denied(name)
    }

    fn login_args(&self) -> Vec<String> {
        vec!["login".into()]
    }

    fn status_args(&self, _context: &AgentContext) -> Vec<String> {
        vec!["status".into()]
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            command_probe: false,
            usage_limits: false,
            shell_bang: false,
        }
    }

    fn judge_startup(&self, screen: &[String]) -> Judgement<tsumugi_model::StartupReadiness> {
        crate::rules::judge_startup(Agent::Cursor, screen)
    }

    fn resume_args(&self, context: &AgentContext, conversation: &str) -> Option<Vec<String>> {
        let mut args = options(context);
        args.extend(["--resume".into(), conversation.into()]);
        Some(args)
    }

    fn past(
        &self,
        context: &AgentContext,
        home: &Path,
        dir: Option<&Path>,
    ) -> Vec<tsumugi_model::Past> {
        crate::kept_in(context, past::read(&root(context, home), dir))
    }

    // Cursor's interface never names its conversation, which a new session
    // is found by as the one it began.
    fn discover_conversation(
        &self,
        context: AgentContext,
        home: PathBuf,
        cwd: PathBuf,
        since_ms: u64,
        taken: Vec<String>,
    ) -> ConversationLookup {
        let root = root(&context, &home);
        Box::pin(async move {
            tokio::task::spawn_blocking(move || past::begun(&root, &cwd, since_ms, &taken))
                .await
                .ok()
                .flatten()
        })
    }

    fn transcript(
        &self,
        context: &AgentContext,
        home: &Path,
        session: &tsumugi_model::Session,
    ) -> Option<Box<dyn ConversationReader>> {
        if session.session_id.is_empty() {
            return None;
        }
        Some(Box::new(transcript::Transcript::open(
            root(context, home),
            session.session_id.clone(),
            &session.cwd,
            home,
        )))
    }

    fn preview(&self, past: &tsumugi_model::Past) -> Option<tsumugi_model::Preview> {
        past::preview(past.log.as_deref()?)
    }

    fn start_args(
        &self,
        context: &AgentContext,
        _name: &str,
        _conversation: &str,
        prompt: &str,
    ) -> Vec<String> {
        let mut args = options(context);
        if !prompt.is_empty() {
            args.extend(["--".into(), prompt.into()]);
        }
        args
    }
}

/// How a session runs, as `context` says.
fn options(context: &AgentContext) -> Vec<String> {
    let options = context.cursor();
    let mut args = Vec::new();
    for (flag, value) in [
        ("--model", &options.model),
        ("--mode", &options.mode),
        ("--sandbox", &options.sandbox),
    ] {
        if !value.trim().is_empty() {
            args.extend([flag.into(), value.trim().into()]);
        }
    }
    args
}

/// A session kept from changing anything: Cursor's ask mode answers without
/// editing, and its sandbox is on.
pub fn read_only(context: &mut AgentContext) {
    if let Some(options) = context.cursor_mut() {
        options.mode = "ask".into();
        options.sandbox = "enabled".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::config::CursorOptions;
    use tsumugi_model::profile::AgentOptions;

    fn with(options: CursorOptions) -> AgentContext {
        AgentContext {
            options: AgentOptions::Cursor(options),
            ..AgentContext::builtin(Agent::Cursor)
        }
    }

    #[test]
    fn a_session_starts_cursor_s_own_interface_with_the_first_prompt_after_the_options() {
        let context = AgentContext::builtin(Agent::Cursor);
        assert_eq!(
            Cursor.start_args(&context, "fix", "", "read"),
            ["--", "read"]
        );
        assert!(Cursor.start_args(&context, "fix", "", "").is_empty());
        assert!(Cursor.conversation().is_empty());
        assert_eq!(Cursor.program(), "cursor-agent");
    }

    #[test]
    fn a_profile_s_model_mode_and_sandbox_are_cursor_s_own_flags() {
        let context = with(CursorOptions {
            model: " gpt-5 ".into(),
            mode: "plan".into(),
            sandbox: "enabled".into(),
        });
        assert_eq!(
            Cursor.start_args(&context, "", "", "--help"),
            [
                "--model",
                "gpt-5",
                "--mode",
                "plan",
                "--sandbox",
                "enabled",
                "--",
                "--help"
            ]
        );
    }

    #[test]
    fn a_conversation_resumes_by_its_id_run_as_a_new_session_would_be() {
        let context = with(CursorOptions {
            model: "auto".into(),
            ..CursorOptions::default()
        });
        assert_eq!(
            Cursor.resume_args(&context, "c0ffee").unwrap(),
            ["--model", "auto", "--resume", "c0ffee"]
        );
    }

    #[test]
    fn a_read_only_session_asks_in_a_sandbox() {
        let mut context = AgentContext::builtin(Agent::Cursor);
        read_only(&mut context);
        assert_eq!(
            Cursor.start_args(&context, "", "", ""),
            ["--mode", "ask", "--sandbox", "enabled"]
        );
    }

    #[test]
    fn images_are_named_in_the_prompt_by_path() {
        let images = [PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")];
        assert_eq!(
            Cursor.image_prompt("look", &images),
            "look @/tmp/a.png @/tmp/b.png"
        );
        assert_eq!(Cursor.image_prompt("", &images), "@/tmp/a.png @/tmp/b.png");
        assert!(Cursor.image_pastes(&images).is_empty());
    }

    #[test]
    fn a_login_of_its_own_removes_the_key_and_what_moves_cursor_s_state() {
        for name in [
            "CURSOR_API_KEY",
            "CURSOR_CONFIG_DIR",
            "AGENT_CLI_CREDENTIAL_STORE",
        ] {
            assert!(Cursor.denies(name), "{name}");
        }
        for name in ["PATH", "HOME", "CURSOR_VERSION", "TERM"] {
            assert!(!Cursor.denies(name), "{name}");
        }
        assert_eq!(Cursor.state_variable(), "CURSOR_CONFIG_DIR");
    }

    #[test]
    fn the_screen_tells_whether_cursor_works_asks_or_waits_at_its_input() {
        use crate::testing::screen;
        use tsumugi_model::{StartupReadiness, State, Wait};
        for (shown, state) in [
            (
                "⬢ Generating... ctrl+c to stop\n→ Add a follow-up\n/ for commands · @ for files",
                Some(State::Working),
            ),
            (
                "→ Plan, search, build anything\n/ for commands · @ for files",
                Some(State::Idle),
            ),
            (
                "Run this command?\n→ Run (once) (y)\nSkip (esc or n)",
                Some(State::Waiting(Wait::Permission)),
            ),
            (
                "Workspace Trust Required\n[a] Trust this workspace",
                Some(State::Waiting(Wait::Input)),
            ),
            ("Starting Cursor...", None),
        ] {
            assert_eq!(Cursor.screen_state(&screen(shown)), state, "{shown:?}");
        }
        assert_eq!(
            Cursor.startup_readiness(&screen(
                "Workspace Trust Required\n[a] Trust this workspace"
            )),
            Some(StartupReadiness::AwaitingTrust)
        );
        assert!(Cursor.dialog_open(&screen("Skip (esc or n)")));
        assert!(!Cursor.dialog_open(&screen("/ for commands · @ for files")));
    }

    #[test]
    fn cursor_offers_neither_probes_usage_nor_shell_lines() {
        let capabilities = Cursor.capabilities();
        assert!(!capabilities.command_probe);
        assert!(!capabilities.usage_limits);
        assert!(!capabilities.shell_bang);
        assert_eq!(Cursor.usage_url(), None);
        assert!(Cursor.rename_command("fix").is_none());
    }
}
