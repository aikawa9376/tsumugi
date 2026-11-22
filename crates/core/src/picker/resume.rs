//! The list of the conversations the agents kept: which directory's it
//! lists, and what choosing a row does.

use std::path::{Path, PathBuf};

use tsumugi_model::{EnvironmentId, Past};
use tsumugi_protocol::{PastSight, WorkSnapshot};

use super::fuzzy::{Query, best};
use super::list::ListState;
use super::{close, short};
use crate::content::{self, Tab};
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::{self, nav};

/// The most conversations the list draws at once.
const ROWS: usize = 200;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resume {
    /// The environment of what the focused window showed when the list
    /// opened, whose agents' conversations it lists.
    pub environment: EnvironmentId,
    /// The directory the focused window worked in when the list opened.
    pub dir: Option<PathBuf>,
    /// Whether every directory's conversations are listed, rather than the
    /// directory's.
    pub every: bool,
    /// What the rows are grouped by.
    pub group: Group,
    pub list: ListState,
}

/// What the conversations of the list are grouped by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Group {
    #[default]
    None,
    Agent,
    Directory,
}

impl Group {
    /// The grouping after this one, going round to the first after the last.
    pub fn next(self) -> Self {
        match self {
            Group::None => Group::Agent,
            Group::Agent => Group::Directory,
            Group::Directory => Group::None,
        }
    }

    /// What the list says it is grouped by.
    pub fn label(self) -> &'static str {
        match self {
            Group::None => "Not grouped",
            Group::Agent => "By agent",
            Group::Directory => "By directory",
        }
    }
}

/// A conversation the list draws, with the session of this window that
/// holds it, if one does.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub past: Past,
    pub session: Option<String>,
    /// What its group is called, on the first row of each group.
    pub heading: Option<String>,
}

impl Resume {
    /// Which conversations the host is asked to list.
    pub fn sight(&self) -> PastSight {
        PastSight {
            dir: self.dir.clone().filter(|_| !self.every),
        }
    }

    /// Group the conversations another way.
    pub(crate) fn cycle_group(&mut self) {
        self.group = self.group.next();
    }

    /// List every directory's conversations, or the directory's again; with
    /// no directory, every directory's stay listed.
    pub(crate) fn toggle_every(&mut self) {
        if self.dir.is_some() {
            self.every = !self.every;
            self.list.cursor = 0;
        }
    }
}

/// The conversations listed for what the list shows, those its query
/// answers to, best first; none while they are being read. Each is matched
/// by its title and the directory it ran in as drawn, `home` written as `~`.
pub fn rows(list: &Resume, home: &Path, work: Option<&WorkSnapshot>) -> Option<Vec<Row>> {
    let pasts = work?.past.of(&list.sight())?;
    let query = Query::new(&list.list.query);
    let matched = best(
        pasts.iter().filter_map(|p| {
            let text = format!("{} {}", p.title, short(&p.cwd, home));
            query.rank_named(&text, &p.title).map(|rank| (rank, p))
        }),
        ROWS,
    );
    let sessions = work.map(|w| w.tree.sessions()).unwrap_or_default();
    let mut rows: Vec<Row> = matched
        .into_iter()
        .map(|p| Row {
            past: p.clone(),
            session: sessions
                .iter()
                .find(|s| s.agent == p.agent && s.session_id == p.id)
                .map(|s| s.id.clone()),
            heading: None,
        })
        .collect();
    group(&mut rows, list.group, home);
    Some(rows)
}

/// `rows` gathered by `by`, the groups in the order their best rows come
/// in and each group's rows in the order they were, the first of each
/// headed.
fn group(rows: &mut Vec<Row>, by: Group, home: &Path) {
    let key = |row: &Row| match by {
        Group::None => String::new(),
        Group::Agent => row.past.agent.label().to_string(),
        Group::Directory => short(&row.past.cwd, home),
    };
    if by == Group::None {
        return;
    }
    let mut groups: Vec<(String, Vec<Row>)> = Vec::new();
    for row in rows.drain(..) {
        let name = key(&row);
        match groups.iter_mut().find(|(group, _)| *group == name) {
            Some((_, members)) => members.push(row),
            None => groups.push((name, vec![row])),
        }
    }
    for (name, members) in groups {
        for (i, mut row) in members.into_iter().enumerate() {
            row.heading = (i == 0).then(|| name.clone());
            rows.push(row);
        }
    }
}

/// The list opened on the directory the focused window works in: the one
/// its session runs in, or else its worktree; every directory's with
/// nothing open.
pub(super) fn open(state: &ViewState) -> Resume {
    let showing = state.content.showing();
    let environment = content::shown_environment(state);
    let work = state.hosts.work(&environment);
    let session_dir = match &showing.tab {
        Some(Tab::Session(_, id)) => nav::find(tree::listed(work), id).map(|(_, s)| s.cwd.clone()),
        _ => None,
    };
    let dir = session_dir.or_else(|| showing.place.map(|p| p.worktree));
    Resume {
        environment,
        every: dir.is_none(),
        dir,
        ..Resume::default()
    }
}

/// The row `at`, with the environment the list is of.
fn row_at(state: &ViewState, at: usize) -> Option<(EnvironmentId, Row)> {
    let view = &state.picker;
    let Some(super::Picking::Resume(list)) = &view.open else {
        return None;
    };
    let mut rows = rows(list, &view.home, state.hosts.work(&list.environment))?;
    (at < rows.len()).then(|| (list.environment.clone(), rows.swap_remove(at)))
}

/// The row `at` chosen: the session that holds its conversation shown, or
/// else the conversation resumed in a new session. The list closes either
/// way.
pub(super) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let Some((environment, row)) = row_at(state, at) else {
        return Vec::new();
    };
    close(&mut state.picker);
    match row.session {
        Some(id) => content::show_session(state, environment, id),
        None => vec![Effect::Resume {
            environment,
            past: row.past,
        }],
    }
}

/// What a key chord does to the row under the cursor, when it does
/// anything: fork it, copy what resumes it or names it, or open its log.
/// None when `key` is not one of them.
pub(super) fn act(state: &mut ViewState, key: &str) -> Option<Vec<Effect>> {
    let at = state.picker.open.as_ref()?.cursor();
    let known = ["f", "y", "s", "l", "o"];
    if !known.contains(&key) {
        return None;
    }
    let Some((environment, row)) = row_at(state, at) else {
        return Some(Vec::new());
    };
    let past = row.past;
    let copy = |text: String, what: &str| match text.is_empty() {
        true => vec![Effect::Notice(format!("This conversation has no {what}"))],
        false => vec![Effect::Copy {
            text,
            done: format!("Copied the {what}"),
        }],
    };
    Some(match key {
        "f" => {
            close(&mut state.picker);
            vec![Effect::Fork { environment, past }]
        }
        "y" => copy(past.command, "resume command"),
        "s" => copy(past.id, "session ID"),
        "l" => copy(
            past.log
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            "log path",
        ),
        _ => match past.log {
            Some(path) => {
                close(&mut state.picker);
                vec![Effect::OpenFile {
                    environment,
                    path,
                    how: crate::content::Opening::Existing,
                }]
            }
            None => vec![Effect::Notice("This conversation has no log".into())],
        },
    })
}

#[cfg(test)]
mod tests;
