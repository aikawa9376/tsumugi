//! What merging, closing or marking ready a pull request comes to: decided
//! from what its tab read of it, so that what GitHub would refuse is told
//! before anything is asked of it, and what is allowed is asked of the user
//! first. Nothing here writes; the answer to the question does.

use tsumugi_model::github::{Merge, Method, Review, Status, Thread, Write};

use crate::viewers::Github;

/// What a key of a pull request's tab that merges, closes or marks it comes
/// to: the write to ask the user about, or why it cannot be made.
pub fn plan(key: Github, thread: &Thread) -> Result<Write, String> {
    let pull = thread
        .item
        .pull
        .as_ref()
        .ok_or("Only a pull request is merged, closed or made ready")?;
    let number = thread.item.number;
    let state = thread.state.as_str();
    match key {
        Github::Close => match state {
            "merged" => Err(format!("Pull request #{number} is already merged")),
            "closed" => Ok(Write::Close { closed: false }),
            _ => Ok(Write::Close { closed: true }),
        },
        Github::Ready => match state {
            "merged" => Err(format!("Pull request #{number} is already merged")),
            "closed" => Err(format!("Pull request #{number} is closed; reopen it first")),
            "draft" => Ok(Write::Draft { draft: false }),
            _ => Ok(Write::Draft { draft: true }),
        },
        Github::Merge(method) => {
            open(thread)?;
            let merge = read(thread)?;
            can_merge(thread, merge)?;
            let method = method_of(merge, method)?;
            Ok(Write::Merge {
                method,
                head: pull.head_sha.clone(),
            })
        }
        Github::AutoMerge => {
            // Turning it off is allowed wherever it is on.
            if let Some(method) = thread.merge.as_ref().and_then(|m| m.auto) {
                return Ok(Write::Auto {
                    method,
                    enabled: false,
                });
            }
            open(thread)?;
            let merge = read(thread)?;
            if !merge.may_merge() {
                return Err(not_allowed(merge));
            }
            if merge.conflicting || merge.status == Status::Dirty {
                return Err(conflicts(thread));
            }
            let waits = merge.review.is_some_and(|r| r != Review::Approved)
                || matches!(merge.status, Status::Blocked | Status::Behind);
            if !waits {
                return Err(format!(
                    "Nothing holds pull request #{number} back, so auto-merge has nothing to wait for; merge it"
                ));
            }
            Ok(Write::Auto {
                method: method_of(merge, None)?,
                enabled: true,
            })
        }
        _ => Err("That key does not merge, close or mark a pull request".into()),
    }
}

/// Whether the pull request is one that can still be merged: open, and not a
/// draft.
fn open(thread: &Thread) -> Result<(), String> {
    let number = thread.item.number;
    match thread.state.as_str() {
        "merged" => Err(format!("Pull request #{number} is already merged")),
        "closed" => Err(format!("Pull request #{number} is closed; reopen it first")),
        "draft" => Err(format!(
            "Pull request #{number} is a draft; mark it ready first"
        )),
        _ => Ok(()),
    }
}

/// What was read of its merging, or why it was not.
fn read(thread: &Thread) -> Result<&Merge, String> {
    thread.merge.as_ref().ok_or_else(|| {
        "What merging depends on could not be read from GitHub; read the pull request again first"
            .into()
    })
}

fn not_allowed(merge: &Merge) -> String {
    format!(
        "The login gh uses has {} access to this repository, and merging needs write access",
        merge.permission.to_lowercase()
    )
}

fn conflicts(thread: &Thread) -> String {
    let base = thread
        .item
        .pull
        .as_ref()
        .map_or("its base", |p| p.base.as_str());
    format!(
        "Pull request #{} has merge conflicts and cannot be merged yet; update the branch with {base} and resolve them first",
        thread.item.number
    )
}

/// Why the pull request cannot be merged now, or nothing when it can.
fn can_merge(thread: &Thread, merge: &Merge) -> Result<(), String> {
    let number = thread.item.number;
    if !merge.may_merge() {
        return Err(not_allowed(merge));
    }
    match merge.review {
        Some(Review::Required) => {
            return Err(format!(
                "Pull request #{number} needs a review approval before it can be merged"
            ));
        }
        Some(Review::ChangesRequested) => {
            return Err(format!(
                "Pull request #{number} has requested changes and cannot be merged yet"
            ));
        }
        _ => {}
    }
    if merge.conflicting || merge.status == Status::Dirty {
        return Err(conflicts(thread));
    }
    match merge.status {
        Status::Behind => Err(format!(
            "Pull request #{number} is behind its base; update the branch first, or turn auto-merge on"
        )),
        Status::Blocked => Err(blocked(thread)),
        Status::Unknown => Err(format!(
            "GitHub has not yet worked out whether pull request #{number} can be merged; read it again in a moment"
        )),
        // Dirty was answered with the conflicts above.
        Status::Clean | Status::Dirty | Status::Unstable | Status::HasHooks | Status::Draft => {
            Ok(())
        }
    }
}

/// What holds a blocked pull request back, from its checks where they say.
fn blocked(thread: &Thread) -> String {
    let number = thread.item.number;
    let failed = thread
        .checks
        .iter()
        .filter(|c| c.outcome == tsumugi_model::github::Outcome::Failed)
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>();
    let pending = thread
        .checks
        .iter()
        .filter(|c| c.outcome == tsumugi_model::github::Outcome::Pending)
        .count();
    if !failed.is_empty() {
        format!(
            "Required checks of pull request #{number} have not passed: {} failed",
            failed.join(", ")
        )
    } else if pending > 0 {
        format!(
            "Required checks of pull request #{number} have not finished: {pending} still running"
        )
    } else {
        format!(
            "GitHub holds pull request #{number} back: what the base branch requires is not met"
        )
    }
}

/// The method asked for, or the repository's preferred one, if the
/// repository allows it.
fn method_of(merge: &Merge, asked: Option<Method>) -> Result<Method, String> {
    let Some(methods) = &merge.methods else {
        return Ok(asked.unwrap_or(Method::Squash));
    };
    let method = asked.unwrap_or(methods.default);
    if methods.allowed.contains(&method) {
        Ok(method)
    } else {
        let allowed: Vec<_> = methods.allowed.iter().map(|m| m.label()).collect();
        Err(format!(
            "{} is not allowed in this repository; it allows {}",
            method.label(),
            allowed.join(", ")
        ))
    }
}

/// What the question about `write` on the pull request `number` says: its
/// title, its words, what answering yes is called, and whether it is the
/// kind of act that cannot be taken back.
pub fn question(number: u64, write: &Write) -> (String, String, &'static str, bool) {
    match write {
        Write::Merge { method, .. } => (
            match method {
                Method::Merge => format!("Merge PR #{number} with a merge commit?"),
                Method::Squash => format!("Squash and merge PR #{number}?"),
                Method::Rebase => format!("Rebase and merge PR #{number}?"),
            },
            "This will update the pull request on GitHub.".into(),
            method.label(),
            false,
        ),
        Write::Auto {
            method,
            enabled: true,
        } => (
            format!("Enable auto-merge on PR #{number}?"),
            format!(
                "GitHub will merge it, with {}, once what it waits on is met.",
                method.label().to_lowercase()
            ),
            "Enable auto-merge",
            false,
        ),
        Write::Auto { enabled: false, .. } => (
            format!("Disable auto-merge on PR #{number}?"),
            "GitHub will no longer merge it by itself.".into(),
            "Disable auto-merge",
            false,
        ),
        Write::Close { closed: true } => (
            format!("Close PR #{number}?"),
            "This will close the pull request on GitHub.".into(),
            "Close",
            true,
        ),
        Write::Close { closed: false } => (
            format!("Reopen PR #{number}?"),
            "This will reopen the pull request on GitHub.".into(),
            "Reopen",
            false,
        ),
        Write::Draft { draft: false } => (
            format!("Mark PR #{number} ready for review?"),
            "Reviewers who were asked are told it is ready.".into(),
            "Mark ready",
            false,
        ),
        Write::Draft { draft: true } => (
            format!("Convert PR #{number} to a draft?"),
            "It cannot be merged until it is marked ready again.".into(),
            "Convert to draft",
            false,
        ),
        Write::Rerun { run } => (
            format!("Re-run failed jobs of PR #{number}?"),
            format!(
                "GitHub runs again the jobs that failed in run {run}, and those that needed them, as the login gh uses."
            ),
            "Re-run",
            false,
        ),
        _ => (
            format!("Write on PR #{number}?"),
            "This will update the pull request on GitHub.".into(),
            "Write",
            false,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::github::{Check, Item, Methods, Outcome, Pull};

    /// An open pull request #7 into `main`, ending at `abc`, that GitHub says
    /// is clean and the login may merge, in a repository that allows every
    /// method.
    fn thread() -> Thread {
        Thread {
            item: Item {
                number: 7,
                pull: Some(Pull {
                    base: "main".into(),
                    head_sha: "abc".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            state: "open".into(),
            merge: Some(Merge {
                status: Status::Clean,
                methods: Some(Methods::any()),
                permission: "WRITE".into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn with(edit: impl FnOnce(&mut Thread, &mut Merge)) -> Thread {
        let mut thread = thread();
        let mut merge = thread.merge.take().unwrap();
        edit(&mut thread, &mut merge);
        thread.merge = Some(merge);
        thread
    }

    fn merge(method: Option<Method>) -> Github {
        Github::Merge(method)
    }

    fn refused(key: Github, thread: &Thread) -> String {
        plan(key, thread).unwrap_err()
    }

    #[test]
    fn a_clean_pull_request_is_merged_with_the_repositorys_preferred_method_fenced_to_its_head() {
        assert_eq!(
            plan(merge(None), &thread()),
            Ok(Write::Merge {
                method: Method::Squash,
                head: "abc".into()
            })
        );
        let rebase = with(|_, m| m.methods = Methods::of(Some(Method::Rebase), true, true, true));
        assert_eq!(
            plan(merge(None), &rebase),
            Ok(Write::Merge {
                method: Method::Rebase,
                head: "abc".into()
            })
        );
    }

    #[test]
    fn a_method_the_repository_does_not_allow_is_refused_with_those_it_does() {
        let only_merge = with(|_, m| m.methods = Methods::of(None, true, false, false));
        let said = refused(merge(Some(Method::Squash)), &only_merge);
        assert!(
            said.contains("Squash and merge is not allowed")
                && said.contains("Create merge commit"),
            "{said}"
        );
        assert!(plan(merge(Some(Method::Merge)), &only_merge).is_ok());
        // With the default asked for, it is the one the repository allows.
        assert_eq!(
            plan(merge(None), &only_merge),
            Ok(Write::Merge {
                method: Method::Merge,
                head: "abc".into()
            })
        );
    }

    #[test]
    fn settings_that_could_not_be_read_leave_the_choice_to_github() {
        let unread = with(|_, m| m.methods = None);
        assert!(plan(merge(Some(Method::Rebase)), &unread).is_ok());
    }

    #[test]
    fn nothing_is_merged_that_is_not_open_and_ready() {
        for (state, words) in [
            ("merged", "already merged"),
            ("closed", "reopen it first"),
            ("draft", "mark it ready first"),
        ] {
            let thread = Thread {
                state: state.into(),
                ..thread()
            };
            assert!(refused(merge(None), &thread).contains(words), "{state}");
            assert!(
                refused(Github::AutoMerge, &thread).contains(words),
                "{state}"
            );
        }
    }

    #[test]
    fn a_login_without_write_access_is_told_so() {
        let read = with(|_, m| m.permission = "READ".into());
        let said = refused(merge(None), &read);
        assert!(
            said.contains("read access") && said.contains("write access"),
            "{said}"
        );
        assert!(
            refused(
                Github::AutoMerge,
                &with(|_, m| {
                    m.permission = "TRIAGE".into();
                    m.status = Status::Blocked;
                })
            )
            .contains("triage access")
        );
        // What was not read is not held against the login.
        assert!(plan(merge(None), &with(|_, m| m.permission.clear())).is_ok());
    }

    #[test]
    fn a_review_that_is_wanted_or_against_it_blocks_the_merge_and_says_which() {
        let required = with(|_, m| m.review = Some(Review::Required));
        assert!(refused(merge(None), &required).contains("review approval"));
        let changes = with(|_, m| m.review = Some(Review::ChangesRequested));
        assert!(refused(merge(None), &changes).contains("requested changes"));
        let approved = with(|_, m| m.review = Some(Review::Approved));
        assert!(plan(merge(None), &approved).is_ok());
    }

    #[test]
    fn conflicts_block_the_merge_and_name_the_base_to_update_with() {
        for thread in [
            with(|_, m| m.conflicting = true),
            with(|_, m| m.status = Status::Dirty),
        ] {
            let said = refused(merge(None), &thread);
            assert!(
                said.contains("conflicts") && said.contains("main"),
                "{said}"
            );
            assert!(refused(Github::AutoMerge, &thread).contains("conflicts"));
        }
    }

    #[test]
    fn checks_that_are_required_and_not_passed_block_the_merge_and_are_named() {
        let check = |name: &str, outcome| Check {
            name: name.into(),
            outcome,
            ..Default::default()
        };
        let failed = with(|t, m| {
            m.status = Status::Blocked;
            t.checks = vec![
                check("lint", Outcome::Failed),
                check("test", Outcome::Passed),
            ];
        });
        let said = refused(merge(None), &failed);
        assert!(
            said.contains("lint") && said.contains("not passed"),
            "{said}"
        );
        let running = with(|t, m| {
            m.status = Status::Blocked;
            t.checks = vec![check("test", Outcome::Pending)];
        });
        assert!(refused(merge(None), &running).contains("1 still running"));
        let other = with(|_, m| m.status = Status::Blocked);
        assert!(refused(merge(None), &other).contains("base branch requires"));
    }

    #[test]
    fn a_branch_that_is_behind_or_whose_status_is_not_yet_known_is_not_merged() {
        let behind = with(|_, m| m.status = Status::Behind);
        assert!(refused(merge(None), &behind).contains("behind"));
        let unknown = with(|_, m| m.status = Status::Unknown);
        assert!(refused(merge(None), &unknown).contains("not yet worked out"));
        // A check that is not required, or hooks, do not stop it.
        for status in [Status::Unstable, Status::HasHooks] {
            assert!(plan(merge(None), &with(|_, m| m.status = status)).is_ok());
        }
    }

    #[test]
    fn a_merge_state_that_was_not_read_is_not_guessed() {
        let unread = Thread {
            merge: None,
            ..thread()
        };
        for key in [merge(None), Github::AutoMerge] {
            assert!(refused(key, &unread).contains("read the pull request again"));
        }
        // Closing needs none of it.
        assert!(plan(Github::Close, &unread).is_ok());
    }

    #[test]
    fn auto_merge_is_offered_only_while_something_holds_the_merge_back() {
        for status in [Status::Blocked, Status::Behind] {
            assert_eq!(
                plan(Github::AutoMerge, &with(|_, m| m.status = status)),
                Ok(Write::Auto {
                    method: Method::Squash,
                    enabled: true
                })
            );
        }
        assert!(
            plan(
                Github::AutoMerge,
                &with(|_, m| m.review = Some(Review::Required))
            )
            .is_ok()
        );
        let said = refused(Github::AutoMerge, &thread());
        assert!(
            said.contains("nothing") || said.contains("Nothing"),
            "{said}"
        );
        assert!(
            refused(Github::AutoMerge, &with(|_, m| m.status = Status::Unstable))
                .contains("Nothing holds")
        );
    }

    #[test]
    fn auto_merge_that_is_on_is_turned_off_with_the_method_it_was_on_with() {
        let on = with(|_, m| m.auto = Some(Method::Rebase));
        assert_eq!(
            plan(Github::AutoMerge, &on),
            Ok(Write::Auto {
                method: Method::Rebase,
                enabled: false
            })
        );
        // Even where the login could not turn it on.
        let read = with(|_, m| {
            m.auto = Some(Method::Squash);
            m.permission = "READ".into();
        });
        assert!(plan(Github::AutoMerge, &read).is_ok());
    }

    #[test]
    fn an_open_pull_request_is_closed_a_closed_one_reopened_and_a_merged_one_neither() {
        assert_eq!(
            plan(Github::Close, &thread()),
            Ok(Write::Close { closed: true })
        );
        let draft = Thread {
            state: "draft".into(),
            ..thread()
        };
        assert_eq!(
            plan(Github::Close, &draft),
            Ok(Write::Close { closed: true })
        );
        let closed = Thread {
            state: "closed".into(),
            ..thread()
        };
        assert_eq!(
            plan(Github::Close, &closed),
            Ok(Write::Close { closed: false })
        );
        let merged = Thread {
            state: "merged".into(),
            ..thread()
        };
        assert!(refused(Github::Close, &merged).contains("already merged"));
    }

    #[test]
    fn a_draft_is_marked_ready_an_open_one_made_a_draft_and_a_closed_one_neither() {
        let draft = Thread {
            state: "draft".into(),
            ..thread()
        };
        assert_eq!(
            plan(Github::Ready, &draft),
            Ok(Write::Draft { draft: false })
        );
        assert_eq!(
            plan(Github::Ready, &thread()),
            Ok(Write::Draft { draft: true })
        );
        let closed = Thread {
            state: "closed".into(),
            ..thread()
        };
        assert!(refused(Github::Ready, &closed).contains("reopen it first"));
    }

    #[test]
    fn an_issue_is_none_of_these() {
        let issue = Thread {
            item: Item {
                number: 3,
                ..Default::default()
            },
            ..Default::default()
        };
        for key in [merge(None), Github::AutoMerge, Github::Close, Github::Ready] {
            assert!(
                refused(key, &issue).contains("Only a pull request"),
                "{key:?}"
            );
        }
    }

    #[test]
    fn each_question_names_the_pull_request_and_what_answering_yes_does() {
        let ask = |write| question(7, &write);
        let (title, _, yes, destroys) = ask(Write::Merge {
            method: Method::Squash,
            head: String::new(),
        });
        assert_eq!(
            (title.as_str(), yes, destroys),
            ("Squash and merge PR #7?", "Squash and merge", false)
        );
        assert_eq!(
            ask(Write::Merge {
                method: Method::Merge,
                head: String::new()
            })
            .0,
            "Merge PR #7 with a merge commit?"
        );
        assert_eq!(
            ask(Write::Merge {
                method: Method::Rebase,
                head: String::new()
            })
            .0,
            "Rebase and merge PR #7?"
        );
        let (title, body, yes, destroys) = ask(Write::Close { closed: true });
        assert_eq!(title, "Close PR #7?");
        assert!(body.contains("close the pull request"));
        assert_eq!((yes, destroys), ("Close", true));
        assert_eq!(ask(Write::Close { closed: false }).0, "Reopen PR #7?");
        assert_eq!(
            ask(Write::Auto {
                method: Method::Squash,
                enabled: true
            })
            .0,
            "Enable auto-merge on PR #7?"
        );
        assert_eq!(
            ask(Write::Auto {
                method: Method::Squash,
                enabled: false
            })
            .0,
            "Disable auto-merge on PR #7?"
        );
        assert_eq!(
            ask(Write::Draft { draft: false }).0,
            "Mark PR #7 ready for review?"
        );
        assert_eq!(
            ask(Write::Draft { draft: true }).0,
            "Convert PR #7 to a draft?"
        );
    }
}
