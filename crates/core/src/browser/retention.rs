//! Which hidden browser pages are due to be freed. Nothing here touches a
//! page: the window frees the ones named and the core only keeps the time.

use std::collections::HashMap;
use std::time::Duration;

use crate::content::BrowserId;

#[cfg(test)]
mod tests;

/// How many times a page may be brought back into view before it is no
/// longer freed: a page swapped in and out that often is one in use.
pub const REVISITS: u32 = 15;

#[derive(Debug, Default)]
struct Seen {
    /// When the page went out of view, none while it is in view.
    hidden_since: Option<Duration>,
    revisits: u32,
    free: bool,
}

/// How long each page has been out of view, by the page's own identity: the
/// windows of different worktrees' layouts are numbered each from 1, so a
/// window number says nothing about which page is meant.
#[derive(Debug, Default)]
pub struct Retention {
    pages: HashMap<BrowserId, Seen>,
}

impl Retention {
    /// Take in how the pages stand at `now`, a time that only grows: `open`
    /// lists every page that is open, and `shown` says which of them a
    /// window displays.
    pub fn observe(
        &mut self,
        now: Duration,
        open: &[BrowserId],
        shown: impl Fn(BrowserId) -> bool,
    ) {
        self.pages.retain(|id, _| open.contains(id));
        for &id in open {
            let seen = self.pages.entry(id).or_default();
            if shown(id) {
                if seen.hidden_since.take().is_some() {
                    seen.revisits += 1;
                }
            } else if seen.hidden_since.is_none() {
                seen.hidden_since = Some(now);
            }
        }
    }

    /// The pages to free at `now`, the one out of view longest first: those
    /// out of view for `idle` that `can_free` lets go of, and not freed yet.
    pub fn due(
        &self,
        now: Duration,
        idle: Duration,
        can_free: impl Fn(BrowserId) -> bool,
    ) -> Vec<BrowserId> {
        let mut due: Vec<_> = self
            .pages
            .iter()
            .filter_map(|(id, seen)| Some((*id, seen.hidden_since?, seen)))
            .filter(|(id, since, seen)| {
                !seen.free
                    && seen.revisits <= REVISITS
                    && now.saturating_sub(*since) >= idle
                    && can_free(*id)
            })
            .map(|(id, since, _)| (since, id))
            .collect();
        due.sort();
        due.into_iter().map(|(_, id)| id).collect()
    }

    /// The page `id` was freed.
    pub fn free(&mut self, id: BrowserId) {
        if let Some(seen) = self.pages.get_mut(&id) {
            seen.free = true;
        }
    }

    /// The page `id` is loaded again; if it is not shown, its idle time
    /// starts over from the next look.
    pub fn thaw(&mut self, id: BrowserId) {
        if let Some(seen) = self.pages.get_mut(&id) {
            seen.free = false;
            seen.hidden_since = None;
        }
    }

    /// Whether the page `id` is freed.
    pub fn is_free(&self, id: BrowserId) -> bool {
        self.pages.get(&id).is_some_and(|seen| seen.free)
    }
}
