//! What the picker going to a session holds while it is open: its list, the
//! directory tree beside it, and what each host offered to list.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tsumugi_model::EnvironmentId;
use tsumugi_protocol::Offers;

use super::Choice;
use super::filer::{Filer, Row};
use super::list::ListState;
use super::scopes::{self, Aim};
use crate::settings::PickerScope;
use crate::tree::Place;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    List,
    Tree,
}

/// What choosing a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Purpose {
    /// Go to the session, or start one in the directory.
    #[default]
    Go,
    /// Add the directory to the tree; only directories are offered.
    Add,
}

/// What the picker holds while it is open.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GoPicker {
    pub list: ListState,
    pub mode: Mode,
    pub purpose: Purpose,
    pub filer: Filer,
    /// What was typed, where the cursor was and the row it was on when the
    /// rows were last drawn.
    pub(crate) held: Option<(ListState, Choice)>,
    /// The directories the tree listed when the picker opened, by the
    /// environment they are in.
    pub running: BTreeMap<EnvironmentId, Vec<PathBuf>>,
    /// What each environment's host offered, as the list ranks it; none
    /// from a host until it is listed.
    pub offered: BTreeMap<EnvironmentId, Offers>,
    /// The directory opened before it was listed, which the cursor moves
    /// into once it is.
    pub(crate) entering: Option<Row>,
    /// The environment the directory tree browses, and the scratch row
    /// starts in.
    pub environment: EnvironmentId,
    /// The home directory of that environment's host, where the tree starts.
    pub home: PathBuf,
    /// The mixes of sources the settings name, as they were when the picker
    /// opened.
    pub scopes: Vec<PickerScope>,
    /// The scope the picker is stepped to; a prefix typed first overrides it.
    pub scope: usize,
    /// The worktree shown when the picker opened, whose files, and whose
    /// project's resources, come first.
    pub place: Option<Place>,
}

impl GoPicker {
    pub fn new(filer: Filer) -> Self {
        Self {
            filer,
            scopes: PickerScope::defaults(),
            ..Self::default()
        }
    }

    /// What the query asks of the picker: the scope it is in, the sources
    /// searched and what is searched for. Adding to the tree offers only
    /// directories, whatever the scopes say.
    pub fn aim(&self) -> Aim<'_> {
        match self.purpose {
            Purpose::Add => scopes::directories_only(&self.list.query),
            Purpose::Go => scopes::resolve(&self.scopes, self.scope, &self.list.query),
        }
    }

    /// Step to the next scope, or the previous, going round.
    pub fn step_scope(&mut self, delta: isize) {
        let at = crate::cursor::wrap(self.aim().scope, delta, self.scopes.len());
        self.set_scope(at);
    }

    /// Go to the scope `at`; the cursor goes back to the first row, and what
    /// is typed stays, less the prefix that would put the picker back.
    pub fn set_scope(&mut self, at: usize) {
        if self.purpose == Purpose::Add || at >= self.scopes.len() {
            return;
        }
        self.list.query = self.aim().query.to_string();
        self.scope = at;
        self.list.cursor = 0;
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::List => Mode::Tree,
            Mode::Tree => Mode::List,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_opens_on_the_list_and_toggles_to_the_tree_and_back() {
        let mut p = GoPicker::new(Filer::new(["/w"]));
        assert_eq!(p.mode, Mode::List);
        p.toggle_mode();
        assert_eq!(p.mode, Mode::Tree);
        p.toggle_mode();
        assert_eq!(p.mode, Mode::List);
    }
}
