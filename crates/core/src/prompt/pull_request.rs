//! Writing a pull request in the prompt: the first line is the title, what
//! follows is the body, and the base branch and whether it is a draft are
//! header lines above them, which sending reads back.

use std::path::PathBuf;

use tsumugi_model::EnvironmentId;
use tsumugi_model::github::Listing;
use tsumugi_protocol::{GitOp, Typing};

use super::send::Taken;
use crate::content::{Area, Tab, shows_prompt};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::tree::{self, nav::Item};
use crate::update::step;

/// A pull request the prompt writes the title and body of.
#[derive(Debug, Clone, PartialEq)]
pub struct Pulling {
    pub environment: EnvironmentId,
    pub worktree: PathBuf,
}

/// What a written pull request says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields {
    /// The branch it merges into; the repository's default when none.
    pub base: Option<String>,
    pub draft: bool,
    pub title: String,
    pub body: String,
}

/// The text the prompt starts from: the headers, then `message` as the commit
/// it comes from was written.
pub fn template(message: &str) -> String {
    format!("Base: \nDraft: no\n\n{}", message.trim())
}

/// The header a line is, as its lower-cased name and its value.
fn header(line: &str) -> Option<(String, &str)> {
    let (name, value) = line.split_once(':')?;
    let name = name.trim().to_lowercase();
    matches!(name.as_str(), "base" | "draft").then(|| (name, value.trim()))
}

/// What was written read as a pull request: header lines, which are
/// optional, up to a blank line; then the title, and the body after it.
pub fn parse(text: &str) -> Result<Fields, String> {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    let headers = lines.iter().take_while(|l| header(l).is_some()).count();
    let has_headers = headers > 0 && lines.get(headers).is_none_or(|l| l.trim().is_empty());
    let (mut base, mut draft, rest) =
        (None, false, &lines[if has_headers { headers } else { 0 }..]);
    if has_headers {
        for line in &lines[..headers] {
            match header(line) {
                Some((name, value)) if name == "base" => {
                    base = Some(value.to_string()).filter(|v| !v.is_empty());
                }
                Some((_, value)) => {
                    draft = match value.to_lowercase().as_str() {
                        "yes" | "y" | "true" | "on" => true,
                        "no" | "n" | "false" | "off" | "" => false,
                        other => return Err(format!("Draft is yes or no, not {other:?}")),
                    };
                }
                None => {}
            }
        }
    }
    let mut rest = rest.iter().skip_while(|l| l.trim().is_empty());
    let title = rest.next().map(|l| l.trim()).unwrap_or_default();
    if title.is_empty() {
        return Err("Write the title of the pull request on its first line".into());
    }
    let body = rest.copied().collect::<Vec<_>>().join("\n");
    Ok(Fields {
        base,
        draft,
        title: title.into(),
        body: body.trim().into(),
    })
}

/// Give the prompt the keys to write a pull request for the branch `worktree`
/// has checked out, which is sent from it as any prompt is. It starts from
/// the last commit's message. A branch that has a pull request open already
/// is refused with its number; where no prompt is shown there is nowhere to
/// write it.
pub(crate) fn begin(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
) -> Vec<Effect> {
    let work = state.hosts.work(&environment);
    if let Some((_, number)) = crate::details::branch_pull(&state.details, work) {
        return vec![Effect::Notice(format!(
            "This branch has pull request #{number} open already"
        ))];
    }
    if !shows_prompt(state.content.active()) {
        return vec![Effect::Notice(
            "Open a session or a draft to write the pull request in its prompt".into(),
        )];
    }
    let last = crate::details::working_copy(&state.details, work)
        .and_then(|view| view.revision)
        .map(|revision| revision.description)
        .unwrap_or_default();
    state.prompt.committing = None;
    state.prompt.basing = None;
    state.prompt.pulling = Some(Pulling {
        environment,
        worktree,
    });
    let mut effects = step(state, Msg::Focus(Area::Prompt));
    effects.push(Effect::PromptRestore {
        tab: state.content.active().cloned(),
        text: template(&last),
        images: Vec::new(),
    });
    effects.push(Effect::Notice(
        "Write the pull request: title on the first line; sending pushes the branch and opens it, leaving the prompt gives up".into(),
    ));
    effects
}

/// What was taken from the prompt, for the pull request `pulling` writes:
/// asked of the work host, or put back with what is wrong with it.
pub(crate) fn taken(state: &mut ViewState, pulling: Pulling, taken: Taken) -> Vec<Effect> {
    match parse(&taken.text) {
        Ok(fields) => {
            state.prompt.sending = Some(super::send::Sending::Handed(taken));
            vec![Effect::Git {
                environment: pulling.environment,
                worktree: pulling.worktree,
                op: GitOp::CreatePull {
                    base: fields.base,
                    title: fields.title,
                    body: fields.body,
                    draft: fields.draft,
                },
                prompt: true,
            }]
        }
        Err(wrong) => {
            state.prompt.sending = None;
            vec![
                Effect::PromptRestore {
                    tab: taken.tab,
                    text: taken.text,
                    images: taken.images,
                },
                Effect::Notice(wrong),
            ]
        }
    }
}

/// The pull request `number` was opened for the branch of `worktree`: shown
/// in its tab.
pub(crate) fn created(
    state: &mut ViewState,
    environment: EnvironmentId,
    worktree: PathBuf,
    number: u64,
) -> Vec<Effect> {
    let work = state.hosts.work(&environment);
    let root = tree::listed(work)
        .workspace_of(&worktree)
        .map_or(worktree, |w| w.root.clone());
    let mut effects = vec![Effect::Notice(format!("Opened pull request #{number}"))];
    effects.extend(step(
        state,
        Msg::OpenRow {
            item: Item::GitHubItem(environment, root, Listing::Pulls, number),
            how: None,
            take_keys: true,
        },
    ));
    effects
}

/// What the focused session's agent is asked to write.
const ASK: &str = "Write the title and the body of a pull request for this branch, from its commits and its changes against the base branch. Answer with the title on the first line and the body after a blank line, and nothing else. Do not create the pull request.";

/// Ask the agent of the session shown to draft the title and body, to be
/// copied into the prompt `begin` gives.
pub(crate) fn ask_agent(state: &mut ViewState) -> Vec<Effect> {
    match state.content.active().cloned() {
        Some(tab @ Tab::Session(..)) => vec![
            Effect::TypeInto {
                tab,
                typing: vec![
                    Typing::Paste { text: ASK.into() },
                    Typing::Key {
                        name: "Enter".into(),
                    },
                ],
            },
            Effect::Notice("Asked the agent to draft the pull request".into()),
        ],
        _ => vec![Effect::Notice(
            "Show a session to ask its agent to draft the pull request".into(),
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_is_the_title_and_the_rest_is_the_body() {
        let fields = parse("core: add it\n\nWhy.\n\n- one\n").unwrap();
        assert_eq!(
            fields,
            Fields {
                base: None,
                draft: false,
                title: "core: add it".into(),
                body: "Why.\n\n- one".into(),
            }
        );
    }

    #[test]
    fn headers_above_a_blank_line_set_the_base_and_the_draft() {
        let fields = parse("Base: develop\nDraft: yes\n\ncore: add it\nWhy.").unwrap();
        assert_eq!(fields.base.as_deref(), Some("develop"));
        assert!(fields.draft);
        assert_eq!(fields.title, "core: add it");
        assert_eq!(fields.body, "Why.");
    }

    #[test]
    fn an_empty_base_is_the_default_branch_and_headers_are_not_case_sensitive() {
        let fields = parse("base:\nDRAFT: No\n\nt").unwrap();
        assert_eq!(fields.base, None);
        assert!(!fields.draft);
    }

    #[test]
    fn a_header_line_without_a_blank_line_after_it_is_the_title() {
        let fields = parse("Base: fix\nbody").unwrap();
        assert_eq!(fields.base, None);
        assert_eq!(fields.title, "Base: fix");
        assert_eq!(fields.body, "body");
    }

    #[test]
    fn a_draft_that_is_neither_yes_nor_no_is_refused() {
        assert!(
            parse("Draft: maybe\n\nt")
                .unwrap_err()
                .contains("yes or no")
        );
    }

    #[test]
    fn a_pull_request_without_a_title_is_refused() {
        assert!(parse("").is_err());
        assert!(parse("Base: main\nDraft: no\n\n").is_err());
        assert!(parse("Base: main\n\n   \n").is_err());
    }

    #[test]
    fn the_template_starts_from_a_commit_message_under_the_headers() {
        let text = template("core: keep it\n\nWhy.\n");
        assert_eq!(text, "Base: \nDraft: no\n\ncore: keep it\n\nWhy.");
        let fields = parse(&text).unwrap();
        assert_eq!(fields.title, "core: keep it");
        assert_eq!(fields.body, "Why.");
        assert_eq!(fields.base, None);
    }
}
