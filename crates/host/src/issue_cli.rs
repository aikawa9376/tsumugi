//! `tsumugi issue`: save, read and change local issues from a command, through
//! the running work host that owns the queue.

use std::path::{Path, PathBuf};

use serde_json::json;
use tsumugi_model::{LocalIssue, LocalIssueAction, LocalIssueState, NewLocalIssue, TryAs};

use crate::command::Output;
use crate::delegation::{Code, Error};
use crate::watch::Watched;

const USAGE: &str = "usage: tsumugi issue <list [--state <state>] [--dir <path>] | show <issue> | create --title <title> [--body <text|->] [--dir <path>] | start <issue> [--attempt <profile>[:<model>]]... | cancel <issue> | retry <issue> | close <issue> | reopen <issue> | delete <issue>> [--json] [--instance <name>]";

/// What `tsumugi issue` was asked.
#[derive(Debug, PartialEq, Eq)]
struct Asked {
    json: bool,
    what: What,
}

#[derive(Debug, PartialEq, Eq)]
enum What {
    List {
        state: Option<LocalIssueState>,
        dir: Option<PathBuf>,
    },
    Show(String),
    Create {
        title: String,
        /// `-` is the standard input.
        body: Option<String>,
        dir: Option<PathBuf>,
    },
    Act(LocalIssueAction, String),
    /// Start the issue as one attempt for each of these.
    Try(String, Vec<TryAs>),
}

fn parse(args: &[String]) -> Option<Asked> {
    let (verb, mut rest) = args.split_first()?;
    let (mut json, mut state, mut dir, mut title, mut body) = (false, None, None, None, None);
    let mut selector = None;
    let mut tries = Vec::new();
    while let Some((word, after)) = rest.split_first() {
        rest = after;
        let mut value = || {
            let (value, after) = rest.split_first()?;
            rest = after;
            Some(value.clone())
        };
        match (verb.as_str(), word.as_str()) {
            (_, "--json") if !json => json = true,
            (_, "--instance") => {
                value().filter(|name| crate::paths::is_instance(name))?;
            }
            ("list", "--state") if state.is_none() => {
                state = Some(LocalIssueState::parse(&value()?)?);
            }
            ("list" | "create", "--dir") if dir.is_none() => dir = Some(PathBuf::from(value()?)),
            ("create", "--title") if title.is_none() => title = Some(value()?),
            ("create", "--body") if body.is_none() => body = Some(value()?),
            ("start", "--attempt") => tries.push(TryAs::parse(&value()?)?),
            (_, name) if name.starts_with("--") => return None,
            (
                "show" | "start" | "cancel" | "retry" | "close" | "reopen" | "delete",
                selector_word,
            ) if selector.is_none() => {
                selector = Some(selector_word.to_owned());
            }
            _ => return None,
        }
    }
    let what = match verb.as_str() {
        "list" => What::List { state, dir },
        "create" => What::Create {
            title: title?,
            body,
            dir,
        },
        "show" => What::Show(selector?),
        "start" if !tries.is_empty() => What::Try(selector?, tries),
        word => What::Act(LocalIssueAction::parse(word)?, selector?),
    };
    Some(Asked { json, what })
}

/// `tsumugi issue <args>`. Returns the process exit code.
pub fn cli(args: &[String]) -> i32 {
    let Some(asked) = parse(args) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let input = match &asked.what {
        What::Create { body, .. } if body.as_deref() == Some("-") => {
            let mut text = String::new();
            if let Err(error) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut text) {
                eprintln!("tsumugi: cannot read the description: {error}");
                return 1;
            }
            Some(text)
        }
        _ => None,
    };
    let mut ask = vec!["issue".to_owned()];
    ask.extend(args.iter().cloned());
    crate::work_host::run_command(ask, input)
}

/// `tsumugi issue <args>` as the work host runs it for a command run in `cwd`
/// with `input` on its standard input.
pub(crate) async fn run(
    watched: &Watched,
    args: &[String],
    cwd: &Path,
    input: Option<String>,
) -> Output {
    let Some(asked) = parse(args) else {
        return Output {
            wrote: Err(USAGE.into()),
            code: 2,
            effects_possible: false,
        };
    };
    let effects_possible = !matches!(asked.what, What::List { .. } | What::Show(_));
    let json = asked.json;
    match carry_out(watched, asked.what, cwd, input).await {
        Ok(shown) => Output {
            wrote: Ok(shown.render(json)),
            code: 0,
            effects_possible,
        },
        Err(error) if json => Output {
            wrote: Ok(
                json!({"ok": false, "error": {"code": error.code, "message": error.message}})
                    .to_string(),
            ),
            code: 1,
            effects_possible,
        },
        Err(error) => Output {
            wrote: Err(error.message),
            code: 1,
            effects_possible,
        },
    }
}

/// What a command shows once it is done.
enum Shown {
    Issues(Vec<(u64, LocalIssue)>),
    One(u64, LocalIssue),
    Changed(LocalIssueAction, u64, LocalIssue),
}

async fn carry_out(
    watched: &Watched,
    what: What,
    cwd: &Path,
    input: Option<String>,
) -> Result<Shown, Error> {
    use crate::delegation::app::{root, with_store};
    match what {
        What::List { state, dir } => {
            let dir = dir.map(|dir| resolved(cwd, &dir));
            let all = with_store(&root(), |store| store.numbered_local_issues()).await?;
            Ok(Shown::Issues(
                all.into_iter()
                    .filter(|(_, issue)| state.is_none_or(|state| issue.state == state))
                    .filter(|(_, issue)| dir.as_ref().is_none_or(|dir| issue.dir.starts_with(dir)))
                    .collect(),
            ))
        }
        What::Show(selector) => {
            let (number, issue) =
                with_store(&root(), move |store| store.local_issue_named(&selector)).await?;
            Ok(Shown::One(number, issue))
        }
        What::Create { title, body, dir } => {
            let body = match body.as_deref() {
                Some("-") => input.unwrap_or_default(),
                Some(text) => text.to_owned(),
                None => String::new(),
            };
            let new = NewLocalIssue {
                id: tsumugi_model::SessionId::new().to_string(),
                dir: resolved(cwd, &dir.unwrap_or_default()),
                title,
                body,
            };
            let id = new.id.clone();
            crate::local_issues::create(watched, new)
                .await
                .map_err(|message| Error::new(Code::Invalid, message))?;
            let (number, issue) =
                with_store(&root(), move |store| store.local_issue_named(&id)).await?;
            Ok(Shown::One(number, issue))
        }
        What::Act(action, selector) => {
            let (number, issue) = crate::local_issues::act(watched, &selector, action).await?;
            Ok(Shown::Changed(action, number, issue))
        }
        What::Try(selector, tries) => {
            let (number, issue) = crate::local_issues::try_as(watched, selector, tries)
                .await
                .map_err(|message| Error::new(Code::Refused, message))?;
            Ok(Shown::One(number, issue))
        }
    }
}

/// `dir` as a directory of this machine, taken from `cwd` when relative.
fn resolved(cwd: &Path, dir: &Path) -> PathBuf {
    let dir = cwd.join(dir);
    tsumugi_sys::fs::canonicalize(&dir).unwrap_or(dir)
}

impl Shown {
    fn render(&self, json: bool) -> String {
        let view = crate::local_issues::view;
        if json {
            let result = match self {
                Self::Issues(all) => {
                    json!({"issues": all.iter().map(|(n, i)| view(*n, i)).collect::<Vec<_>>()})
                }
                Self::One(number, issue) => json!({"issue": view(*number, issue)}),
                Self::Changed(action, number, issue) => {
                    json!({"issue": view(*number, issue), "deleted": *action == LocalIssueAction::Delete})
                }
            };
            return json!({"ok": true, "result": result}).to_string();
        }
        match self {
            Self::Issues(all) if all.is_empty() => "No local issues".into(),
            Self::Issues(all) => all
                .iter()
                .map(|(number, issue)| line(*number, issue))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::One(number, issue) => detail(*number, issue),
            Self::Changed(LocalIssueAction::Delete, number, issue) => {
                format!("#{number} deleted  {}", issue.title)
            }
            Self::Changed(_, number, issue) => line(*number, issue),
        }
    }
}

fn line(number: u64, issue: &LocalIssue) -> String {
    format!(
        "#{number}  {:<9}  {}  {}",
        issue.state.word(),
        issue.dir.display(),
        issue.title
    )
}

fn detail(number: u64, issue: &LocalIssue) -> String {
    let mut text = format!(
        "#{number} {}\nstate: {}\ndir: {}\nid: {}",
        issue.title,
        issue.state.word(),
        issue.dir.display(),
        issue.id
    );
    if let Some(session) = &issue.session {
        text.push_str(&format!("\nsession: {session}"));
    }
    if let Some(error) = &issue.error {
        text.push_str(&format!("\nerror: {error}"));
    }
    for attempt in &issue.attempts {
        text.push_str(&format!(
            "\nattempt {}: {:<9}  {}",
            attempt.number,
            attempt.state.word(),
            crate::local_issues::tried_as_word(&attempt.tried_as)
        ));
        if let Some(worktree) = &attempt.worktree {
            text.push_str(&format!("  {}", worktree.display()));
        }
        if let Some(error) = &attempt.error {
            text.push_str(&format!("  ({error})"));
        }
    }
    if !issue.body.is_empty() {
        text.push_str(&format!("\n\n{}", issue.body));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn asked(words: &[&str]) -> Option<What> {
        let args: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        parse(&args).map(|asked| asked.what)
    }

    fn saved(state: LocalIssueState) -> LocalIssue {
        LocalIssue {
            id: "ab12".to_owned() + &"0".repeat(28),
            dir: "/project".into(),
            title: "Fix it".into(),
            body: "Details".into(),
            state,
            record: None,
            session: Some("0f".into()),
            error: Some("It failed".into()),
            schedule: None,
            attempts: Vec::new(),
        }
    }

    #[test]
    fn every_action_names_an_issue_and_nothing_more() {
        for action in LocalIssueAction::ALL {
            assert_eq!(
                asked(&[action.word(), "3"]),
                Some(What::Act(action, "3".into()))
            );
            assert_eq!(asked(&[action.word()]), None);
            assert_eq!(asked(&[action.word(), "3", "4"]), None);
        }
        assert_eq!(asked(&["show", "ab12"]), Some(What::Show("ab12".into())));
    }

    #[test]
    fn start_with_attempts_tries_the_issue_once_for_each_time_one_is_given() {
        let tried = |word: &str| TryAs::parse(word).unwrap();
        assert_eq!(
            asked(&[
                "start",
                "3",
                "--attempt",
                "claude:opus",
                "--attempt",
                "codex"
            ]),
            Some(What::Try(
                "3".into(),
                vec![tried("claude:opus"), tried("codex")]
            ))
        );
        assert_eq!(
            asked(&["start", "--attempt", "codex", "--attempt", "codex", "3"]),
            Some(What::Try("3".into(), vec![tried("codex"), tried("codex")]))
        );
        assert_eq!(asked(&["start", "3", "--attempt", ":opus"]), None);
        assert_eq!(asked(&["retry", "3", "--attempt", "codex"]), None);
    }

    #[test]
    fn a_listing_is_narrowed_by_state_and_directory() {
        assert_eq!(
            asked(&["list", "--state", "failed", "--dir", "api"]),
            Some(What::List {
                state: Some(LocalIssueState::Failed),
                dir: Some("api".into())
            })
        );
        assert_eq!(asked(&["list", "--state", "stuck"]), None);
        assert_eq!(asked(&["list", "extra"]), None);
    }

    #[test]
    fn an_issue_is_created_from_a_title_with_a_description_from_text_or_standard_input() {
        assert_eq!(
            asked(&["create", "--title", "Fix", "--body", "-", "--dir", "."]),
            Some(What::Create {
                title: "Fix".into(),
                body: Some("-".into()),
                dir: Some(".".into())
            })
        );
        assert_eq!(asked(&["create", "--body", "text"]), None);
    }

    #[test]
    fn the_output_format_and_the_instance_come_anywhere_and_an_unknown_option_is_refused() {
        let args: Vec<String> = ["--json", "list", "--instance", "work"]
            .map(String::from)
            .to_vec();
        assert_eq!(parse(&args), None, "the verb comes first");
        let args: Vec<String> = ["list", "--instance", "work", "--json"]
            .map(String::from)
            .to_vec();
        assert!(parse(&args).is_some_and(|asked| asked.json));
        assert_eq!(asked(&["list", "--instance"]), None);
        assert_eq!(asked(&["list", "--verbose"]), None);
        assert_eq!(asked(&["remove", "1"]), None);
        assert_eq!(asked(&[]), None);
    }

    #[test]
    fn issues_are_listed_one_to_a_line_and_shown_with_their_request() {
        let all = vec![(3, saved(LocalIssueState::Failed))];
        assert_eq!(
            Shown::Issues(all.clone()).render(false),
            "#3  failed     /project  Fix it"
        );
        assert_eq!(Shown::Issues(vec![]).render(false), "No local issues");
        let shown = Shown::One(3, saved(LocalIssueState::Failed)).render(false);
        assert!(shown.starts_with("#3 Fix it\nstate: failed\ndir: /project\nid: ab12"));
        assert!(shown.contains("session: 0f") && shown.contains("error: It failed"));
        assert!(shown.ends_with("\n\nDetails"));
        assert_eq!(
            Shown::Changed(LocalIssueAction::Delete, 3, saved(LocalIssueState::Closed))
                .render(false),
            "#3 deleted  Fix it"
        );
    }

    #[test]
    fn json_wraps_a_result_the_way_a_scripts_reads_it() {
        let all = vec![(3, saved(LocalIssueState::Waiting))];
        let listed: Value = serde_json::from_str(&Shown::Issues(all).render(true)).unwrap();
        assert_eq!(listed["ok"], true);
        let issue = &listed["result"]["issues"][0];
        assert_eq!(issue["number"], 3);
        assert_eq!(issue["state"], "waiting");
        assert_eq!(issue["dir"], "/project");
        assert_eq!(issue["session"], "0f");
        assert_eq!(issue["body"], "Details");
        let deleted: Value = serde_json::from_str(
            &Shown::Changed(LocalIssueAction::Delete, 3, saved(LocalIssueState::Closed))
                .render(true),
        )
        .unwrap();
        assert_eq!(deleted["result"]["deleted"], true);
    }
}
