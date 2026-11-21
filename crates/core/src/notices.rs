//! What needed the user, as this window reads it: which of the notices each
//! work host keeps it has read, which of them it tells the user of outside
//! the window, and the list the user goes through.
//!
//! The host keeps the notices themselves; this window keeps only what it has
//! read of them, so another window, or the same one after it opens again,
//! need not agree.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use tsumugi_model::{EnvironmentId, Notice, NoticeKind, Standing};

use crate::effect::Effect;
use crate::host::Hosts;
use crate::picker::ListState;
use crate::state::ViewState;

pub mod mute;
pub use mute::Mutes;

/// The notices this window has read, by the environment keeping them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads(BTreeMap<EnvironmentId, BTreeSet<u64>>);

/// How [`Reads`] are written: environments by their ids.
#[derive(Serialize, Deserialize)]
struct Written(BTreeMap<String, BTreeSet<u64>>);

impl Reads {
    pub fn is_read(&self, environment: &EnvironmentId, id: u64) -> bool {
        self.0.get(environment).is_some_and(|ids| ids.contains(&id))
    }

    fn set(&mut self, environment: &EnvironmentId, id: u64, read: bool) -> bool {
        if read {
            self.0.entry(environment.clone()).or_default().insert(id)
        } else {
            self.0
                .get_mut(environment)
                .is_some_and(|ids| ids.remove(&id))
        }
    }

    /// Forget the notices their host no longer keeps, so what is kept stays
    /// as small as what the hosts keep. An environment whose host has not
    /// said yet keeps what it had.
    fn pruned(&mut self, hosts: &Hosts) -> bool {
        let mut changed = false;
        for (environment, ids) in &mut self.0 {
            let Some(work) = hosts.work(environment) else {
                continue;
            };
            let kept: BTreeSet<u64> = work.tree.notices.iter().map(|n| n.id).collect();
            let before = ids.len();
            ids.retain(|id| kept.contains(id));
            changed |= ids.len() != before;
        }
        self.0.retain(|_, ids| !ids.is_empty());
        changed
    }

    pub fn to_json(&self) -> String {
        let written = Written(
            self.0
                .iter()
                .map(|(e, i)| (e.to_string(), i.clone()))
                .collect(),
        );
        serde_json::to_string(&written).expect("read marks are plain data")
    }

    /// From what [`Reads::to_json`] wrote.
    pub fn from_json(text: &[u8]) -> Result<Self, serde_json::Error> {
        let written: Written = serde_json::from_slice(text)?;
        Ok(Self(
            written
                .0
                .into_iter()
                .map(|(e, ids)| (EnvironmentId::new(e), ids))
                .collect(),
        ))
    }
}

/// What the window holds of the notices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoticeView {
    pub reads: Reads,
    /// The workspaces and worktrees whose notifications are off.
    pub mutes: Mutes,
    /// The newest notice of each environment already told of outside the
    /// window, or passed over as old when its host was first heard from;
    /// none while its host has kept none.
    told: BTreeMap<EnvironmentId, Option<u64>>,
}

/// The list open: what is typed to narrow it, and where the cursor is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub list: ListState,
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub environment: EnvironmentId,
    pub notice: Notice,
    pub standing: Standing,
    pub read: bool,
}

/// The notices of every environment that match `query`, those whose session
/// still stands as they say first, then the unread, each newest first.
pub fn rows(view: &NoticeView, hosts: &Hosts, query: &str) -> Vec<Row> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut rows: Vec<Row> = Vec::new();
    for (environment, host) in hosts.each() {
        let Some(work) = &host.work else { continue };
        let standings = tsumugi_model::standings(&work.tree.notices, work.tree.tree());
        for (notice, standing) in work.tree.notices.iter().zip(standings) {
            let text = format!(
                "{} {} {} {}",
                notice.name,
                notice.workspace,
                notice.kind.word(),
                notice.reason
            )
            .to_lowercase();
            if !terms.iter().all(|t| text.contains(t.as_str())) {
                continue;
            }
            rows.push(Row {
                environment: environment.clone(),
                read: view.reads.is_read(environment, notice.id),
                notice: notice.clone(),
                standing,
            });
        }
    }
    rows.sort_by(|a, b| {
        let key = |r: &Row| {
            (
                r.standing != Standing::Current,
                r.read,
                std::cmp::Reverse((r.notice.at_ms, r.notice.id)),
            )
        };
        key(a).cmp(&key(b))
    });
    rows
}

/// How many notices are unread, and how many of them wait on the user now.
pub fn unread(view: &NoticeView, hosts: &Hosts) -> usize {
    rows(view, hosts, "").iter().filter(|r| !r.read).count()
}

/// Open the list.
pub(crate) fn open(state: &mut ViewState) -> Vec<Effect> {
    state.picker.open = Some(crate::picker::Picking::Notices(Listing::default()));
    Vec::new()
}

/// The row `at` chosen: it is read, and its session is shown when it still
/// runs.
pub(crate) fn choose(state: &mut ViewState, at: usize) -> Vec<Effect> {
    let Some(query) = query_of(state) else {
        return Vec::new();
    };
    let Some(row) = rows(&state.notices, &state.hosts, &query)
        .into_iter()
        .nth(at)
    else {
        return Vec::new();
    };
    let mut effects = mark(state, &row, true);
    if row.standing != Standing::Ended {
        crate::picker::close(&mut state.picker);
        effects.extend(crate::content::show_session(
            state,
            row.environment,
            row.notice.session,
        ));
    }
    effects
}

/// The row under the cursor read, or unread when it was read.
pub(crate) fn toggle(state: &mut ViewState) -> Vec<Effect> {
    let Some(query) = query_of(state) else {
        return Vec::new();
    };
    let at = state.picker.open.as_ref().map_or(0, |o| o.cursor());
    match rows(&state.notices, &state.hosts, &query)
        .into_iter()
        .nth(at)
    {
        Some(row) => mark(state, &row, !row.read),
        None => Vec::new(),
    }
}

/// Every row the list shows read.
pub(crate) fn read_all(state: &mut ViewState) -> Vec<Effect> {
    let Some(query) = query_of(state) else {
        return Vec::new();
    };
    let mut changed = false;
    for row in rows(&state.notices, &state.hosts, &query) {
        changed |= state
            .notices
            .reads
            .set(&row.environment, row.notice.id, true);
    }
    keep(changed, state)
}

fn query_of(state: &ViewState) -> Option<String> {
    match &state.picker.open {
        Some(crate::picker::Picking::Notices(l)) => Some(l.list.query.clone()),
        _ => None,
    }
}

fn mark(state: &mut ViewState, row: &Row, read: bool) -> Vec<Effect> {
    let changed = state
        .notices
        .reads
        .set(&row.environment, row.notice.id, read);
    keep(changed, state)
}

fn keep(changed: bool, state: &ViewState) -> Vec<Effect> {
    if changed {
        vec![Effect::KeepReads(state.notices.reads.clone())]
    } else {
        Vec::new()
    }
}

/// What the window read before it closed comes back, ahead of what it reads
/// since.
pub(crate) fn restore(state: &mut ViewState, reads: Reads) -> Vec<Effect> {
    for (environment, ids) in reads.0 {
        state
            .notices
            .reads
            .0
            .entry(environment)
            .or_default()
            .extend(ids);
    }
    Vec::new()
}

/// `environment`'s host said something: forget what it no longer keeps, and
/// tell the user of its new failures and completions, as the settings say.
///
/// What its host kept before this window first heard from it is not told of,
/// and nothing is told twice, whatever the window hears again.
pub(crate) fn heard(state: &mut ViewState, environment: &EnvironmentId) -> Vec<Effect> {
    let mut effects = Vec::new();
    if state.notices.reads.pruned(&state.hosts) {
        effects.push(Effect::KeepReads(state.notices.reads.clone()));
    }
    let Some(work) = state.hosts.work(environment) else {
        return effects;
    };
    let newest = work.tree.notices.iter().map(|n| n.id).max();
    let settings = &state.config.settings;
    let told_before = state.notices.told.get(environment).copied();
    let told_of: Vec<(String, String)> = match told_before {
        // The first word of a host passes over what it kept before.
        None => Vec::new(),
        Some(told) => work
            .tree
            .notices
            .iter()
            // A host whose file was lost counts again from the first.
            .filter(|n| told.is_none_or(|t| n.id > t || newest < Some(t)))
            .filter(|n| {
                !state
                    .notices
                    .mutes
                    .silences(environment, work.tree.tree(), &n.session)
            })
            .filter(|n| match n.kind {
                NoticeKind::Waiting => false,
                NoticeKind::Failed => settings.notify_failed,
                NoticeKind::Done => settings.notify_done,
            })
            .map(|n| {
                let why = match n.reason.as_str() {
                    "" => n.kind.word().to_owned(),
                    reason => format!("{}: {reason}", n.kind.word()),
                };
                (n.name.clone(), why)
            })
            .collect(),
    };
    if !told_of.is_empty() {
        effects.push(Effect::NotifyNotices(told_of));
    }
    state.notices.told.insert(environment.clone(), newest);
    effects
}

#[cfg(test)]
mod tests;
