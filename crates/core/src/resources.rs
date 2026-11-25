//! Keeping the resources of the project the details panel shows: the sheet
//! that takes one, and what the panel's rows do to the ones it lists.

use crate::details::{self, Entry, Source};
use crate::effect::Effect;
use crate::overlay::ResourceSheet;
use crate::state::ViewState;
use crate::tree::Place;

/// Open the sheet that takes a resource for the project of what the panel
/// shows.
pub(crate) fn add(state: &mut ViewState) -> Vec<Effect> {
    open(state, None)
}

/// Open the sheet on the pinned resource the panel's cursor is on.
pub(crate) fn edit(state: &mut ViewState) -> Vec<Effect> {
    match chosen(state) {
        Some(Entry::Pinned(resource)) => open(state, Some(resource)),
        _ => Vec::new(),
    }
}

/// Keep the pinned resource the panel's cursor is on no more.
pub(crate) fn remove(state: &mut ViewState) -> Vec<Effect> {
    match chosen(state) {
        Some(Entry::Pinned(resource)) => vec![Effect::DropResource {
            environment: state.details.shown.environment.clone(),
            id: resource.id,
        }],
        _ => Vec::new(),
    }
}

/// Keep what the panel's cursor is on, an address or a file or an image
/// file, in the project, unless it keeps it already.
pub(crate) fn keep(state: &mut ViewState) -> Vec<Effect> {
    let Some(worktree) = state.details.shown.worktree.clone() else {
        return Vec::new();
    };
    let (target, label) = match chosen(state) {
        Some(Entry::Link(link)) => (link.target, link.title),
        Some(Entry::Image(shot)) => match shot.source {
            Source::File { path, .. } => (path, None),
            Source::Console { .. } => {
                return vec![Effect::Notice("A drawn image has no file to keep".into())];
            }
        },
        _ => return Vec::new(),
    };
    let kept = kept_in(state).any(|resource| resource.target == target);
    if kept {
        return vec![Effect::Notice(format!(
            "{target} is among the project's resources already"
        ))];
    }
    vec![Effect::KeepResource {
        environment: state.details.shown.environment.clone(),
        dir: worktree,
        target,
        label,
    }]
}

fn chosen(state: &ViewState) -> Option<Entry> {
    let work = state.hosts.work(&state.details.shown.environment);
    details::chosen(&state.details, work)
}

/// What the project of the worktree shown keeps.
fn kept_in(state: &ViewState) -> impl Iterator<Item = &tsumugi_model::Resource> {
    let shown = &state.details.shown;
    let work = state.hosts.work(&shown.environment);
    let project = shown
        .worktree
        .as_deref()
        .zip(work)
        .and_then(|(worktree, work)| work.tree.tree().workspace_of(worktree))
        .map(|workspace| workspace.root.clone());
    work.into_iter()
        .flat_map(|work| work.tree.resources.iter())
        .filter(move |resource| Some(&resource.project) == project.as_ref())
}

fn open(state: &mut ViewState, editing: Option<tsumugi_model::Resource>) -> Vec<Effect> {
    let shown = &state.details.shown;
    let Some(worktree) = shown.worktree.clone() else {
        return vec![Effect::Notice(
            "Show a worktree to keep a resource in its project".into(),
        )];
    };
    state.overlays.menu = None;
    state.overlays.resources = Some(ResourceSheet {
        place: Place::at(&shown.environment, worktree),
        editing,
    });
    Vec::new()
}

#[cfg(test)]
mod tests;
