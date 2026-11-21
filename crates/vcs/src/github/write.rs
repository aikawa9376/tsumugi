//! Writing on a pull request or an issue through `gh`, so the login stays
//! `gh`'s: a comment, a reply to a thread of lines, resolving that thread, a
//! reaction, a review and a file marked viewed. Each is one explicit call;
//! nothing here edits or deletes what someone else wrote.

use std::path::Path;

use tsumugi_model::github::{Listing, REACTIONS, Verdict, Write};

use super::remote;
use crate::{Error, run};

/// Do `write` on the pull request or issue `number` of the repository at
/// `root`, and say in a few words what was done. It fails, with nothing
/// written, when the text is empty where GitHub wants some, when `gh` is
/// missing or logged out, and with GitHub's own words when it refuses, as
/// for a login that may not write there.
pub async fn write(
    root: &Path,
    listing: Listing,
    number: u64,
    write: &Write,
) -> Result<String, String> {
    let slug = remote(root)
        .await
        .ok_or("the repository has no remote on GitHub")?
        .slug;
    write_with("gh", root, &slug, listing, number, write).await
}

/// [`write`], running `gh` as the program `gh` on the repository `slug`.
async fn write_with(
    gh: &str,
    root: &Path,
    slug: &str,
    listing: Listing,
    number: u64,
    write: &Write,
) -> Result<String, String> {
    let args = args(slug, listing, number, write)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(gh, &args, Some(root)).await {
        Ok(_) => Ok(done(listing, number, write)),
        Err(Error::Spawn { .. }) => {
            Err("gh is not installed on the machine that holds the repository".into())
        }
        Err(Error::Failed { stderr, .. }) => Err(explain(&stderr)),
    }
}

/// What was done, in words for a notice.
fn done(listing: Listing, number: u64, write: &Write) -> String {
    let what = match listing {
        Listing::Pulls => "pull request",
        Listing::Issues => "issue",
    };
    match write {
        Write::Comment { .. } => format!("Commented on {what} #{number}"),
        Write::Reply { .. } => format!("Replied on {what} #{number}"),
        Write::Resolve { resolved: true, .. } => "Resolved the thread".into(),
        Write::Resolve {
            resolved: false, ..
        } => "Reopened the thread".into(),
        Write::React {
            content, add: true, ..
        } => format!("Reacted {content}"),
        Write::React {
            content,
            add: false,
            ..
        } => format!("Took back {content}"),
        Write::Review { verdict, .. } => match verdict {
            Verdict::Approve => format!("Approved {what} #{number}"),
            Verdict::RequestChanges => format!("Requested changes on {what} #{number}"),
            Verdict::Comment => format!("Reviewed {what} #{number} with a comment"),
        },
        Write::Viewed {
            path, viewed: true, ..
        } => format!("Marked {path} viewed"),
        Write::Viewed {
            path,
            viewed: false,
            ..
        } => format!("Marked {path} not viewed"),
    }
}

/// The GraphQL name of a reaction.
fn graphql_reaction(content: &str) -> Option<&'static str> {
    Some(match content {
        "+1" => "THUMBS_UP",
        "-1" => "THUMBS_DOWN",
        "laugh" => "LAUGH",
        "confused" => "CONFUSED",
        "heart" => "HEART",
        "hooray" => "HOORAY",
        "rocket" => "ROCKET",
        "eyes" => "EYES",
        _ => return None,
    })
}

fn graphql(query: &str, fields: &[(&str, &str)]) -> Vec<String> {
    let mut args: Vec<String> = ["api", "graphql", "-f"]
        .into_iter()
        .map(String::from)
        .collect();
    args.push(format!("query={query}"));
    for (name, value) in fields {
        args.push("-f".into());
        args.push(format!("{name}={value}"));
    }
    args
}

/// The arguments of the `gh` call that does `write`, or why it cannot be
/// made. A value is joined to its flag with `=`, so a body that starts with
/// `-` is not read as a flag, and `-f` gives it as the text it is.
fn args(slug: &str, listing: Listing, number: u64, write: &Write) -> Result<Vec<String>, String> {
    let empty = |body: &str, what: &str| {
        if body.trim().is_empty() {
            Err(format!("{what} is empty"))
        } else {
            Ok(())
        }
    };
    Ok(match write {
        Write::Comment { body } => {
            empty(body, "The comment")?;
            let path = format!("repos/{slug}/issues/{number}/comments");
            vec![
                "api".into(),
                "--method=POST".into(),
                path,
                "-f".into(),
                format!("body={}", body.trim()),
            ]
        }
        Write::Reply { thread, body } => {
            empty(body, "The reply")?;
            graphql(
                "mutation($thread:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$thread,body:$body}){comment{id}}}",
                &[("thread", thread), ("body", body.trim())],
            )
        }
        Write::Resolve { thread, resolved } => {
            let mutation = if *resolved {
                "resolveReviewThread"
            } else {
                "unresolveReviewThread"
            };
            graphql(
                &format!(
                    "mutation($thread:ID!){{{mutation}(input:{{threadId:$thread}}){{thread{{isResolved}}}}}}"
                ),
                &[("thread", thread)],
            )
        }
        Write::React {
            subject,
            content,
            add,
        } => {
            let named = graphql_reaction(content).ok_or_else(|| {
                format!(
                    "{content:?} is not a reaction GitHub has; they are {}",
                    REACTIONS.join(", ")
                )
            })?;
            let mutation = if *add {
                "addReaction"
            } else {
                "removeReaction"
            };
            graphql(
                &format!(
                    "mutation($subject:ID!,$content:ReactionContent!){{{mutation}(input:{{subjectId:$subject,content:$content}}){{subject{{id}}}}}}"
                ),
                &[("subject", subject), ("content", named)],
            )
        }
        Write::Review { verdict, body } => {
            if listing != Listing::Pulls {
                return Err("Only a pull request is reviewed".into());
            }
            if *verdict != Verdict::Approve {
                empty(body, "The review")?;
            }
            let event = match verdict {
                Verdict::Approve => "APPROVE",
                Verdict::RequestChanges => "REQUEST_CHANGES",
                Verdict::Comment => "COMMENT",
            };
            let mut args = vec![
                "api".into(),
                "--method=POST".into(),
                format!("repos/{slug}/pulls/{number}/reviews"),
                "-f".into(),
                format!("event={event}"),
            ];
            if !body.trim().is_empty() {
                args.push("-f".into());
                args.push(format!("body={}", body.trim()));
            }
            args
        }
        Write::Viewed { pull, path, viewed } => {
            if pull.is_empty() {
                return Err("The pull request was not read whole; read it again first".into());
            }
            let mutation = if *viewed {
                "markFileAsViewed"
            } else {
                "unmarkFileAsViewed"
            };
            graphql(
                &format!(
                    "mutation($pull:ID!,$path:String!){{{mutation}(input:{{pullRequestId:$pull,path:$path}}){{pullRequest{{id}}}}}}"
                ),
                &[("pull", pull), ("path", path)],
            )
        }
    })
}

/// Why `gh` refused, from its own words, with what a refusal for the login
/// usually means.
fn explain(stderr: &str) -> String {
    if stderr.contains("gh auth login") {
        return "gh is not logged in; run `gh auth login` on the machine that holds the repository"
            .into();
    }
    let said = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    let denied = [
        "HTTP 403",
        "HTTP 404",
        "Resource not accessible",
        "FORBIDDEN",
    ];
    if denied.iter().any(|d| stderr.contains(d)) {
        format!("{said} (the login gh uses may not be allowed to write here)")
    } else if said.is_empty() {
        "gh failed".into()
    } else {
        said
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulls(write: &Write) -> Result<Vec<String>, String> {
        args("o/r", Listing::Pulls, 7, write)
    }

    #[test]
    fn a_comment_is_posted_to_the_conversation_as_text() {
        let args = pulls(&Write::Comment {
            body: "  -x @file\n".into(),
        })
        .unwrap();
        assert_eq!(
            args,
            [
                "api",
                "--method=POST",
                "repos/o/r/issues/7/comments",
                "-f",
                "body=-x @file"
            ]
        );
    }

    #[test]
    fn nothing_is_written_with_an_empty_body() {
        for write in [
            Write::Comment { body: " \n".into() },
            Write::Reply {
                thread: "T".into(),
                body: String::new(),
            },
            Write::Review {
                verdict: Verdict::RequestChanges,
                body: "  ".into(),
            },
            Write::Review {
                verdict: Verdict::Comment,
                body: String::new(),
            },
        ] {
            assert!(pulls(&write).unwrap_err().contains("empty"), "{write:?}");
        }
    }

    #[test]
    fn an_approval_needs_no_words() {
        let args = pulls(&Write::Review {
            verdict: Verdict::Approve,
            body: String::new(),
        })
        .unwrap();
        assert_eq!(
            args,
            [
                "api",
                "--method=POST",
                "repos/o/r/pulls/7/reviews",
                "-f",
                "event=APPROVE"
            ]
        );
    }

    #[test]
    fn a_review_says_its_verdict_and_its_words() {
        let args = pulls(&Write::Review {
            verdict: Verdict::RequestChanges,
            body: "Fix it.".into(),
        })
        .unwrap();
        assert!(args.contains(&"event=REQUEST_CHANGES".to_string()));
        assert!(args.contains(&"body=Fix it.".to_string()));
    }

    #[test]
    fn an_issue_is_not_reviewed() {
        let error = args(
            "o/r",
            Listing::Issues,
            7,
            &Write::Review {
                verdict: Verdict::Approve,
                body: String::new(),
            },
        )
        .unwrap_err();
        assert!(error.contains("pull request"), "{error}");
    }

    #[test]
    fn a_reply_and_a_resolution_are_asked_of_the_thread_by_its_id() {
        let reply = pulls(&Write::Reply {
            thread: "PRRT_1".into(),
            body: "Done.".into(),
        })
        .unwrap();
        assert_eq!(&reply[..3], ["api", "graphql", "-f"]);
        assert!(reply[3].contains("addPullRequestReviewThreadReply"));
        assert!(reply.contains(&"thread=PRRT_1".to_string()));
        assert!(reply.contains(&"body=Done.".to_string()));
        let resolve = pulls(&Write::Resolve {
            thread: "PRRT_1".into(),
            resolved: true,
        })
        .unwrap();
        assert!(resolve[3].contains("resolveReviewThread(") && !resolve[3].contains("unresolve"));
        let reopen = pulls(&Write::Resolve {
            thread: "PRRT_1".into(),
            resolved: false,
        })
        .unwrap();
        assert!(reopen[3].contains("unresolveReviewThread"));
    }

    #[test]
    fn a_reaction_is_added_or_taken_back_by_its_graphql_name() {
        let add = pulls(&Write::React {
            subject: "IC_1".into(),
            content: "+1".into(),
            add: true,
        })
        .unwrap();
        assert!(add[3].contains("addReaction"));
        assert!(add.contains(&"content=THUMBS_UP".to_string()));
        assert!(add.contains(&"subject=IC_1".to_string()));
        let take = pulls(&Write::React {
            subject: "IC_1".into(),
            content: "eyes".into(),
            add: false,
        })
        .unwrap();
        assert!(take[3].contains("removeReaction"));
        assert!(take.contains(&"content=EYES".to_string()));
    }

    #[test]
    fn every_reaction_github_offers_has_a_graphql_name_and_no_other_does() {
        for content in REACTIONS {
            assert!(graphql_reaction(content).is_some(), "{content}");
        }
        let error = pulls(&Write::React {
            subject: "IC_1".into(),
            content: "smile".into(),
            add: true,
        })
        .unwrap_err();
        assert!(
            error.contains("not a reaction") && error.contains("rocket"),
            "{error}"
        );
    }

    #[test]
    fn a_file_is_marked_viewed_by_the_pull_requests_id() {
        let args = pulls(&Write::Viewed {
            pull: "PR_1".into(),
            path: "src/a.rs".into(),
            viewed: true,
        })
        .unwrap();
        assert!(args[3].contains("markFileAsViewed"));
        assert!(args.contains(&"pull=PR_1".to_string()));
        assert!(args.contains(&"path=src/a.rs".to_string()));
        let unread = pulls(&Write::Viewed {
            pull: String::new(),
            path: "a".into(),
            viewed: true,
        });
        assert!(unread.is_err());
    }

    #[test]
    fn a_refusal_for_the_login_says_so_beside_githubs_words() {
        let said = explain("gh: Resource not accessible by integration (HTTP 403)\n");
        assert!(said.contains("Resource not accessible"), "{said}");
        assert!(said.contains("may not be allowed"), "{said}");
        assert!(explain("gh auth login to authenticate").contains("not logged in"));
        assert_eq!(explain("boom\n\nsecond"), "boom; second");
        assert_eq!(explain(""), "gh failed");
    }

    #[cfg(unix)]
    mod with_a_fake_gh {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn gh(dir: &Path, script: &str) -> String {
            let path = dir.join("gh");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\necho \"$@\" >> \"{}/calls\"\n{script}\n",
                    dir.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path.to_str().unwrap().into()
        }

        fn comment() -> Write {
            Write::Comment { body: "Hi".into() }
        }

        #[tokio::test]
        async fn a_comment_is_posted_with_gh_and_reported() {
            let dir = tempfile::tempdir().unwrap();
            let gh = gh(dir.path(), "exit 0");
            let said = write_with(&gh, dir.path(), "o/r", Listing::Issues, 3, &comment())
                .await
                .unwrap();
            assert_eq!(said, "Commented on issue #3");
            let calls = std::fs::read_to_string(dir.path().join("calls")).unwrap();
            assert!(calls.contains("repos/o/r/issues/3/comments"), "{calls}");
        }

        #[tokio::test]
        async fn a_refusal_comes_back_in_ghs_words_and_a_missing_gh_is_named() {
            let dir = tempfile::tempdir().unwrap();
            let gh = gh(dir.path(), "echo 'gh: Not Found (HTTP 404)' >&2; exit 1");
            let error = write_with(&gh, dir.path(), "o/r", Listing::Pulls, 3, &comment())
                .await
                .unwrap_err();
            assert!(error.contains("Not Found"), "{error}");
            let missing = dir.path().join("none").to_str().unwrap().to_string();
            let error = write_with(&missing, dir.path(), "o/r", Listing::Pulls, 3, &comment())
                .await
                .unwrap_err();
            assert!(error.contains("not installed"), "{error}");
        }

        #[tokio::test]
        async fn an_empty_body_never_reaches_gh() {
            let dir = tempfile::tempdir().unwrap();
            let gh = gh(dir.path(), "exit 0");
            let empty = Write::Comment { body: "".into() };
            assert!(
                write_with(&gh, dir.path(), "o/r", Listing::Pulls, 3, &empty)
                    .await
                    .is_err()
            );
            assert!(!dir.path().join("calls").exists());
        }
    }
}
