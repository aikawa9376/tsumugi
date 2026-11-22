//! A layout of tabs and windows for each worktree: selecting a worktree puts
//! away the layout shown and shows the last that worktree had.

use super::{Content, ContentView, Tab};
use crate::tree::Place;

/// The layouts put away, each by the worktree it is of, and the worktree the
/// layout shown is of.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Layouts {
    shown_of: Option<Place>,
    away: Vec<(Place, Content<Tab>)>,
}

impl Layouts {
    /// Show the layout of `place` in `windows`, putting the layout there away
    /// under the worktree it is of. Where none is yet of a worktree, `windows`
    /// is of `place`.
    pub(super) fn go_to(&mut self, windows: &mut Content<Tab>, place: &Place) {
        let Some(from) = self.shown_of.replace(place.clone()) else {
            return;
        };
        if from == *place {
            return;
        }
        let next = self
            .away
            .iter()
            .position(|(p, _)| p == place)
            .map(|at| self.away.remove(at).1)
            .unwrap_or_default();
        let put_away = std::mem::replace(windows, next);
        if !put_away.is_empty() {
            self.away.push((from, put_away));
        }
    }

    /// Every layout put away.
    pub(super) fn each(&self) -> impl Iterator<Item = &Content<Tab>> {
        self.away.iter().map(|(_, c)| c)
    }

    /// Change every layout put away with `f`, dropping those left empty.
    pub(super) fn edit(&mut self, f: impl Fn(&mut Content<Tab>)) {
        for (_, c) in &mut self.away {
            f(c);
        }
        self.away.retain(|(_, c)| !c.is_empty());
    }
}

impl ContentView {
    /// Show the layout of `place` and make it what the middle is of; what is
    /// opened next opens in it.
    pub(crate) fn go_to_place(&mut self, place: Place) {
        self.layouts.go_to(&mut self.windows, &place);
        self.place = Some(place);
    }

    /// Everything some window shows, in the layout shown or in one put away.
    pub fn shown_in_layouts(&self) -> Vec<&Tab> {
        let mut out = self.windows.shown();
        for tab in self.layouts.each().flat_map(|c| c.shown()) {
            if !out.contains(&tab) {
                out.push(tab);
            }
        }
        out
    }

    /// Whether some window of the layout shown or of one put away shows
    /// `thing`.
    pub fn shows_in_any_layout(&self, thing: &Tab) -> bool {
        self.windows.shows_anywhere(thing) || self.layouts.each().any(|c| c.shows_anywhere(thing))
    }
}
