//! Writing on the pull request or the issue a GitHub tab shows, in the
//! prompt under it: what is sent from there is a new comment, unless the
//! keys began a reply, a reaction or a review, which the first lines of the
//! text say how to read. Nothing is sent but by sending the prompt, and a
//! write that fails puts the text back.

use tsumugi_model::github::{Listing, Post, REACTIONS, Thread, Verdict, Write};

use crate::viewers::Github;

use super::send::Taken;
use crate::content::{Area, Tab, shows_prompt};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::state::ViewState;
use crate::update::step;

/// What the prompt written in a GitHub tab is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aim {
    /// A new comment on the conversation.
    Comment,
    /// A reply to a card: in the thread of lines it belongs to, or, where
    /// it belongs to none, a comment that quotes it.
    Reply {
        thread: Option<String>,
        quote: String,
    },
    /// A reaction to the post `subject` is the id of.
    React { subject: String },
    /// A review of the pull request.
    Review,
}

/// The text the prompt starts from, or none where it is left as it is.
pub fn template(aim: &Aim) -> Option<String> {
    match aim {
        Aim::Comment => None,
        Aim::Reply {
            thread: Some(_), ..
        } => None,
        Aim::Reply {
            thread: None,
            quote,
        } => {
            let quoted: Vec<String> = quote
                .trim()
                .lines()
                .map(|l| format!("> {l}").trim_end().to_string())
                .collect();
            (!quoted.is_empty()).then(|| format!("{}\n\n", quoted.join("\n")))
        }
        Aim::React { .. } => Some("React: ".into()),
        Aim::Review => Some("Review: comment\n\n".into()),
    }
}

/// A reaction named as GitHub names it or as its emoji.
fn reaction(word: &str) -> Option<&'static str> {
    let word = word.trim().trim_matches(':').to_lowercase();
    let emoji = [
        ("👍", "+1"),
        ("👎", "-1"),
        ("😄", "laugh"),
        ("😕", "confused"),
        ("❤️", "heart"),
        ("❤", "heart"),
        ("🎉", "hooray"),
        ("🚀", "rocket"),
        ("👀", "eyes"),
    ];
    REACTIONS
        .iter()
        .find(|r| **r == word)
        .or_else(|| emoji.iter().find(|(e, _)| *e == word).map(|(_, r)| r))
        .copied()
}

/// What was written, read for `aim` as what to write on GitHub, or what is
/// wrong with it.
pub fn parse(aim: &Aim, text: &str) -> Result<Write, String> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    match aim {
        Aim::Comment | Aim::Reply { thread: None, .. } => {
            let has_words = text
                .lines()
                .any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('>'));
            if !has_words {
                return Err("Write the comment; a quote alone says nothing".into());
            }
            Ok(Write::Comment { body: text.into() })
        }
        Aim::Reply {
            thread: Some(thread),
            ..
        } => {
            if text.is_empty() {
                return Err("Write the reply first".into());
            }
            Ok(Write::Reply {
                thread: thread.clone(),
                body: text.into(),
            })
        }
        Aim::React { subject } => {
            let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            let (add, word) = match line.split_once(':') {
                Some((head, word)) if head.trim().eq_ignore_ascii_case("react") => (true, word),
                Some((head, word)) if head.trim().eq_ignore_ascii_case("unreact") => (false, word),
                _ => (true, line),
            };
            let content = reaction(word).ok_or_else(|| {
                format!(
                    "Write the reaction after React: or Unreact:, one of {}",
                    REACTIONS.join(" ")
                )
            })?;
            Ok(Write::React {
                subject: subject.clone(),
                content: content.into(),
                add,
            })
        }
        Aim::Review => {
            let (head, body) = text.split_once('\n').unwrap_or((text, ""));
            let verdict = match head.split_once(':') {
                Some((name, verdict)) if name.trim().eq_ignore_ascii_case("review") => {
                    match verdict
                        .trim()
                        .to_lowercase()
                        .replace([' ', '_'], "-")
                        .as_str()
                    {
                        "approve" | "approved" => Verdict::Approve,
                        "request-changes" | "changes" | "changes-requested" => {
                            Verdict::RequestChanges
                        }
                        "comment" | "commented" => Verdict::Comment,
                        other => {
                            return Err(format!(
                                "Review is approve, request-changes or comment, not {other:?}"
                            ));
                        }
                    }
                }
                _ => {
                    return Err(
                        "Start the review with Review: approve, request-changes or comment".into(),
                    );
                }
            };
            Ok(Write::Review {
                verdict,
                body: body.trim().into(),
            })
        }
    }
}

/// What a key of a GitHub tab's cards comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Select the card with this number.
    Select(usize),
    /// Write in the prompt for this aim.
    Compose(Aim),
    /// Ask an agent to fix what a pull request's checks or review threads
    /// say.
    Fix(Box<super::fix_request::Fix>),
    /// Write at once.
    Write(Write),
    /// Write once the user has answered yes to the question about it.
    Ask(Write),
}

/// What `key` comes to in the tab of `thread`, with the card numbered
/// `selected` selected, or why it comes to nothing.
pub fn act(key: Github, thread: &Thread, selected: Option<usize>) -> Result<Act, String> {
    let posts: Vec<&Post> = thread.posts().collect();
    let last = posts.len().saturating_sub(1);
    let card = || {
        selected
            .and_then(|i| posts.get(i).copied())
            .ok_or_else(|| "Select a card first with n and N".to_string())
    };
    Ok(match key {
        Github::Next => Act::Select(selected.map_or(0, |i| (i + 1).min(last))),
        Github::Prev => Act::Select(selected.map_or(0, |i| i.saturating_sub(1).min(last))),
        Github::Comment => Act::Compose(Aim::Comment),
        Github::Review => Act::Compose(Aim::Review),
        Github::FixChecks | Github::FixThreads => Act::Fix(Box::new(super::fix_request::collect(
            key, thread, selected,
        )?)),
        Github::Reply => {
            let post = card()?;
            Act::Compose(Aim::Reply {
                thread: post.thread.as_ref().map(|t| t.id.clone()),
                quote: post.body.clone(),
            })
        }
        Github::Merge(_) | Github::AutoMerge | Github::Close | Github::Ready => {
            Act::Ask(crate::pull_change::plan(key, thread)?)
        }
        Github::Resolve => {
            let on = card()?
                .thread
                .as_ref()
                .ok_or("That card is not in a thread of lines, which is what is resolved")?;
            Act::Write(Write::Resolve {
                thread: on.id.clone(),
                resolved: !on.resolved,
            })
        }
        Github::React => {
            let post = card()?;
            if post.id.is_empty() {
                return Err("GitHub gave that card no id to react to; read it again".into());
            }
            Act::Compose(Aim::React {
                subject: post.id.clone(),
            })
        }
    })
}

/// Ask whether to do `write` on the pull request the GitHub tab shown shows;
/// nothing is written until the answer is yes.
pub(crate) fn ask(state: &mut ViewState, write: Write) -> Vec<Effect> {
    match subject(state.content.active()) {
        Some((environment, repository, Listing::Pulls, number)) => {
            state.overlays.confirm = Some(crate::overlay::confirm::Ask::PullWrite {
                environment,
                repository,
                number,
                write,
            });
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The pull request or the issue a GitHub tab shows, as the write is asked
/// of the work host.
fn subject(
    tab: Option<&Tab>,
) -> Option<(
    tsumugi_model::EnvironmentId,
    std::path::PathBuf,
    tsumugi_model::github::Listing,
    u64,
)> {
    match tab {
        Some(Tab::GitHub {
            environment,
            repository,
            listing,
            number,
        }) => Some((environment.clone(), repository.clone(), *listing, *number)),
        _ => None,
    }
}

/// Give the prompt under the GitHub tab shown the keys to write for `aim`.
pub(crate) fn begin(state: &mut ViewState, aim: Aim) -> Vec<Effect> {
    let tab = state.content.active().cloned();
    let Some((_, _, listing, _)) = subject(tab.as_ref()) else {
        return Vec::new();
    };
    if !shows_prompt(tab.as_ref()) {
        return Vec::new();
    }
    if aim == Aim::Review && listing != tsumugi_model::github::Listing::Pulls {
        return vec![Effect::Notice("Only a pull request is reviewed".into())];
    }
    let words = match &aim {
        Aim::Comment => "Write the comment; sending posts it, leaving the prompt gives it up",
        Aim::Reply { .. } => "Write the reply; sending posts it, leaving the prompt gives it up",
        Aim::React { .. } => {
            "Write the reaction: React: +1 -1 laugh confused heart hooray rocket eyes, or Unreact: to take one back"
        }
        Aim::Review => {
            "Write the review: Review: approve, request-changes or comment, then your words; sending submits it"
        }
    };
    let text = template(&aim);
    state.prompt.committing = None;
    state.prompt.pulling = None;
    state.prompt.basing = None;
    state.prompt.branching = None;
    state.prompt.writing = Some(aim);
    let mut effects = step(state, Msg::Focus(Area::Prompt));
    if let Some(text) = text {
        effects.push(Effect::PromptRestore {
            tab,
            text,
            images: Vec::new(),
        });
    }
    effects.push(Effect::Notice(words.into()));
    effects
}

/// What was taken from the prompt under a GitHub tab: written on GitHub as
/// the aim says, or put back with what is wrong with it.
pub(crate) fn taken(state: &mut ViewState, taken: Taken) -> Vec<Effect> {
    let aim = state.prompt.writing.clone().unwrap_or(Aim::Comment);
    let Some((environment, repository, listing, number)) = subject(taken.tab.as_ref()) else {
        state.prompt.sending = None;
        return vec![restore(taken)];
    };
    match parse(&aim, &taken.text) {
        Ok(write) => {
            state.prompt.sending = Some(super::send::Sending::Handed(taken));
            vec![Effect::GitHubWrite {
                environment,
                repository,
                listing,
                number,
                write,
                prompt: true,
            }]
        }
        Err(wrong) => {
            state.prompt.sending = None;
            vec![restore(taken), Effect::Notice(wrong)]
        }
    }
}

fn restore(taken: Taken) -> Effect {
    Effect::PromptRestore {
        tab: taken.tab,
        text: taken.text,
        images: taken.images,
    }
}

/// Write `write` on what the GitHub tab shown shows, at once: a key that
/// resolves a thread or marks a file viewed is the explicit act.
pub(crate) fn at_once(state: &mut ViewState, write: Write) -> Vec<Effect> {
    match subject(state.content.active()) {
        Some((environment, repository, listing, number)) => vec![Effect::GitHubWrite {
            environment,
            repository,
            listing,
            number,
            write,
            prompt: false,
        }],
        None => Vec::new(),
    }
}

/// `write` was done on the pull request or issue `number`: the window
/// showing it reads it again.
pub(crate) fn written(
    state: &mut ViewState,
    environment: tsumugi_model::EnvironmentId,
    repository: std::path::PathBuf,
    listing: tsumugi_model::github::Listing,
    number: u64,
) -> Vec<Effect> {
    let shown = Tab::GitHub {
        environment: environment.clone(),
        repository,
        listing,
        number,
    };
    match state.content.windows.focused_window() {
        Some(window) if state.content.active() == Some(&shown) => vec![Effect::Viewer {
            environment,
            window,
            action: crate::viewers::ViewerAction::Reload,
        }],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::github::Listing;

    fn thread_reply() -> Aim {
        Aim::Reply {
            thread: Some("PRRT_1".into()),
            quote: "Why?".into(),
        }
    }

    #[test]
    fn a_comment_is_the_text_as_written() {
        assert_eq!(
            parse(&Aim::Comment, "  Looks good.\r\n\nShip it.\n").unwrap(),
            Write::Comment {
                body: "Looks good.\n\nShip it.".into()
            }
        );
        assert!(parse(&Aim::Comment, " \n").is_err());
    }

    #[test]
    fn a_reply_in_a_thread_goes_to_that_thread_and_a_reply_elsewhere_quotes() {
        assert_eq!(
            parse(&thread_reply(), "Done.").unwrap(),
            Write::Reply {
                thread: "PRRT_1".into(),
                body: "Done.".into()
            }
        );
        assert!(parse(&thread_reply(), "").is_err());
        let aim = Aim::Reply {
            thread: None,
            quote: "It broke.\nOn tabs.".into(),
        };
        assert_eq!(
            template(&aim).as_deref(),
            Some("> It broke.\n> On tabs.\n\n")
        );
        let said = parse(&aim, "> It broke.\n> On tabs.\n\nFixed.").unwrap();
        assert_eq!(
            said,
            Write::Comment {
                body: "> It broke.\n> On tabs.\n\nFixed.".into()
            }
        );
        let error = parse(&aim, "> It broke.\n> On tabs.\n\n").unwrap_err();
        assert!(error.contains("quote alone"), "{error}");
    }

    #[test]
    fn a_reply_in_a_thread_and_a_comment_leave_what_is_written_as_it_is() {
        assert_eq!(template(&Aim::Comment), None);
        assert_eq!(template(&thread_reply()), None);
    }

    #[test]
    fn a_reaction_is_named_after_react_or_taken_back_after_unreact() {
        let aim = Aim::React {
            subject: "IC_1".into(),
        };
        let react = |text: &str| parse(&aim, text);
        let on = |content: &str, add: bool| Write::React {
            subject: "IC_1".into(),
            content: content.into(),
            add,
        };
        assert_eq!(react("React: +1").unwrap(), on("+1", true));
        assert_eq!(react("react: Rocket\n").unwrap(), on("rocket", true));
        assert_eq!(react("Unreact: eyes").unwrap(), on("eyes", false));
        assert_eq!(react("React: 👀").unwrap(), on("eyes", true));
        assert_eq!(react("heart").unwrap(), on("heart", true));
        assert!(react("React: :tada:").unwrap_err().contains("hooray"));
        assert!(react("React: ").is_err());
        assert_eq!(template(&aim).as_deref(), Some("React: "));
    }

    #[test]
    fn a_review_says_its_verdict_on_its_first_line_and_the_rest_is_its_words() {
        let review = |text: &str| parse(&Aim::Review, text);
        assert_eq!(
            review("Review: request-changes\n\nFix the parser.").unwrap(),
            Write::Review {
                verdict: Verdict::RequestChanges,
                body: "Fix the parser.".into()
            }
        );
        assert_eq!(
            review("review: Approve").unwrap(),
            Write::Review {
                verdict: Verdict::Approve,
                body: String::new()
            }
        );
        assert_eq!(
            review("Review: changes requested\nx").unwrap(),
            review("Review: request-changes\nx").unwrap()
        );
        assert!(review("Review: maybe\n\nx").unwrap_err().contains("maybe"));
        assert!(
            review("Looks fine")
                .unwrap_err()
                .contains("Start the review")
        );
        assert_eq!(
            template(&Aim::Review).as_deref(),
            Some("Review: comment\n\n")
        );
    }

    mod keys {
        use super::*;
        use tsumugi_model::github::ReviewThread;

        fn thread() -> Thread {
            let mut on_lines = Post {
                id: "PRRC_1".into(),
                body: "Why?".into(),
                ..Default::default()
            };
            on_lines.thread = Some(ReviewThread {
                id: "PRRT_1".into(),
                resolved: false,
                path: "a.rs".into(),
                outdated: false,
                line: Some(3),
            });
            Thread {
                body: Post {
                    id: "I_1".into(),
                    body: "Body".into(),
                    ..Default::default()
                },
                said: vec![
                    on_lines,
                    Post {
                        body: "No id".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }
        }

        #[test]
        fn n_and_capital_n_move_the_selection_through_the_cards_and_stop_at_the_ends() {
            let t = thread();
            assert_eq!(act(Github::Next, &t, None), Ok(Act::Select(0)));
            assert_eq!(act(Github::Next, &t, Some(0)), Ok(Act::Select(1)));
            assert_eq!(act(Github::Next, &t, Some(2)), Ok(Act::Select(2)));
            assert_eq!(act(Github::Prev, &t, Some(2)), Ok(Act::Select(1)));
            assert_eq!(act(Github::Prev, &t, Some(0)), Ok(Act::Select(0)));
            assert_eq!(act(Github::Prev, &t, None), Ok(Act::Select(0)));
        }

        #[test]
        fn a_new_comment_and_a_review_need_no_card() {
            let t = thread();
            assert_eq!(
                act(Github::Comment, &t, None),
                Ok(Act::Compose(Aim::Comment))
            );
            assert_eq!(act(Github::Review, &t, None), Ok(Act::Compose(Aim::Review)));
        }

        #[test]
        fn a_reply_goes_to_the_thread_of_the_card_or_quotes_a_card_that_has_none() {
            let t = thread();
            assert_eq!(
                act(Github::Reply, &t, Some(1)),
                Ok(Act::Compose(Aim::Reply {
                    thread: Some("PRRT_1".into()),
                    quote: "Why?".into()
                }))
            );
            assert_eq!(
                act(Github::Reply, &t, Some(0)),
                Ok(Act::Compose(Aim::Reply {
                    thread: None,
                    quote: "Body".into()
                }))
            );
            assert!(
                act(Github::Reply, &t, None)
                    .unwrap_err()
                    .contains("Select a card")
            );
            assert!(act(Github::Reply, &t, Some(9)).is_err());
        }

        #[test]
        fn resolving_flips_the_thread_of_the_card_and_a_card_in_none_says_so() {
            let mut t = thread();
            assert_eq!(
                act(Github::Resolve, &t, Some(1)),
                Ok(Act::Write(Write::Resolve {
                    thread: "PRRT_1".into(),
                    resolved: true
                }))
            );
            t.said[0].thread.as_mut().unwrap().resolved = true;
            assert_eq!(
                act(Github::Resolve, &t, Some(1)),
                Ok(Act::Write(Write::Resolve {
                    thread: "PRRT_1".into(),
                    resolved: false
                }))
            );
            assert!(
                act(Github::Resolve, &t, Some(0))
                    .unwrap_err()
                    .contains("thread")
            );
        }

        #[test]
        fn a_reaction_is_for_a_card_that_has_an_id() {
            let t = thread();
            assert_eq!(
                act(Github::React, &t, Some(0)),
                Ok(Act::Compose(Aim::React {
                    subject: "I_1".into()
                }))
            );
            assert!(act(Github::React, &t, Some(2)).is_err());
            assert!(act(Github::React, &t, None).is_err());
        }
    }

    mod in_the_window {
        use super::*;
        use crate::state::ViewState;
        use crate::testing::{here, show};

        fn github(listing: Listing) -> Tab {
            Tab::GitHub {
                environment: here(),
                repository: "/r".into(),
                listing,
                number: 7,
            }
        }

        fn shown(listing: Listing) -> ViewState {
            let mut state = ViewState::default();
            step(&mut state, show(github(listing)));
            state
        }

        fn taken(state: &ViewState, text: &str) -> Taken {
            Taken {
                tab: state.content.active().cloned(),
                text: text.into(),
                ..Default::default()
            }
        }

        fn writes(effects: &[Effect]) -> Vec<(&Write, bool)> {
            effects
                .iter()
                .filter_map(|e| match e {
                    Effect::GitHubWrite { write, prompt, .. } => Some((write, *prompt)),
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn a_github_tab_shows_the_prompt_to_write_in() {
            let state = shown(Listing::Pulls);
            assert!(shows_prompt(state.content.active()));
        }

        #[test]
        fn writing_a_comment_gives_the_prompt_the_keys_and_sends_nothing() {
            let mut state = shown(Listing::Issues);
            let effects = begin(&mut state, Aim::Comment);
            assert_eq!(state.content.focus, Area::Prompt);
            assert_eq!(state.prompt.writing, Some(Aim::Comment));
            assert_eq!(writes(&effects), []);
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, Effect::PromptRestore { .. }))
            );
        }

        #[test]
        fn what_is_sent_with_no_aim_is_a_new_comment_on_the_tab_shown() {
            let mut state = shown(Listing::Issues);
            let sent = taken_in(&mut state, "Hello");
            assert_eq!(
                writes(&sent),
                [(
                    &Write::Comment {
                        body: "Hello".into()
                    },
                    true
                )]
            );
            let Some(Effect::GitHubWrite {
                repository,
                listing,
                number,
                ..
            }) = sent.first()
            else {
                panic!("{sent:?}")
            };
            assert_eq!(
                (repository.to_str(), *listing, *number),
                (Some("/r"), Listing::Issues, 7)
            );
        }

        fn taken_in(state: &mut ViewState, text: &str) -> Vec<Effect> {
            let t = taken(state, text);
            super::super::taken(state, t)
        }

        #[test]
        fn a_reaction_begun_starts_from_react_and_is_asked_of_the_post() {
            let mut state = shown(Listing::Pulls);
            let started = begin(
                &mut state,
                Aim::React {
                    subject: "IC_1".into(),
                },
            );
            assert!(
                started
                    .iter()
                    .any(|e| matches!(e, Effect::PromptRestore { text, .. } if text == "React: "))
            );
            let sent = taken_in(&mut state, "React: heart");
            assert_eq!(
                writes(&sent),
                [(
                    &Write::React {
                        subject: "IC_1".into(),
                        content: "heart".into(),
                        add: true
                    },
                    true
                )]
            );
        }

        #[test]
        fn text_that_cannot_be_read_is_put_back_with_the_reason_and_nothing_is_sent() {
            let mut state = shown(Listing::Pulls);
            begin(&mut state, Aim::Review);
            let sent = taken_in(&mut state, "Review: maybe\n\nx");
            assert_eq!(writes(&sent), []);
            assert!(sent.iter().any(
                |e| matches!(e, Effect::PromptRestore { text, .. } if text.contains("maybe"))
            ));
            assert!(sent.iter().any(|e| matches!(e, Effect::Notice(_))));
            assert!(state.prompt.sending.is_none());
            assert_eq!(state.prompt.writing, Some(Aim::Review));
        }

        #[test]
        fn a_write_that_failed_puts_the_text_back_and_keeps_the_aim() {
            let mut state = shown(Listing::Pulls);
            begin(&mut state, Aim::Comment);
            taken_in(&mut state, "Hello");
            let failed = step(&mut state, Msg::PromptWent(false));
            assert!(
                failed
                    .iter()
                    .any(|e| matches!(e, Effect::PromptRestore { text, .. } if text == "Hello"))
            );
            assert_eq!(state.prompt.writing, Some(Aim::Comment));
        }

        #[test]
        fn a_write_that_went_ends_the_aim() {
            let mut state = shown(Listing::Pulls);
            begin(&mut state, Aim::Comment);
            taken_in(&mut state, "Hello");
            let went = step(&mut state, Msg::PromptWent(true));
            assert!(went.iter().any(|e| matches!(e, Effect::PromptSent)));
            assert_eq!(state.prompt.writing, None);
        }

        #[test]
        fn leaving_the_prompt_gives_the_aim_up_without_writing_anything() {
            let mut state = shown(Listing::Pulls);
            begin(&mut state, Aim::Review);
            let left = step(&mut state, Msg::Focus(Area::Console));
            assert_eq!(state.prompt.writing, None);
            assert_eq!(writes(&left), []);
        }

        #[test]
        fn an_issue_is_not_reviewed() {
            let mut state = shown(Listing::Issues);
            let effects = begin(&mut state, Aim::Review);
            assert_eq!(state.prompt.writing, None);
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::Notice(t) if t.contains("pull request")))
            );
        }

        #[test]
        fn nothing_begins_where_no_github_tab_is_shown() {
            let mut state = ViewState::default();
            assert_eq!(begin(&mut state, Aim::Comment), []);
            assert_eq!(state.prompt.writing, None);
        }

        #[test]
        fn a_resolution_is_written_at_once_for_the_tab_shown_and_not_through_the_prompt() {
            let mut state = shown(Listing::Pulls);
            let effects = at_once(
                &mut state,
                Write::Resolve {
                    thread: "PRRT_1".into(),
                    resolved: true,
                },
            );
            assert_eq!(
                writes(&effects),
                [(
                    &Write::Resolve {
                        thread: "PRRT_1".into(),
                        resolved: true
                    },
                    false
                )]
            );
        }

        fn squash() -> Write {
            Write::Merge {
                method: tsumugi_model::github::Method::Squash,
                head: "abc".into(),
            }
        }

        #[test]
        fn a_merge_is_asked_of_the_user_and_writes_nothing_until_the_answer_is_yes() {
            let mut state = shown(Listing::Pulls);
            let asked = step(&mut state, Msg::GitHubAsk(squash()));
            assert_eq!(writes(&asked), []);
            assert!(matches!(
                state.overlays.confirm,
                Some(crate::overlay::confirm::Ask::PullWrite { number: 7, .. })
            ));
            let yes = step(&mut state, Msg::ConfirmAnswer(true));
            assert_eq!(writes(&yes), [(&squash(), false)]);
            let Some(Effect::GitHubWrite {
                repository,
                listing,
                number,
                ..
            }) = yes.first()
            else {
                panic!("{yes:?}")
            };
            assert_eq!(
                (repository.to_str(), *listing, *number),
                (Some("/r"), Listing::Pulls, 7)
            );
        }

        #[test]
        fn a_merge_that_is_answered_no_writes_nothing_at_all() {
            let mut state = shown(Listing::Pulls);
            step(&mut state, Msg::GitHubAsk(squash()));
            let no = step(&mut state, Msg::ConfirmAnswer(false));
            assert_eq!(writes(&no), []);
            assert_eq!(state.overlays.confirm, None);
        }

        #[test]
        fn closing_is_asked_as_a_loss_and_merging_is_not() {
            let mut state = shown(Listing::Pulls);
            step(&mut state, Msg::GitHubAsk(Write::Close { closed: true }));
            assert!(state.overlays.confirm.as_ref().unwrap().destroys());
            let (_, title, _, yes) = state.overlays.confirm.as_ref().unwrap().dialog();
            assert_eq!((title.as_str(), yes), ("Close PR #7?", "Close"));
            step(&mut state, Msg::ConfirmAnswer(false));
            step(&mut state, Msg::GitHubAsk(squash()));
            assert!(!state.overlays.confirm.as_ref().unwrap().destroys());
        }

        #[test]
        fn nothing_is_asked_where_the_tab_shown_is_an_issue_or_no_github_tab() {
            let mut state = shown(Listing::Issues);
            step(&mut state, Msg::GitHubAsk(squash()));
            assert_eq!(state.overlays.confirm, None);
            let mut none = ViewState::default();
            step(&mut none, Msg::GitHubAsk(squash()));
            assert_eq!(none.overlays.confirm, None);
        }

        #[test]
        fn the_keys_that_merge_close_or_ready_come_to_a_question_or_to_a_reason() {
            use tsumugi_model::github::{Item, Merge, Pull, Status};
            let thread = Thread {
                item: Item {
                    number: 7,
                    pull: Some(Pull::default()),
                    ..Default::default()
                },
                state: "open".into(),
                merge: Some(Merge {
                    status: Status::Dirty,
                    conflicting: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert_eq!(
                act(Github::Close, &thread, None),
                Ok(Act::Ask(Write::Close { closed: true }))
            );
            let said = act(Github::Merge(None), &thread, None).unwrap_err();
            assert!(said.contains("conflicts"), "{said}");
        }

        #[test]
        fn what_was_written_makes_the_window_read_the_tab_again() {
            let mut state = shown(Listing::Pulls);
            step(&mut state, Msg::Focus(Area::Console));
            let effects = written(&mut state, here(), "/r".into(), Listing::Pulls, 7);
            assert!(matches!(
                effects.as_slice(),
                [Effect::Viewer {
                    action: crate::viewers::ViewerAction::Reload,
                    ..
                }]
            ));
            let elsewhere = written(&mut state, here(), "/r".into(), Listing::Pulls, 8);
            assert_eq!(elsewhere, []);
        }
    }
}
