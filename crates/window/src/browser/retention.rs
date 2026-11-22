//! Hidden pages freed to give back their views' memory, and what a freed page
//! keeps to come back as it was: its address, its scroll position and the
//! address its bar was being typed into. The policy that names the pages is
//! the core's; this holds what a page keeps and does the freeing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use dioxus::prelude::ReadableExt;

use tsumugi_core::browser::retention::Retention;
use tsumugi_core::content::BrowserId;

use super::{Page, Pool};
use crate::bridge::AppCtx;

/// What a freed page keeps.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Saved {
    /// The address as the environment itself writes it.
    pub address: String,
    pub scroll: [f64; 2],
}

#[derive(Default)]
struct Keep {
    retention: Retention,
    saved: HashMap<BrowserId, Saved>,
    /// What was typed into each page's address bar and not yet entered.
    typed: HashMap<BrowserId, String>,
}

impl Keep {
    fn free(&mut self, id: BrowserId, saved: Saved) {
        self.saved.insert(id, saved);
        self.retention.free(id);
    }

    fn take(&mut self, id: BrowserId) -> Option<Saved> {
        self.retention.thaw(id);
        self.saved.remove(&id)
    }

    /// The page `id` is closed; what it kept is let go, and the address it
    /// was freed at is handed back.
    fn forget(&mut self, id: BrowserId) -> Option<String> {
        self.typed.remove(&id);
        self.saved.remove(&id).map(|saved| saved.address)
    }
}

thread_local! {
    static KEEP: RefCell<Keep> = RefCell::new(Keep::default());
    static STARTED: Instant = Instant::now();
}

/// Where a page says it is scrolled to: `scroll:<x>,<y>`, in pixels from the
/// top left of the document.
pub(super) fn scrolled(message: &str) -> Option<[f64; 2]> {
    let (x, y) = message.strip_prefix("scroll:")?.split_once(',')?;
    let at = [x.parse::<f64>().ok()?, y.parse::<f64>().ok()?];
    at.iter().all(|v| v.is_finite() && *v >= 0.0).then_some(at)
}

/// What the page `id` kept when it was freed, still waiting to be loaded again.
pub(super) fn saved(id: BrowserId) -> Option<Saved> {
    KEEP.with(|keep| keep.borrow().saved.get(&id).cloned())
}

/// The pages freed now.
pub(super) fn freed() -> std::collections::HashSet<BrowserId> {
    KEEP.with(|keep| keep.borrow().saved.keys().copied().collect())
}

/// The page `id` is made again: what it kept is handed over to load.
pub(super) fn take(id: BrowserId) -> Option<Saved> {
    KEEP.with(|keep| keep.borrow_mut().take(id))
}

/// The page `id` closed. Its address, when it was freed, is what to reopen.
pub(super) fn forget(id: BrowserId) -> Option<String> {
    KEEP.with(|keep| keep.borrow_mut().forget(id))
}

/// `text` is what is typed into the address bar of the page `id` and not yet
/// entered; none when the bar shows the page's own address.
pub(super) fn type_into(id: BrowserId, text: Option<String>) {
    KEEP.with(|keep| {
        let mut keep = keep.borrow_mut();
        match text {
            Some(text) => keep.typed.insert(id, text),
            None => keep.typed.remove(&id),
        };
    });
}

/// What is typed into the address bar of the page `id` and not yet entered.
pub(super) fn typed(id: BrowserId) -> Option<String> {
    KEEP.with(|keep| keep.borrow().typed.get(&id).cloned())
}

/// Whether the page can be let go of without losing work: nothing holds it,
/// no field in it was edited, and no note on it waits to be sent. A command
/// of an agent holds the page for as long as it runs.
fn idle(page: &Rc<Page>) -> bool {
    Rc::strong_count(page) == 1
        && !page.edited.get()
        && page.notes.borrow().is_empty()
        && !page.selection.borrow().armed
}

/// Free the pages that have gone unviewed for as long as the settings say.
/// `listed` is every page open, those of every worktree's layout included.
pub(super) fn sweep(app: AppCtx, pool: &mut Pool, listed: &[BrowserId]) {
    let now = STARTED.with(|started| started.elapsed());
    let shown = |id| app.content.peek().window_of_page(id).is_some();
    let idle_for = app.page_idle_limit();
    let due = KEEP.with(|keep| {
        let mut keep = keep.borrow_mut();
        keep.retention.observe(now, listed, shown);
        idle_for.map(|idle_for| {
            keep.retention
                .due(now, idle_for, |id| pool.pages.get(&id).is_some_and(idle))
        })
    });
    for id in due.unwrap_or_default() {
        free(app, pool, id);
    }
}

fn free(app: AppCtx, pool: &mut Pool, id: BrowserId) {
    let Some(page) = pool.pages.remove(&id) else {
        return;
    };
    let url = page.view.url().unwrap_or_default();
    // A page that was restored while its work machine was away shows nothing
    // yet; the address it was to load is the one it keeps.
    let address = if url.is_empty() || url == "about:blank" {
        page.home.borrow().clone()
    } else {
        page.ports
            .borrow_mut()
            .shown(&url, |local| app.work_port(&page.environment, local))
    };
    let saved = Saved {
        address,
        scroll: page.scroll.get(),
    };
    KEEP.with(|keep| keep.borrow_mut().free(id, saved));
    // The view and the memory it holds go with the last hold on the page.
    drop(page);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn saved_at(address: &str, y: f64) -> Saved {
        Saved {
            address: address.into(),
            scroll: [0.0, y],
        }
    }

    #[test]
    fn a_freed_page_keeps_its_address_and_scroll_until_it_is_made_again() {
        let mut keep = Keep::default();
        let id = BrowserId::new();
        keep.retention.observe(Duration::ZERO, &[id], |_| false);
        keep.free(id, saved_at("http://localhost:3000/a", 480.0));
        assert!(keep.retention.is_free(id));
        assert_eq!(
            keep.take(id),
            Some(saved_at("http://localhost:3000/a", 480.0))
        );
        assert!(!keep.retention.is_free(id));
        assert_eq!(keep.take(id), None);
    }

    #[test]
    fn a_page_tells_where_it_is_scrolled_to_and_nothing_else_is_taken_for_it() {
        assert_eq!(scrolled("scroll:0,480.5"), Some([0.0, 480.5]));
        for message in [
            "scroll:",
            "scroll:1",
            "scroll:a,b",
            "scroll:-1,2",
            "scroll:NaN,1",
            "focus",
        ] {
            assert_eq!(scrolled(message), None, "{message}");
        }
    }

    #[test]
    fn what_is_typed_into_the_bar_is_kept_for_the_page_it_was_typed_in() {
        let (a, b) = (BrowserId::new(), BrowserId::new());
        type_into(a, Some("exam".into()));
        assert_eq!(typed(a).as_deref(), Some("exam"));
        assert_eq!(typed(b), None);
        type_into(a, None);
        assert_eq!(typed(a), None);
    }

    #[test]
    fn closing_a_freed_page_hands_back_its_address_and_forgets_the_rest() {
        let mut keep = Keep::default();
        let id = BrowserId::new();
        keep.free(id, saved_at("https://example.com/", 0.0));
        keep.typed.insert(id, "half".into());
        assert_eq!(keep.forget(id).as_deref(), Some("https://example.com/"));
        assert!(keep.typed.is_empty() && keep.saved.is_empty());
        assert_eq!(keep.forget(id), None);
    }
}
