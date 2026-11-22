//! What an agent's browser commands may reach: the pages of the worktree its
//! session works in, in the environment it works in, and no others.

use std::path::{Path, PathBuf};

use tsumugi_model::EnvironmentId;

use crate::content::BrowserId;
use crate::state::ViewState;
use crate::tree;

/// The pages a session working in a directory may act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reach {
    /// The worktree that holds the directory, which the pages hang under.
    pub worktree: PathBuf,
    /// Its pages, in the order they were opened; a page's place in this list
    /// is the index `tab list` shows.
    pub pages: Vec<BrowserId>,
}

/// The pages of the worktree of `environment` that holds `dir`. A directory no
/// worktree of the environment holds reaches nothing.
pub fn reach(state: &ViewState, environment: &EnvironmentId, dir: &Path) -> Result<Reach, String> {
    let listed = tree::listed(state.hosts.work(environment));
    let worktree =
        tsumugi_model::innermost(listed.worktrees().map(|w| &w.worktree.path), dir).cloned();
    let Some(worktree) = worktree else {
        return Err(format!(
            "{} is in no worktree this window lists for the environment, so there are no browser pages to act on",
            dir.display()
        ));
    };
    let pages = state
        .content
        .pages()
        .into_iter()
        .filter(|(page_environment, page)| {
            page_environment == environment && page.worktree == worktree
        })
        .map(|(_, page)| page.id)
        .collect();
    Ok(Reach { worktree, pages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Tab;
    use crate::msg::Msg;
    use crate::scenario::Scenario;
    use crate::testing::{here, listed, session, snapshot, there};

    fn two_worktrees() -> Scenario {
        Scenario::new().event(
            snapshot().listed(
                listed()
                    .session(session("a").cwd("/r/a"))
                    .session(session("b").cwd("/r/b")),
            ),
        )
    }

    fn id(tab: &Tab) -> BrowserId {
        match tab {
            Tab::Browser { id, .. } => *id,
            _ => unreachable!(),
        }
    }

    #[test]
    fn a_session_reaches_the_pages_of_its_own_worktree_in_the_order_they_were_opened() {
        let (first, second, other) = (
            Tab::browser(here(), "/r/a"),
            Tab::browser(here(), "/r/a"),
            Tab::browser(here(), "/r/b"),
        );
        let run = two_worktrees()
            .send(Msg::OpenBehind(first.clone()))
            .send(Msg::OpenBehind(other.clone()))
            .send(Msg::OpenBehind(second.clone()));
        // A directory below the worktree's root is the worktree's.
        let reach = reach(run.state(), &here(), Path::new("/r/a/src")).unwrap();
        assert_eq!(reach.worktree, PathBuf::from("/r/a"));
        assert_eq!(reach.pages, [id(&first), id(&second)]);
    }

    #[test]
    fn a_session_cannot_reach_a_page_of_another_worktree_or_another_environment() {
        let theirs = Tab::browser(here(), "/r/b");
        let elsewhere = Tab::browser(there(), "/r/a");
        let run = two_worktrees()
            .send(Msg::OpenBehind(theirs))
            .send(Msg::OpenBehind(elsewhere));
        let reach = reach(run.state(), &here(), Path::new("/r/a")).unwrap();
        assert!(reach.pages.is_empty());
    }

    #[test]
    fn a_directory_no_worktree_holds_reaches_nothing_and_is_told_so() {
        let run = two_worktrees();
        let failed = reach(run.state(), &here(), Path::new("/elsewhere")).unwrap_err();
        assert!(failed.contains("/elsewhere"), "{failed}");
    }
}
