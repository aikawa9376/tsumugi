//! `tsumugi gate` and `tsumugi cleanup`: ask the work host to check the
//! repository's latest commit with its gate, and to list and clear away the
//! worktrees that are done with. The same asks the `request_gate` and
//! `cleanup_worktrees` tools make for a session.

use std::path::{Path, PathBuf};

use crate::command::Output;

const GATE_USAGE: &str = "usage: tsumugi gate [--dir <path>] [--wait <seconds>]";
const CLEANUP_USAGE: &str = "usage: tsumugi cleanup [--dir <path>] [--remove]";

/// What `tsumugi gate` or `tsumugi cleanup` was asked.
#[derive(Debug, Default, PartialEq, Eq)]
struct Asked {
    dir: Option<PathBuf>,
    wait: Option<u64>,
    remove: bool,
}

fn parse(verb: &str, args: &[String]) -> Option<Asked> {
    let mut asked = Asked::default();
    let mut rest = args;
    while let Some((word, after)) = rest.split_first() {
        rest = after;
        let mut value = || {
            let (value, after) = rest.split_first()?;
            rest = after;
            Some(value.clone())
        };
        match (verb, word.as_str()) {
            (_, "--dir") if asked.dir.is_none() => asked.dir = Some(PathBuf::from(value()?)),
            ("gate", "--wait") if asked.wait.is_none() => asked.wait = Some(value()?.parse().ok()?),
            ("cleanup", "--remove") if !asked.remove => asked.remove = true,
            _ => return None,
        }
    }
    Some(asked)
}

fn usage(verb: &str) -> &'static str {
    if verb == "gate" {
        GATE_USAGE
    } else {
        CLEANUP_USAGE
    }
}

/// `tsumugi gate` or `tsumugi cleanup`, run in the work host of the instance.
pub fn cli(verb: &str, args: &[String]) -> i32 {
    if parse(verb, args).is_none() {
        eprintln!("{}", usage(verb));
        return 2;
    }
    let mut ask = vec![verb.to_string()];
    ask.extend(args.iter().cloned());
    crate::work_host::run_command(ask, None)
}

/// The command as the work host runs it for a command run in `cwd`.
pub(crate) async fn run(verb: &str, args: &[String], cwd: &Path) -> Output {
    let Some(asked) = parse(verb, args) else {
        return Output {
            wrote: Err(usage(verb).into()),
            code: 2,
            effects_possible: false,
        };
    };
    let dir = asked.dir.map_or_else(
        || cwd.to_path_buf(),
        |dir| {
            if dir.is_absolute() {
                dir
            } else {
                cwd.join(dir)
            }
        },
    );
    let daemon = crate::daemon::daemon();
    let settings = crate::settings::current();
    let (told, code) = if verb == "gate" {
        match crate::gate::ask(&daemon, settings, &dir, asked.wait).await {
            Ok(told) => {
                let code = match told["state"].as_str() {
                    Some("done") if told["report"]["passed"] == true => 0,
                    Some("queued" | "running") => 3,
                    _ => 1,
                };
                (Ok(told), code)
            }
            Err(error) => (Err(error.message), 1),
        }
    } else {
        match crate::cleanup::ask(&daemon, settings, &dir, asked.remove).await {
            Ok(told) => (Ok(told), 0),
            Err(error) => (Err(error.message), 1),
        }
    };
    Output {
        wrote: told.map(|told| serde_json::to_string_pretty(&told).unwrap_or_default()),
        code,
        effects_possible: verb == "gate" || asked.remove,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_gate_asks_for_a_directory_and_how_long_to_wait() {
        assert_eq!(parse("gate", &[]), Some(Asked::default()));
        assert_eq!(
            parse("gate", &words(&["--dir", "api", "--wait", "30"])),
            Some(Asked {
                dir: Some("api".into()),
                wait: Some(30),
                remove: false
            })
        );
    }

    #[test]
    fn a_cleanup_only_lists_unless_it_is_told_to_remove() {
        assert!(!parse("cleanup", &[]).unwrap().remove);
        assert!(parse("cleanup", &words(&["--remove"])).unwrap().remove);
    }

    #[test]
    fn what_a_command_does_not_take_is_refused() {
        assert_eq!(parse("gate", &words(&["--remove"])), None);
        assert_eq!(parse("cleanup", &words(&["--wait", "3"])), None);
        assert_eq!(parse("gate", &words(&["--wait", "soon"])), None);
        assert_eq!(parse("gate", &words(&["--dir"])), None);
        assert_eq!(parse("gate", &words(&["--now"])), None);
    }
}
