//! The pages closed lately, which reopening brings back.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dioxus::prelude::ReadableExt;
use tsumugi_core::content::{BrowserId, Opening, Tab};
use tsumugi_core::msg::Msg;
use tsumugi_core::tree::nav::Page as Listed;
use tsumugi_model::EnvironmentId;

use crate::bridge::AppCtx;

thread_local! {
    static CLOSED: RefCell<Closed> = RefCell::new(Closed::default());
    /// Where each page hangs, kept while it is listed: a page is gone from
    /// the content by the time the pool lets it go.
    static ORIGINS: RefCell<HashMap<BrowserId, (EnvironmentId, PathBuf)>> =
        RefCell::new(HashMap::new());
}

/// Remember where the pages listed hang.
pub(super) fn track(listed: &[(EnvironmentId, Listed)]) {
    ORIGINS.with(|origins| {
        let mut origins = origins.borrow_mut();
        for (environment, page) in listed {
            origins
                .entry(page.id)
                .or_insert_with(|| (environment.clone(), page.worktree.clone()));
        }
    });
}

/// The page `id`, which was at `url` as its environment writes it, is closed:
/// it can come back.
pub(super) fn closing_at(id: BrowserId, url: String) {
    let Some((environment, worktree)) = ORIGINS.with(|origins| origins.borrow_mut().remove(&id))
    else {
        return;
    };
    CLOSED.with(|closed| closed.borrow_mut().close(environment, worktree, url));
}

/// Open `url` in a new page hanging under `worktree` of `environment`, as
/// `how` says.
pub(crate) fn open_at(
    app: AppCtx,
    environment: EnvironmentId,
    worktree: PathBuf,
    url: String,
    how: Opening,
) {
    // A link asks for a page, not for another of the same; a new tab or
    // window that is asked for by name is still opened.
    let listed = app.content.peek().pages();
    let existing = matches!(how, Opening::Existing)
        .then(|| super::reuse::existing(&listed, &environment, &worktree, &url))
        .flatten();
    let thing = match existing {
        Some(id) => Tab::Browser {
            environment: environment.clone(),
            worktree: worktree.clone(),
            id,
        },
        None => {
            let thing = Tab::browser(environment.clone(), worktree.clone());
            if let Tab::Browser { id, .. } = &thing {
                super::start_at(*id, url);
            }
            thing
        }
    };
    app.dispatch(Msg::Show {
        thing,
        place: Some(tsumugi_core::tree::Place::at(&environment, worktree)),
        how: Some(how),
    });
}

/// Open the page closed last under `worktree` of `environment` again.
pub(crate) fn reopen(app: AppCtx, environment: EnvironmentId, worktree: PathBuf) {
    let url = CLOSED.with(|closed| closed.borrow_mut().reopen(&environment, &worktree));
    match url {
        Some(url) => open_at(app, environment, worktree, url, Opening::Existing),
        None => app.notice("No closed page to reopen".into()),
    }
}

/// How many closed pages a worktree keeps to reopen.
const KEPT: usize = 10;

struct Page {
    environment: EnvironmentId,
    worktree: PathBuf,
    url: String,
}

/// The pages closed in this run of the window, the last closed at the end.
#[derive(Default)]
pub(super) struct Closed(Vec<Page>);

impl Closed {
    /// Keep the page of `environment` under `worktree`, closed at `url`. A
    /// page that shows nothing yet is not worth coming back to.
    pub(super) fn close(&mut self, environment: EnvironmentId, worktree: PathBuf, url: String) {
        if url.is_empty() || url == "about:blank" {
            return;
        }
        let same = |p: &Page| p.environment == environment && p.worktree == worktree;
        if self.0.iter().filter(|p| same(p)).count() >= KEPT
            && let Some(oldest) = self.0.iter().position(same)
        {
            self.0.remove(oldest);
        }
        self.0.push(Page {
            environment,
            worktree,
            url,
        });
    }

    /// The address of the page closed last under `worktree` of
    /// `environment`, which is no longer kept.
    pub(super) fn reopen(
        &mut self,
        environment: &EnvironmentId,
        worktree: &Path,
    ) -> Option<String> {
        let at = self
            .0
            .iter()
            .rposition(|p| p.environment == *environment && p.worktree == worktree)?;
        Some(self.0.remove(at).url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> EnvironmentId {
        EnvironmentId::new(name)
    }

    #[test]
    fn the_page_closed_last_in_a_worktree_comes_back_first() {
        let mut closed = Closed::default();
        closed.close(env("a"), "/w".into(), "https://one.example/".into());
        closed.close(env("a"), "/w".into(), "https://two.example/".into());

        assert_eq!(
            closed.reopen(&env("a"), Path::new("/w")).as_deref(),
            Some("https://two.example/")
        );
        assert_eq!(
            closed.reopen(&env("a"), Path::new("/w")).as_deref(),
            Some("https://one.example/")
        );
        assert_eq!(closed.reopen(&env("a"), Path::new("/w")), None);
    }

    #[test]
    fn a_worktree_gets_back_only_its_own_pages() {
        let mut closed = Closed::default();
        closed.close(env("a"), "/w".into(), "https://one.example/".into());
        closed.close(env("b"), "/w".into(), "https://two.example/".into());
        closed.close(env("a"), "/v".into(), "https://three.example/".into());

        assert_eq!(
            closed.reopen(&env("a"), Path::new("/w")).as_deref(),
            Some("https://one.example/")
        );
        assert_eq!(closed.reopen(&env("a"), Path::new("/w")), None);
    }

    #[test]
    fn a_page_that_showed_nothing_is_not_kept() {
        let mut closed = Closed::default();
        closed.close(env("a"), "/w".into(), "about:blank".into());
        closed.close(env("a"), "/w".into(), String::new());

        assert_eq!(closed.reopen(&env("a"), Path::new("/w")), None);
    }

    #[test]
    fn a_worktree_keeps_the_last_ten_pages_and_others_are_left_alone() {
        let mut closed = Closed::default();
        closed.close(env("a"), "/v".into(), "https://other.example/".into());
        for n in 0..12 {
            closed.close(env("a"), "/w".into(), format!("https://{n}.example/"));
        }

        let mut back = Vec::new();
        while let Some(url) = closed.reopen(&env("a"), Path::new("/w")) {
            back.push(url);
        }
        assert_eq!(back.len(), KEPT);
        assert_eq!(
            back.first().map(String::as_str),
            Some("https://11.example/")
        );
        assert_eq!(back.last().map(String::as_str), Some("https://2.example/"));
        assert_eq!(
            closed.reopen(&env("a"), Path::new("/v")).as_deref(),
            Some("https://other.example/")
        );
    }
}
