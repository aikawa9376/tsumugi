//! The sidebar's independent directory and agent-family views.
use std::collections::{BTreeMap, BTreeSet};

use tsumugi_model::{EnvironmentId, Session, State};

use crate::host::Hosts;
use crate::tree;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Tree,
    Agents,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Identity {
    Session(String),
    Parent(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeKey {
    pub environment: EnvironmentId,
    pub identity: Identity,
}

impl NodeKey {
    pub fn session(environment: EnvironmentId, id: impl Into<String>) -> Self {
        Self {
            environment,
            identity: Identity::Session(id.into()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    pub starting: usize,
    pub working: usize,
    pub background: usize,
    pub waiting: usize,
    pub idle: usize,
    pub done: usize,
    pub failed: usize,
}

impl Counts {
    pub fn active(&self) -> usize {
        self.starting + self.working + self.background
    }
    pub fn total(&self) -> usize {
        self.active() + self.waiting + self.idle + self.done + self.failed
    }
    fn add_state(&mut self, state: &State) {
        match state {
            State::Starting => self.starting += 1,
            State::Working => self.working += 1,
            State::Background => self.background += 1,
            State::Waiting(_) => self.waiting += 1,
            State::Idle => self.idle += 1,
            State::Done => self.done += 1,
            State::Failed => self.failed += 1,
        }
    }
    fn add(&mut self, other: &Self) {
        self.starting += other.starting;
        self.working += other.working;
        self.background += other.background;
        self.waiting += other.waiting;
        self.idle += other.idle;
        self.done += other.done;
        self.failed += other.failed;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub key: NodeKey,
    pub session: Option<Session>,
    pub parent: Option<NodeKey>,
    pub depth: usize,
    pub children: usize,
    pub counts: Counts,
    pub open: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sidebar {
    pub panel: Panel,
    pub selected: Option<NodeKey>,
    pub collapsed: BTreeSet<NodeKey>,
    pub filter: String,
    pub filtering: bool,
    pub totals: Counts,
    rows: Vec<Row>,
    all: Vec<Row>,
    built: Option<BuiltFrom>,
}

type BuiltFrom = (
    Vec<(EnvironmentId, Option<tsumugi_protocol::Seq>)>,
    String,
    BTreeSet<NodeKey>,
);

impl Sidebar {
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub(crate) fn rebuild(&mut self, hosts: &Hosts) {
        let from = (
            hosts
                .each()
                .map(|(e, h)| (e.clone(), h.work.as_ref().map(|w| w.seq)))
                .collect(),
            self.filter.clone(),
            self.collapsed.clone(),
        );
        if self.built.as_ref() == Some(&from) {
            return;
        }
        self.built = Some(from);
        let mut old_parent = self
            .selected
            .as_ref()
            .and_then(|key| self.all.iter().find(|r| &r.key == key))
            .and_then(|r| r.parent.clone());
        let mut nodes = BTreeMap::new();
        let mut identities: BTreeMap<_, Vec<NodeKey>> = BTreeMap::new();
        self.totals = Counts::default();
        for (environment, host) in hosts.each() {
            for (_, session) in tree::listed(host.work.as_ref()).sessions() {
                let key = NodeKey::session(environment.clone(), &session.id);
                if nodes.contains_key(&key) {
                    continue;
                }
                self.totals.add_state(&session.state);
                if let Some(id) = session.delegation {
                    identities
                        .entry((environment.clone(), id))
                        .or_default()
                        .push(key.clone());
                }
                nodes.insert(
                    key.clone(),
                    Row {
                        key,
                        session: Some(session.clone()),
                        parent: None,
                        depth: 0,
                        children: 0,
                        counts: Counts::default(),
                        open: true,
                    },
                );
            }
        }
        let mut parents = BTreeMap::new();
        for (key, row) in &nodes {
            let parent = row.session.as_ref().and_then(|s| s.parent).map(|id| {
                match identities
                    .get(&(key.environment.clone(), id))
                    .map(Vec::as_slice)
                {
                    Some([parent]) => parent.clone(),
                    _ => NodeKey {
                        environment: key.environment.clone(),
                        identity: Identity::Parent(id.to_string()),
                    },
                }
            });
            parents.insert(key.clone(), parent);
        }
        for parent in parents.values().flatten() {
            nodes.entry(parent.clone()).or_insert_with(|| Row {
                key: parent.clone(),
                session: None,
                parent: None,
                depth: 0,
                children: 0,
                counts: Counts::default(),
                open: true,
            });
        }
        let mut children: BTreeMap<NodeKey, Vec<NodeKey>> = BTreeMap::new();
        let mut ordered: Vec<_> = nodes.keys().cloned().collect();
        ordered.sort_by_key(|key| {
            let session = nodes[key].session.as_ref();
            (
                key.environment.clone(),
                session.and_then(|s| s.started_ms).unwrap_or_default(),
                session.map(|s| s.name.to_lowercase()).unwrap_or_default(),
                key.clone(),
            )
        });
        for key in &ordered {
            if let Some(parent) = parents.get(key).and_then(Option::as_ref) {
                children
                    .entry(parent.clone())
                    .or_default()
                    .push(key.clone());
            }
        }
        self.all.clear();
        let mut visited = BTreeSet::new();
        // Real roots first; a remaining cycle gets one deterministic root.
        for (environment, _) in hosts.each() {
            let in_environment = || ordered.iter().filter(|key| &key.environment == environment);
            for key in in_environment()
                .filter(|k| parents.get(*k).is_none_or(Option::is_none))
                .chain(in_environment())
            {
                append(key, None, 0, &nodes, &children, &mut visited, &mut self.all);
            }
        }
        let mut keep = BTreeSet::new();
        let filter = self.filter.to_lowercase();
        for row in &self.all {
            let matches = filter.is_empty()
                || row.session.as_ref().is_some_and(|s| {
                    [
                        s.name.clone(),
                        s.id.clone(),
                        s.code.as_ref().map(ToString::to_string).unwrap_or_default(),
                        s.cwd.to_string_lossy().into_owned(),
                        s.agent.label().to_owned(),
                    ]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&filter))
                });
            if matches {
                let mut at = Some(&row.key);
                while let Some(key) = at {
                    if !keep.insert(key.clone()) {
                        break;
                    }
                    at = self
                        .all
                        .iter()
                        .find(|r| &r.key == key)
                        .and_then(|r| r.parent.as_ref());
                }
            }
        }
        self.rows.clear();
        let mut hidden = None;
        for row in &self.all {
            if hidden.is_some_and(|depth| row.depth > depth) {
                continue;
            }
            hidden = None;
            if !keep.contains(&row.key) {
                continue;
            }
            let mut row = row.clone();
            row.open =
                row.children > 0 && (!self.collapsed.contains(&row.key) || !filter.is_empty());
            if row.children > 0 && !row.open {
                hidden = Some(row.depth);
            }
            self.rows.push(row);
        }
        let mut selected = self.selected.clone();
        while let Some(key) = selected.as_ref() {
            if self.rows.iter().any(|r| &r.key == key) {
                break;
            }
            selected = match self.all.iter().find(|r| &r.key == key) {
                Some(row) => row.parent.clone(),
                None => old_parent.take(),
            };
        }
        self.selected = selected.or_else(|| self.rows.first().map(|r| r.key.clone()));
    }
}

fn append(
    key: &NodeKey,
    parent: Option<&NodeKey>,
    depth: usize,
    nodes: &BTreeMap<NodeKey, Row>,
    children: &BTreeMap<NodeKey, Vec<NodeKey>>,
    visited: &mut BTreeSet<NodeKey>,
    rows: &mut Vec<Row>,
) -> Option<Counts> {
    if !visited.insert(key.clone()) {
        return None;
    }
    let mut row = nodes[key].clone();
    row.parent = parent.cloned();
    row.depth = depth;
    if let Some(session) = &row.session {
        row.counts.add_state(&session.state);
    }
    let at = rows.len();
    rows.push(row);
    for child in children.get(key).into_iter().flatten() {
        if let Some(counts) = append(child, Some(key), depth + 1, nodes, children, visited, rows) {
            rows[at].children += 1;
            rows[at].counts.add(&counts);
        }
    }
    Some(rows[at].counts.clone())
}

pub(crate) fn switch(
    state: &mut crate::state::ViewState,
    panel: Panel,
) -> Vec<crate::effect::Effect> {
    use crate::effect::Effect;
    if state.sidebar.panel != panel {
        state.overlays.rename = None;
        state.overlays.naming = None;
    }
    state.sidebar.panel = panel;
    state.content.focus = crate::content::Area::Sidebar;
    state.console.pending.clear();
    let mut effects = crate::chrome::panels::show(
        &mut state.chrome.panels,
        crate::chrome::panels::layout::Side::Sidebar,
    );
    if state.tree.filtering || state.sidebar.filtering {
        state.tree.filtering = false;
        state.sidebar.filtering = false;
        effects.push(Effect::FilterFocus(false));
    }
    effects
}

/// The sidebar row commands act on, independently of the other tab's cursor.
pub fn selected(state: &crate::state::ViewState) -> Option<tree::nav::Item> {
    if state.content.focus == crate::content::Area::Sidebar && state.sidebar.panel == Panel::Agents
    {
        let key = state.sidebar.selected.as_ref()?;
        match &key.identity {
            Identity::Session(id) => Some(tree::nav::Item::Session(
                key.environment.clone(),
                id.clone(),
            )),
            Identity::Parent(_) => None,
        }
    } else {
        state.tree.selected.clone()
    }
}

/// Agent bindings replace directory bindings, including those of a hidden file.
pub fn contexts(
    panel: Panel,
    area: crate::content::Area,
    mut contexts: Vec<crate::keymap::Context>,
) -> Vec<crate::keymap::Context> {
    use crate::keymap::Context;
    if panel == Panel::Agents && area == crate::content::Area::Sidebar {
        contexts.retain(|context| *context != Context::Filer);
        for context in &mut contexts {
            if *context == Context::Tree {
                *context = Context::Agents;
            }
        }
    }
    contexts
}

pub(crate) fn press(
    state: &mut crate::state::ViewState,
    action: &crate::actions::Action,
) -> Option<Vec<crate::effect::Effect>> {
    use crate::actions::Action;
    use crate::content::{Area, Opening};
    if state.content.focus != Area::Sidebar || state.sidebar.panel != Panel::Agents {
        return None;
    }
    let view = &mut state.sidebar;
    let at = view
        .selected
        .as_ref()
        .and_then(|key| view.rows.iter().position(|r| &r.key == key))
        .unwrap_or_default();
    match action {
        Action::CursorDown | Action::CursorUp | Action::CursorFirst | Action::CursorLast => {
            if !view.rows.is_empty() {
                let last = view.rows.len() - 1;
                let at = match action {
                    Action::CursorDown => (at + 1).min(last),
                    Action::CursorUp => at.saturating_sub(1),
                    Action::CursorLast => last,
                    _ => 0,
                };
                view.selected = Some(view.rows[at].key.clone());
            }
        }
        Action::AgentsCollapse | Action::CursorFold => {
            if let Some(row) = view.rows.get(at) {
                if row.children > 0 && row.open {
                    view.collapsed.insert(row.key.clone());
                } else if let Some(parent) = &row.parent {
                    view.selected = Some(parent.clone());
                }
            }
        }
        Action::AgentsExpand => {
            if let Some(row) = view.rows.get(at) {
                if row.children > 0 {
                    if row.open {
                        view.selected = view.rows.get(at + 1).map(|r| r.key.clone());
                    } else {
                        view.collapsed.remove(&row.key);
                    }
                } else if let Some(item) = selected(state) {
                    return Some(crate::content::open_row(
                        state,
                        item,
                        Some(Opening::Existing),
                        true,
                    ));
                }
            }
        }
        _ => return None,
    }
    Some(Vec::new())
}

pub(crate) fn toggle(view: &mut Sidebar, key: NodeKey) {
    if view.rows.iter().any(|r| r.key == key && r.children > 0) && !view.collapsed.remove(&key) {
        view.collapsed.insert(key);
    }
}

#[cfg(test)]
mod tests;
