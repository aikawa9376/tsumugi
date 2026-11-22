//! An agent's program run once on a prompt, for the text it answers with:
//! its non-interactive mode, started as a session of the profile would be
//! and given up after a time. It is asked for text only; what it does in the
//! worktree meanwhile is up to the arguments the user's recipe gives it and
//! the prompt, which forbids committing and pushing.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tsumugi_model::Agent;

use crate::{AgentContext, Started, launch_in};

/// The longest a prompt is, in bytes: one argument of a program cannot be
/// longer than the system allows.
pub const MAX_PROMPT: usize = 120_000;

/// How long an agent has to answer.
pub const TIMEOUT: Duration = Duration::from_secs(300);

/// The arguments that run `agent` once on `prompt`, with `extra` from the
/// user's recipe ahead of it.
pub fn print_args(agent: Agent, extra: &[String], prompt: &str) -> Vec<String> {
    let (lead, separator): (&[&str], bool) = match agent {
        Agent::Claude => (&["-p"], true),
        Agent::Codex => (&["exec"], true),
        Agent::Pi => (&["-p"], false),
        Agent::OpenCode => (&["run"], false),
        Agent::Cursor => (&["-p"], false),
        // A command agent takes its prompt as it is.
        Agent::Generic => (&[], false),
    };
    let mut args: Vec<String> = lead.iter().map(|a| a.to_string()).collect();
    args.extend(extra.iter().cloned());
    if separator {
        args.push("--".into());
    }
    args.push(prompt.to_string());
    args
}

/// The text `started` prints in `dir`, or why there is none: it cannot
/// start, it ended badly, it did not end in `timeout` or it said nothing.
pub async fn run_once(
    (program, args, env): Started,
    dir: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let child = tokio::process::Command::new(&program)
        .args(args)
        .envs(env)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Cannot start {program}: {e}"))?;
    let out = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("Cannot wait for {program}: {e}")),
        Err(_) => {
            return Err(format!(
                "{program} did not answer within {} seconds",
                timeout.as_secs()
            ));
        }
    };
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        let said = said.trim();
        let said = if said.is_empty() { text.as_str() } else { said };
        let tail: Vec<&str> = said.lines().rev().take(5).collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(match out.status.code() {
            Some(code) => format!("{program} ended with status {code}: {tail}"),
            None => format!("{program} was stopped: {tail}"),
        });
    }
    if text.is_empty() {
        return Err(format!("{program} answered nothing"));
    }
    Ok(text)
}

/// Run the agent of `context` once in `dir` on `prompt`, with `extra`
/// arguments, and answer with its text.
pub async fn generate(
    context: &AgentContext,
    dir: &Path,
    extra: &[String],
    prompt: &str,
) -> Result<String, String> {
    if prompt.len() > MAX_PROMPT {
        return Err(format!(
            "The prompt is {} bytes, more than the {MAX_PROMPT} an agent is run with; shorten the recipe's template",
            prompt.len()
        ));
    }
    let args = print_args(context.agent(), extra, prompt);
    run_once(launch_in(context, args, Vec::new()), dir, TIMEOUT).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{program, started};

    #[test]
    fn each_agent_is_run_in_its_print_mode_with_the_recipe_s_arguments_before_the_prompt() {
        let extra = vec!["--model".to_string(), "x".to_string()];
        assert_eq!(
            print_args(Agent::Claude, &extra, "go"),
            ["-p", "--model", "x", "--", "go"]
        );
        assert_eq!(
            print_args(Agent::Codex, &extra, "go"),
            ["exec", "--model", "x", "--", "go"]
        );
        for agent in [Agent::Pi, Agent::Cursor] {
            assert_eq!(print_args(agent, &[], "go"), ["-p", "go"]);
        }
        assert_eq!(print_args(Agent::OpenCode, &[], "go"), ["run", "go"]);
    }

    #[test]
    fn a_prompt_that_looks_like_an_option_is_still_the_prompt_for_agents_that_take_a_separator() {
        let args = print_args(Agent::Claude, &[], "--dangerous");
        assert_eq!(args.last().unwrap(), "--dangerous");
        assert_eq!(args[args.len() - 2], "--");
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
    async fn what_the_program_printed_is_the_text_trimmed() {
        let (dir, p) = program("cli", "printf '  core: add it\\n\\nwhy\\n\\n'");
        let text = run_once(started(&p, &[]), dir.path(), Duration::from_secs(5)).await;
        assert_eq!(text.unwrap(), "core: add it\n\nwhy");
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
    async fn a_program_that_fails_says_what_it_wrote_to_its_error_output() {
        let (dir, p) = program("cli", "echo 'not logged in' >&2\nexit 3");
        let wrong = run_once(started(&p, &[]), dir.path(), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(
            wrong.contains("status 3") && wrong.contains("not logged in"),
            "{wrong}"
        );
    }

    #[tokio::test]
    #[cfg_attr(windows, ignore = "fakes the agent with a shell script")]
    async fn a_program_that_says_nothing_or_too_late_gives_no_text() {
        let (dir, p) = program("cli", "exit 0");
        let wrong = run_once(started(&p, &[]), dir.path(), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(wrong.contains("answered nothing"), "{wrong}");
        let (dir, p) = program("slow", "exec sleep 30");
        let wrong = run_once(started(&p, &[]), dir.path(), Duration::from_millis(200))
            .await
            .unwrap_err();
        assert!(wrong.contains("did not answer"), "{wrong}");
    }

    #[tokio::test]
    async fn a_program_that_cannot_start_says_so() {
        let missing = started(Path::new("/nonexistent/tsumugi-test-cli"), &[]);
        let wrong = run_once(missing, Path::new("/"), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(wrong.contains("Cannot start"), "{wrong}");
    }

    #[tokio::test]
    async fn a_prompt_too_long_for_an_argument_is_refused_before_anything_runs() {
        let wrong = generate(
            &AgentContext::builtin(Agent::Claude),
            Path::new("/"),
            &[],
            &"x".repeat(MAX_PROMPT + 1),
        )
        .await
        .unwrap_err();
        assert!(wrong.contains("shorten"), "{wrong}");
    }
}
