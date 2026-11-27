//! What the Issues sheet opens of an issue's attempts: each one's change,
//! and two of them read against each other.

use std::path::Path;

use tsumugi_model::repository::{Rev, Spot};
use tsumugi_model::{Attempt, EnvironmentId, Tree};

use crate::msg::Msg;

/// The diff of `attempt`'s change: its worktree, committed or not, against
/// the commit the attempts were made from. None before it has a worktree.
pub fn change(environment: &EnvironmentId, attempt: &Attempt) -> Option<Msg> {
    let worktree = attempt.worktree.clone()?;
    Some(Msg::OpenDiff {
        environment: environment.clone(),
        worktree,
        spot: Spot::Stream(Rev::Base(attempt.base.clone())),
        neovim: false,
        line: None,
    })
}

/// The diff of `second`'s branch read against `first`'s, in `second`'s
/// worktree, as the `tree` lists their branches. Why not, when either has
/// no worktree or no branch yet.
pub fn compare(
    environment: &EnvironmentId,
    tree: &Tree,
    first: &Attempt,
    second: &Attempt,
) -> Result<Msg, String> {
    let branch = |attempt: &Attempt| {
        attempt
            .worktree
            .as_deref()
            .and_then(|path| branch_of(tree, path))
            .ok_or_else(|| format!("attempt {} has no branch to compare yet", attempt.number))
    };
    let (from, to) = (branch(first)?, branch(second)?);
    Ok(Msg::OpenDiff {
        environment: environment.clone(),
        worktree: second.worktree.clone().unwrap_or_default(),
        spot: Spot::Stream(Rev::Range(from, to)),
        neovim: false,
        line: None,
    })
}

fn branch_of(tree: &Tree, path: &Path) -> Option<String> {
    tree.worktrees()
        .find(|w| w.worktree.path == path)
        .and_then(|w| w.worktree.branch.clone())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tsumugi_model::{LocalIssueState, TryAs};

    use super::*;
    use crate::scenario::Scenario;
    use crate::testing::{here, listed, snapshot, worktree};

    fn attempt(number: u32, worktree: Option<&str>) -> Attempt {
        Attempt {
            number,
            tried_as: TryAs::default(),
            state: LocalIssueState::Done,
            record: None,
            session: None,
            worktree: worktree.map(PathBuf::from),
            base: "abc123".into(),
            error: None,
        }
    }

    #[test]
    fn an_attempt_s_change_is_its_worktree_against_the_commit_the_attempts_began_from() {
        assert_eq!(
            change(&here(), &attempt(1, Some("/r/one"))),
            Some(Msg::OpenDiff {
                environment: here(),
                worktree: "/r/one".into(),
                spot: Spot::Stream(Rev::Base("abc123".into())),
                neovim: false,
                line: None,
            })
        );
        assert_eq!(change(&here(), &attempt(2, None)), None);
    }

    #[test]
    fn two_attempts_are_compared_by_their_branches_in_the_second_s_worktree() {
        let run = Scenario::new().event(
            snapshot().listed(
                listed()
                    .kept("/r")
                    .found("/r", Some(worktree("/r", "/r", "main")))
                    .worktree(worktree("/r", "/r", "main"))
                    .worktree(worktree("/r/one", "/r", "try-1"))
                    .worktree(worktree("/r/two", "/r", "try-2")),
            ),
        );
        let tree = crate::tree::listed(run.state().hosts.work(&here()));
        let (one, two) = (attempt(1, Some("/r/one")), attempt(2, Some("/r/two")));

        assert_eq!(
            compare(&here(), tree, &one, &two),
            Ok(Msg::OpenDiff {
                environment: here(),
                worktree: "/r/two".into(),
                spot: Spot::Stream(Rev::Range("try-1".into(), "try-2".into())),
                neovim: false,
                line: None,
            })
        );
        let missing = attempt(3, Some("/r/gone"));
        assert!(compare(&here(), tree, &one, &missing).is_err());
    }
}
