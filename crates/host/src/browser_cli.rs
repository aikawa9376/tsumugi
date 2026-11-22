//! `tsumugi browser`: what the MCP browser tools do, for an agent that has a
//! shell instead. It acts as the session it runs in, on the pages of that
//! session's worktree.

use serde_json::Value;
use tsumugi_protocol::{BrowserCommand, ElementState, ImageFormat, LoadState};

use crate::delegation::launch::{ROOT_ENV, TOKEN_ENV};
use crate::delegation::protocol::{Command, ask};

pub const USAGE: &str = "usage: tsumugi browser <goto --url <url> | snapshot [--interactive] [--delta] [--selector <css>] [--depth <n>] | click --element <@ref> | fill --element <@ref> --value <text> | wait [--selector <css>] [--state <hidden|visible>] [--text <text>] [--url <pattern>] [--load <load|domcontentloaded|networkidle>] [--timeout <ms>] | screenshot | full-screenshot [--format <png|jpeg>] | pdf | console [--limit <n>] | network [--limit <n>] | capture <start|stop> | set device --name <device> | tab list | tab create [--url <url>] | tab switch --index <n>> [--json]";

/// A positive whole number a flag holds.
fn positive(flag: &str, text: &str) -> Result<usize, String> {
    text.parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("{flag} takes a number above 0"))
}

/// A command read from `args`, and whether it asked for JSON.
#[derive(Debug, PartialEq)]
struct Asked {
    command: BrowserCommand,
    json: bool,
}

/// What `args` after `tsumugi browser` ask for.
fn parse(args: &[String]) -> Result<Asked, String> {
    let mut words = args.iter().map(String::as_str);
    let mut json = false;
    let mut positional = Vec::new();
    let mut flags: Vec<(&str, Option<&str>)> = Vec::new();
    while let Some(word) = words.next() {
        match word {
            "--json" => json = true,
            "--interactive" | "--delta" => flags.push((word, None)),
            "--fn" => {
                return Err("--fn is not taken: a page's password fields are never read to an agent, and a script of your own could read them; wait on --selector, --text, --url or --load".into());
            }
            "--url" | "--element" | "--value" | "--selector" | "--depth" | "--index" | "--text"
            | "--load" | "--state" | "--timeout" | "--limit" | "--name" | "--format" => {
                let value = words
                    .next()
                    .ok_or_else(|| format!("{word} needs a value"))?;
                flags.push((word, Some(value)));
            }
            "--worktree" | "--page" | "--environment" => {
                return Err(format!(
                    "{word} is not taken: a session reaches the pages of its own worktree only"
                ));
            }
            word if word.starts_with("--") => return Err(format!("unknown option {word}")),
            word => positional.push(word),
        }
    }
    let flag = |name: &str| {
        flags
            .iter()
            .find(|(flag, _)| *flag == name)
            .map(|(_, value)| value.unwrap_or_default().to_string())
    };
    let on = |name: &str| flags.iter().any(|(flag, _)| *flag == name);
    let required = |name: &str, first: Option<&&str>| {
        flag(name)
            .or_else(|| first.map(|word| word.to_string()))
            .ok_or_else(|| format!("{name} is needed"))
    };
    let (verb, rest) = positional.split_first().ok_or("say what to do")?;
    let command = match (*verb, rest) {
        ("goto", rest) => BrowserCommand::Goto {
            url: required("--url", rest.first())?,
        },
        ("snapshot", _) => BrowserCommand::Snapshot {
            interactive: on("--interactive"),
            delta: on("--delta"),
            selector: flag("--selector"),
            depth: flag("--depth")
                .map(|depth| {
                    depth
                        .parse()
                        .map_err(|_| "--depth takes a number".to_string())
                })
                .transpose()?,
        },
        ("click", rest) => BrowserCommand::Click {
            element: required("--element", rest.first())?,
        },
        ("fill", rest) => BrowserCommand::Fill {
            element: required("--element", rest.first())?,
            value: flag("--value").ok_or("--value is needed")?,
        },
        ("screenshot", _) => BrowserCommand::Screenshot,
        ("wait", _) => BrowserCommand::Wait {
            selector: flag("--selector"),
            state: flag("--state")
                .map(|state| match state.as_str() {
                    "visible" => Ok(ElementState::Visible),
                    "hidden" => Ok(ElementState::Hidden),
                    _ => Err("--state is visible or hidden".to_string()),
                })
                .transpose()?,
            text: flag("--text"),
            url: flag("--url"),
            load: flag("--load")
                .map(|load| match load.as_str() {
                    "load" => Ok(LoadState::Load),
                    "domcontentloaded" => Ok(LoadState::DomContentLoaded),
                    "networkidle" => Ok(LoadState::NetworkIdle),
                    _ => Err("--load is load, domcontentloaded or networkidle".to_string()),
                })
                .transpose()?,
            timeout: flag("--timeout")
                .map(|timeout| {
                    timeout
                        .parse::<u64>()
                        .map_err(|_| "--timeout takes milliseconds".to_string())
                })
                .transpose()?,
        },
        ("console", _) => BrowserCommand::Console {
            limit: flag("--limit")
                .map(|limit| positive("--limit", &limit))
                .transpose()?,
        },
        ("network", _) => BrowserCommand::Network {
            limit: flag("--limit")
                .map(|limit| positive("--limit", &limit))
                .transpose()?,
        },
        ("capture", [action, ..]) => match *action {
            "start" => BrowserCommand::CaptureStart,
            "stop" => BrowserCommand::CaptureStop,
            other => return Err(format!("no capture command {other}")),
        },
        ("capture", []) => return Err("say capture start or stop".into()),
        ("set", [action, ..]) => match *action {
            "device" => BrowserCommand::SetDevice {
                name: flag("--name").ok_or("--name is needed")?,
            },
            other => return Err(format!("no set command {other}")),
        },
        ("set", []) => return Err("say set device".into()),
        ("full-screenshot", _) => BrowserCommand::FullScreenshot {
            format: flag("--format")
                .map(|format| match format.as_str() {
                    "png" => Ok(ImageFormat::Png),
                    "jpeg" => Ok(ImageFormat::Jpeg),
                    _ => Err("--format is png or jpeg".to_string()),
                })
                .transpose()?,
        },
        ("pdf", _) => BrowserCommand::Pdf,
        ("tab", [action, ..]) => match *action {
            "list" => BrowserCommand::TabList,
            "create" => BrowserCommand::TabCreate { url: flag("--url") },
            "switch" => BrowserCommand::TabSwitch {
                index: flag("--index")
                    .or_else(|| rest.get(1).map(|word| word.to_string()))
                    .ok_or("--index is needed")?
                    .parse()
                    .map_err(|_| "--index takes a number".to_string())?,
            },
            other => return Err(format!("no tab command {other}")),
        },
        ("tab", []) => return Err("say tab list, create or switch".into()),
        (other, _) => return Err(format!("no browser command {other}")),
    };
    command.check()?;
    Ok(Asked { command, json })
}

/// How the answer is written: a text as it reads, anything else as JSON.
fn written(answer: &Value, json: bool) -> String {
    match answer {
        Value::String(text) if !json => text.clone(),
        answer => serde_json::to_string_pretty(answer).unwrap_or_default(),
    }
}

/// `tsumugi browser <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    let asked = match parse(args) {
        Ok(asked) => asked,
        Err(text) => {
            eprintln!("tsumugi: {text}\n{USAGE}");
            return 2;
        }
    };
    let (Ok(credential), Ok(root)) = (std::env::var(TOKEN_ENV), std::env::var(ROOT_ENV)) else {
        eprintln!("tsumugi: browser commands act as a session of Tsumugi; run this inside one");
        return 2;
    };
    let answered = crate::cli::block_on(ask(
        std::path::Path::new(&root),
        &credential,
        Command::Browser(asked.command),
        None,
    ));
    match answered {
        Ok(Ok(answer)) => {
            println!("{}", written(&answer, asked.json));
            0
        }
        Ok(Err(error)) => {
            eprintln!("tsumugi: {error}");
            1
        }
        Err(error) => {
            eprintln!("tsumugi: {error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(words: &[&str]) -> Result<Asked, String> {
        parse(&words.iter().map(|w| w.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_commands_orca_has_are_read_by_the_flags_orca_gives_them() {
        let command = |words: &[&str]| parsed(words).unwrap().command;
        assert_eq!(
            command(&["goto", "--url", "http://localhost:3000"]),
            BrowserCommand::Goto {
                url: "http://localhost:3000".into()
            }
        );
        assert_eq!(
            command(&[
                "snapshot",
                "--interactive",
                "--depth",
                "3",
                "--selector",
                "form"
            ]),
            BrowserCommand::Snapshot {
                interactive: true,
                delta: false,
                selector: Some("form".into()),
                depth: Some(3)
            }
        );
        assert_eq!(
            command(&["click", "--element", "@e3"]),
            BrowserCommand::Click {
                element: "@e3".into()
            }
        );
        assert_eq!(
            command(&["fill", "--element", "@e1", "--value", "user@example.com"]),
            BrowserCommand::Fill {
                element: "@e1".into(),
                value: "user@example.com".into()
            }
        );
        assert_eq!(command(&["screenshot"]), BrowserCommand::Screenshot);
        assert_eq!(command(&["tab", "list"]), BrowserCommand::TabList);
        assert_eq!(
            command(&["tab", "create", "--url", "http://localhost:3000"]),
            BrowserCommand::TabCreate {
                url: Some("http://localhost:3000".into())
            }
        );
        assert_eq!(
            command(&["tab", "switch", "--index", "1"]),
            BrowserCommand::TabSwitch { index: 1 }
        );
        assert!(parsed(&["tab", "list", "--json"]).unwrap().json);
    }

    #[test]
    fn the_second_phase_commands_are_read_by_the_flags_orca_gives_them() {
        let command = |words: &[&str]| parsed(words).unwrap().command;
        assert_eq!(
            command(&[
                "wait",
                "--selector",
                "#done",
                "--state",
                "hidden",
                "--text",
                "Welcome",
                "--url",
                "**/dashboard",
                "--load",
                "networkidle",
                "--timeout",
                "5000"
            ]),
            BrowserCommand::Wait {
                selector: Some("#done".into()),
                state: Some(ElementState::Hidden),
                text: Some("Welcome".into()),
                url: Some("**/dashboard".into()),
                load: Some(LoadState::NetworkIdle),
                timeout: Some(5000),
            }
        );
        // With no condition, a wait is a pause.
        assert_eq!(
            command(&["wait", "--timeout", "250"]),
            BrowserCommand::Wait {
                selector: None,
                state: None,
                text: None,
                url: None,
                load: None,
                timeout: Some(250),
            }
        );
        assert_eq!(
            command(&["console", "--limit", "20"]),
            BrowserCommand::Console { limit: Some(20) }
        );
        assert_eq!(
            command(&["network"]),
            BrowserCommand::Network { limit: None }
        );
        assert_eq!(command(&["capture", "start"]), BrowserCommand::CaptureStart);
        assert_eq!(command(&["capture", "stop"]), BrowserCommand::CaptureStop);
        assert_eq!(
            command(&["set", "device", "--name", "Mobile M"]),
            BrowserCommand::SetDevice {
                name: "Mobile M".into()
            }
        );
        assert_eq!(
            command(&["full-screenshot", "--format", "jpeg"]),
            BrowserCommand::FullScreenshot {
                format: Some(ImageFormat::Jpeg)
            }
        );
        assert_eq!(
            command(&["full-screenshot"]),
            BrowserCommand::FullScreenshot { format: None }
        );
        assert_eq!(command(&["pdf"]), BrowserCommand::Pdf);
    }

    #[test]
    fn a_wait_cannot_run_the_agents_own_script_in_the_page() {
        let failed = parsed(&["wait", "--fn", "document.title == 'x'"]).unwrap_err();
        assert!(failed.contains("--fn"), "{failed}");
        assert!(failed.contains("password"), "{failed}");
    }

    #[test]
    fn the_new_commands_refuse_what_they_cannot_read() {
        for words in [
            &["wait", "--load", "idle"][..],
            &["wait", "--state", "gone"],
            &["wait", "--timeout", "soon"],
            &["wait", "--timeout", "999999999"],
            &["console", "--limit", "0"],
            &["console", "--limit", "many"],
            &["network", "--limit", "-1"],
            &["capture"],
            &["capture", "pause"],
            &["set"],
            &["set", "device"],
            &["set", "offline"],
            &["full-screenshot", "--format", "gif"],
        ] {
            assert!(parsed(words).is_err(), "{words:?}");
        }
    }

    #[test]
    fn a_command_that_names_a_worktree_or_a_page_of_another_session_is_refused() {
        for words in [
            &["goto", "--url", "x", "--worktree", "active"][..],
            &["snapshot", "--worktree", "/other"],
            &["click", "--element", "@e1", "--page", "3"],
        ] {
            let failed = parsed(words).unwrap_err();
            assert!(failed.contains("own worktree"), "{failed}");
        }
    }

    #[test]
    fn what_is_missing_or_unknown_is_said_before_anything_is_asked() {
        for words in [
            &[][..],
            &["goto"],
            &["click"],
            &["fill", "--element", "@e1"],
            &["tab"],
            &["tab", "switch"],
            &["tab", "switch", "--index", "x"],
            &["snapshot", "--depth", "deep"],
            &["snapshot", "--frobnicate"],
            &["eval", "document.cookie"],
        ] {
            assert!(parsed(words).is_err(), "{words:?}");
        }
    }

    #[test]
    fn a_text_answer_is_written_as_it_reads_unless_json_is_asked_for() {
        let text = Value::String("Page: a\n- link".into());
        assert_eq!(written(&text, false), "Page: a\n- link");
        assert_eq!(written(&text, true), "\"Page: a\\n- link\"");
        assert!(written(&serde_json::json!({"tabs": []}), false).contains("\"tabs\""));
    }
}
