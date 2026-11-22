//! The agent board: every session as a card in the lane of what it asks of
//! the user, read and moved over by keys.

use std::collections::{BTreeMap, BTreeSet};

use tsumugi_model::conversation::Role;
use tsumugi_model::{Agent, EnvironmentId, Session, State};

use crate::actions::Action;
use crate::completion::Completions;
use crate::content::{Opening, Tab};
use crate::effect::Effect;
use crate::host::Hosts;
use crate::state::ViewState;
use crate::tree;

/// How long a finished session the user has seen stays quiet before the
/// board counts it idle.
pub const IDLE_AFTER_MS: u64 = 30 * 60 * 1000;

/// How much of a message a card keeps.
const SAID: usize = 240;

/// A column of the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    /// Waiting on the user, or failed.
    NeedsYou,
    Working,
    /// Finished.
    Done,
    /// Quiet, with nothing left to look at.
    Idle,
}

impl Lane {
    pub const ALL: [Lane; 4] = [Lane::NeedsYou, Lane::Working, Lane::Done, Lane::Idle];

    pub const fn label(self) -> &'static str {
        match self {
            Lane::NeedsYou => "Needs You",
            Lane::Working => "Working",
            Lane::Done => "Done",
            Lane::Idle => "Idle",
        }
    }
}

/// The lane a session in `state` stands in, `quiet_ms` after its state last
/// changed or the user last wrote to it; 0 where that is not known.
pub fn lane_of(state: &State, unread: bool, quiet_ms: u64) -> Lane {
    match state {
        State::Waiting(_) | State::Failed => Lane::NeedsYou,
        State::Starting | State::Working | State::Background => Lane::Working,
        State::Done if unread || quiet_ms < IDLE_AFTER_MS => Lane::Done,
        State::Done | State::Idle => Lane::Idle,
    }
}

/// How long ago, in the largest whole unit.
fn age(ms: u64) -> String {
    let minutes = ms / 60_000;
    match minutes {
        0 => "just now".into(),
        1..60 => format!("{minutes}m"),
        60..1440 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// What a card stands for: a session of some environment.
pub type Key = (EnvironmentId, String);

/// A session another opened, under the card of its opener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub environment: EnvironmentId,
    pub id: String,
    pub name: String,
    pub state: State,
    pub lane: Lane,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub environment: EnvironmentId,
    pub id: String,
    pub name: String,
    pub agent: Agent,
    pub state: State,
    pub lane: Lane,
    pub unread: bool,
    /// The workspace it runs in.
    pub project: String,
    /// The worktree it runs in: its branch, or else its directory.
    pub worktree: String,
    pub last_user: Option<String>,
    pub last_agent: Option<String>,
    /// What it says it waits on, while it waits.
    pub ask: Option<String>,
    /// How long it has stood as it does; 0 where that is not known.
    pub quiet_ms: u64,
    /// The sessions its opener's family holds under it, while grouped.
    pub children: Vec<Child>,
    /// How many of them ask for the user.
    pub asking: usize,
    /// When its state last changed, as far as the board knows, for order.
    since: u64,
}

impl Card {
    /// How long ago it came to stand as it does.
    pub fn age(&self) -> String {
        age(self.quiet_ms)
    }

    /// How long it has been quiet.
    pub fn quiet(&self) -> String {
        format!("{} idle", age(self.quiet_ms))
    }

    pub fn key(&self) -> Key {
        (self.environment.clone(), self.id.clone())
    }

    fn text(&self) -> String {
        [
            Some(self.name.as_str()),
            Some(self.agent.label()),
            Some(self.agent.key()),
            Some(self.project.as_str()),
            Some(self.worktree.as_str()),
            Some(self.state.word()),
            self.last_user.as_deref(),
            self.last_agent.as_deref(),
            self.ask.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
    }
}

/// The cards of one lane, the newest change first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneCards {
    pub lane: Lane,
    pub cards: Vec<Card>,
}

/// A project the board can be narrowed to, with what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardView {
    /// Whether the lane of idle sessions is shown.
    pub show_idle: bool,
    pub query: String,
    /// The projects the board is narrowed to; every project when empty.
    pub projects: BTreeSet<String>,
    /// The lanes the board is narrowed to; every lane when empty.
    pub states: BTreeSet<Lane>,
    /// Whether the sessions an opener opened fold under its card.
    pub group: bool,
    /// The cards whose children are listed.
    pub expanded: BTreeSet<Key>,
    /// Whether the search field has the keys.
    pub searching: bool,
    now_ms: u64,
    /// The state each session was last seen in, and when it was seen to
    /// change into it; none for one that stood so when first seen.
    seen: BTreeMap<Key, (&'static str, Option<u64>)>,
    selected: Option<Key>,
    /// Where the cursor was, for when its card is gone.
    fallback: (usize, usize),
}

impl BoardView {
    pub fn selected(&self) -> Option<&Key> {
        self.selected.as_ref()
    }

    /// Know what time it is.
    pub(crate) fn tick(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// When the session was last quiet or changed, in milliseconds since the
    /// Unix epoch; 0 where nothing says.
    fn since(&self, key: &Key, session: &Session) -> u64 {
        let seen = self.seen.get(key).and_then(|(_, at)| *at);
        [seen, session.last_input_ms, session.started_ms]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(0)
    }

    fn all(&self, hosts: &Hosts, completions: &Completions) -> Vec<Card> {
        let mut cards = Vec::new();
        for (environment, host) in hosts.each() {
            let Some(work) = host.work.as_ref() else {
                continue;
            };
            for workspace in &tree::listed(Some(work)).workspaces {
                for worktree in &workspace.worktrees {
                    for session in &worktree.sessions {
                        let key = (environment.clone(), session.id.clone());
                        let since = self.since(&key, session);
                        let quiet_ms = if since == 0 || self.now_ms == 0 {
                            0
                        } else {
                            self.now_ms.saturating_sub(since)
                        };
                        let unread = completions.unread(session);
                        let said = said(hosts, environment, session);
                        cards.push(Card {
                            environment: environment.clone(),
                            id: session.id.clone(),
                            name: session.name.clone(),
                            agent: session.agent,
                            lane: lane_of(&session.state, unread, quiet_ms),
                            ask: match &session.state {
                                State::Waiting(_) => session.status_message.clone(),
                                _ => None,
                            },
                            state: session.state.clone(),
                            unread,
                            project: workspace.name.clone(),
                            worktree: worktree
                                .worktree
                                .branch
                                .clone()
                                .or_else(|| {
                                    worktree
                                        .worktree
                                        .path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                })
                                .unwrap_or_default(),
                            last_user: said.0,
                            last_agent: said.1,
                            quiet_ms,
                            children: Vec::new(),
                            asking: 0,
                            since,
                        });
                    }
                }
            }
        }
        cards
    }

    /// The card of every session that has one, apart or grouped, narrowed
    /// by the filters, each in the lane it stands in.
    fn cards(&self, hosts: &Hosts, completions: &Completions) -> Vec<Card> {
        let all = self.all(hosts, completions);
        let families = if self.group {
            group(hosts, all)
        } else {
            all.into_iter().map(|c| (c, Vec::new())).collect()
        };
        let query = self.query.trim().to_lowercase();
        families
            .into_iter()
            .filter(|(root, members)| {
                let family = || std::iter::once(root).chain(members);
                (query.is_empty() || family().any(|c| c.text().contains(&query)))
                    && (self.projects.is_empty()
                        || family().any(|c| self.projects.contains(&c.project)))
                    && (self.states.is_empty() || self.states.contains(&root.lane))
            })
            .map(|(mut root, members)| {
                root.asking = members.iter().filter(|c| c.lane == Lane::NeedsYou).count();
                root.children = members
                    .into_iter()
                    .map(|c| Child {
                        environment: c.environment,
                        id: c.id,
                        name: c.name,
                        state: c.state,
                        lane: c.lane,
                    })
                    .collect();
                root
            })
            .collect()
    }

    /// The lanes the board shows, the idle one only on request and only
    /// those a state filter names, each with its cards.
    pub fn lanes(&self, hosts: &Hosts, completions: &Completions) -> Vec<LaneCards> {
        let cards = self.cards(hosts, completions);
        Lane::ALL
            .into_iter()
            .filter(|lane| *lane != Lane::Idle || self.show_idle)
            .filter(|lane| self.states.is_empty() || self.states.contains(lane))
            .map(|lane| {
                let mut cards: Vec<Card> =
                    cards.iter().filter(|c| c.lane == lane).cloned().collect();
                cards.sort_by(|a, b| {
                    b.since
                        .cmp(&a.since)
                        .then_with(|| a.name.cmp(&b.name))
                        .then_with(|| a.id.cmp(&b.id))
                });
                LaneCards { lane, cards }
            })
            .collect()
    }

    /// The projects of the sessions the board holds, with how many cards
    /// each has, before any filter.
    pub fn projects(&self, hosts: &Hosts, completions: &Completions) -> Vec<Project> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for card in self.all(hosts, completions) {
            *counts.entry(card.project).or_default() += 1;
        }
        counts
            .into_iter()
            .map(|(name, count)| Project { name, count })
            .collect()
    }

    /// Take in the states the hosts list: one seen to change counts from
    /// now, and the cursor stays on its card, or near where it was.
    pub(crate) fn sync(&mut self, hosts: &Hosts, completions: &Completions) {
        let now = (self.now_ms > 0).then_some(self.now_ms);
        let mut listed = BTreeSet::new();
        for (environment, host) in hosts.each() {
            let Some(work) = host.work.as_ref() else {
                continue;
            };
            for (_, session) in tree::listed(Some(work)).sessions() {
                let key = (environment.clone(), session.id.clone());
                let word = session.state.word();
                match self.seen.get(&key) {
                    Some((was, _)) if *was == word => {}
                    Some(_) => {
                        self.seen.insert(key.clone(), (word, now));
                    }
                    None => {
                        self.seen.insert(key.clone(), (word, None));
                    }
                }
                listed.insert(key);
            }
        }
        self.seen.retain(|key, _| listed.contains(key));
        let lanes = self.lanes(hosts, completions);
        match self.selected.as_ref().and_then(|key| locate(&lanes, key)) {
            Some(at) => self.fallback = at,
            None => {
                self.selected = near(&lanes, self.fallback).map(|(key, at)| {
                    self.fallback = at;
                    key
                });
            }
        }
    }
}

/// The last words of the user and of the agent in what its conversation
/// read, where the host reads it.
fn said(
    hosts: &Hosts,
    environment: &EnvironmentId,
    session: &Session,
) -> (Option<String>, Option<String>) {
    let tab = Tab::Session(environment.clone(), session.id.clone());
    let Some(read) = crate::content::conversation_read(hosts, &tab) else {
        return (None, None);
    };
    let last = |role: Role| {
        read.messages
            .iter()
            .rev()
            .find(|m| {
                m.role == role && !m.thinking && m.tool.is_none() && !m.text.trim().is_empty()
            })
            .map(|m| short(&m.text))
    };
    (last(Role::User), last(Role::Assistant))
}

/// `text` on one line, cut to what a card keeps.
fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(SAID) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line,
    }
}

/// `cards` with each opened session under the card of the session that
/// opened it, however far back: the cards left, each with its family. One
/// whose opener is not listed keeps a card of its own.
fn group(hosts: &Hosts, cards: Vec<Card>) -> Vec<(Card, Vec<Card>)> {
    let mut opener: BTreeMap<Key, Key> = BTreeMap::new();
    let mut delegated: BTreeMap<(EnvironmentId, tsumugi_model::SessionId), Key> = BTreeMap::new();
    let mut parents: BTreeMap<Key, tsumugi_model::SessionId> = BTreeMap::new();
    for (environment, host) in hosts.each() {
        let Some(work) = host.work.as_ref() else {
            continue;
        };
        for (_, session) in tree::listed(Some(work)).sessions() {
            let key = (environment.clone(), session.id.clone());
            if let Some(id) = session.delegation {
                delegated.insert((environment.clone(), id), key.clone());
            }
            if let Some(parent) = session.parent {
                parents.insert(key, parent);
            }
        }
    }
    for (key, parent) in &parents {
        if let Some(from) = delegated.get(&(key.0.clone(), *parent))
            && from != key
        {
            opener.insert(key.clone(), from.clone());
        }
    }
    let root_of = |key: &Key| {
        let mut at = key.clone();
        // A loop of openers ends where it began.
        for _ in 0..opener.len() {
            match opener.get(&at) {
                Some(up) if up != key => at = up.clone(),
                _ => break,
            }
        }
        at
    };
    let mut families: BTreeMap<Key, Vec<Card>> = BTreeMap::new();
    let mut roots = Vec::new();
    for card in cards {
        let key = card.key();
        let root = root_of(&key);
        if root == key || !opener.contains_key(&key) {
            roots.push(card);
        } else {
            families.entry(root).or_default().push(card);
        }
    }
    roots
        .into_iter()
        .map(|card| {
            let members = families.remove(&card.key()).unwrap_or_default();
            (card, members)
        })
        .collect()
}

/// Where the card by `key` is: its lane's place among `lanes`, and its row.
fn locate(lanes: &[LaneCards], key: &Key) -> Option<(usize, usize)> {
    lanes.iter().enumerate().find_map(|(lane, l)| {
        l.cards
            .iter()
            .position(|c| &c.key() == key)
            .map(|row| (lane, row))
    })
}

/// The card nearest `at`: in its lane at its row or the last before it, or
/// else in the lane nearest to it that holds any.
fn near(lanes: &[LaneCards], at: (usize, usize)) -> Option<(Key, (usize, usize))> {
    let lane = (0..lanes.len())
        .filter(|l| !lanes[*l].cards.is_empty())
        .min_by_key(|l| l.abs_diff(at.0))?;
    let row = at.1.min(lanes[lane].cards.len() - 1);
    Some((lanes[lane].cards[row].key(), (lane, row)))
}

/// `set` without `value`, or with it where it was not in.
pub(crate) fn toggle<T: Ord>(set: &mut BTreeSet<T>, value: T) {
    if !set.remove(&value) {
        set.insert(value);
    }
}

impl BoardView {
    /// Put the cursor on the card of the session, where it is listed.
    pub(crate) fn select(&mut self, environment: EnvironmentId, id: String) {
        self.selected = Some((environment, id));
    }
}

/// `values` with the one after `current` chosen alone, going round to none.
fn cycle<T: Ord + Clone>(values: &[T], current: &BTreeSet<T>) -> BTreeSet<T> {
    let at = match current.iter().next() {
        Some(first) if current.len() == 1 => values.iter().position(|v| v == first),
        _ => return values.first().cloned().into_iter().collect(),
    };
    at.and_then(|at| values.get(at + 1))
        .cloned()
        .into_iter()
        .collect()
}

/// Do what a board action asks; none for an action that is not one.
pub(crate) fn act(state: &mut ViewState, action: &Action) -> Option<Vec<Effect>> {
    if !matches!(
        action,
        Action::BoardOpen
            | Action::BoardLeft
            | Action::BoardRight
            | Action::BoardDown
            | Action::BoardUp
            | Action::BoardFirst
            | Action::BoardLast
            | Action::BoardEnter
            | Action::BoardIdle
            | Action::BoardGroup
            | Action::BoardExpand
            | Action::BoardProject
            | Action::BoardState
            | Action::BoardClear
            | Action::BoardSearch
    ) {
        return None;
    }
    let mut effects = Vec::new();
    match action {
        Action::BoardOpen => {
            let environment = state.hosts.environment.clone();
            state.content.windows.open(Tab::Board(environment));
            state.content.focus = crate::content::Area::Console;
        }
        Action::BoardIdle => state.board.show_idle = !state.board.show_idle,
        Action::BoardGroup => state.board.group = !state.board.group,
        Action::BoardExpand => {
            if let Some(key) = state.board.selected.clone()
                && !state.board.expanded.remove(&key)
            {
                state.board.expanded.insert(key);
            }
        }
        Action::BoardProject => {
            let names: Vec<_> = state
                .board
                .projects(&state.hosts, &state.tree.completions)
                .into_iter()
                .map(|p| p.name)
                .collect();
            state.board.projects = cycle(&names, &state.board.projects);
        }
        Action::BoardState => {
            let lanes: Vec<_> = Lane::ALL
                .into_iter()
                .filter(|l| *l != Lane::Idle || state.board.show_idle)
                .collect();
            state.board.states = cycle(&lanes, &state.board.states);
        }
        Action::BoardClear => {
            state.board.query.clear();
            state.board.projects.clear();
            state.board.states.clear();
        }
        Action::BoardSearch => effects.push(Effect::BoardSearch(true)),
        Action::BoardEnter => {
            if let Some((environment, id)) = state.board.selected.clone() {
                return Some(crate::content::open_row(
                    state,
                    tree::nav::Item::Session(environment, id),
                    Some(Opening::Existing),
                    true,
                ));
            }
        }
        _ => {
            let lanes = state.board.lanes(&state.hosts, &state.tree.completions);
            let at = state
                .board
                .selected
                .as_ref()
                .and_then(|key| locate(&lanes, key))
                .unwrap_or(state.board.fallback);
            let to = match action {
                Action::BoardDown | Action::BoardUp | Action::BoardFirst | Action::BoardLast => {
                    lanes.get(at.0).filter(|l| !l.cards.is_empty()).map(|l| {
                        let last = l.cards.len() - 1;
                        let row = match action {
                            Action::BoardDown => (at.1 + 1).min(last),
                            Action::BoardUp => at.1.saturating_sub(1),
                            Action::BoardLast => last,
                            _ => 0,
                        };
                        (at.0, row)
                    })
                }
                // The next lane that holds a card, at the row the cursor
                // is on or the last it has.
                _ => {
                    let towards: Vec<usize> = if *action == Action::BoardRight {
                        (at.0 + 1..lanes.len()).collect()
                    } else {
                        (0..at.0).rev().collect()
                    };
                    towards
                        .into_iter()
                        .find(|l| !lanes[*l].cards.is_empty())
                        .map(|l| (l, at.1.min(lanes[l].cards.len() - 1)))
                }
            };
            if let Some((lane, row)) = to
                && let Some(card) = lanes.get(lane).and_then(|l| l.cards.get(row))
            {
                state.board.selected = Some(card.key());
                state.board.fallback = (lane, row);
            }
        }
    }
    let (hosts, completions) = (&state.hosts, &state.tree.completions);
    state.board.sync(hosts, completions);
    Some(effects)
}

#[cfg(test)]
mod tests;
