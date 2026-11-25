//! Capturing the screen for a project's resources: the screens are taken
//! after the delay asked for, then a part of one of them is chosen on a still
//! of it and kept.

use std::time::Duration;

use crate::actions::Action;
use crate::cursor;
use crate::effect::Effect;
use crate::state::ViewState;
use crate::tree::Place;

/// A drag shorter than this, in pixels of the screen, is a click: it marks
/// no part.
const LEAST_DRAG: u32 = 2;

/// A capture under way, for the project of the worktree it was asked from.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub place: Place,
    pub stage: Stage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// The screens are being taken, once the delay asked for has gone by.
    Taking,
    /// A still of each screen taken, by its size in pixels, the one `shown`
    /// chosen from, and the part of it marked: left, top, width and height.
    Choosing {
        screens: Vec<[u32; 2]>,
        shown: usize,
        part: Option<[u32; 4]>,
    },
}

impl Capture {
    /// Whether the stills are shown, holding the keys.
    pub fn choosing(&self) -> bool {
        matches!(self.stage, Stage::Choosing { .. })
    }
}

/// Take the screens after `delay`, for the project of the worktree the
/// details panel shows.
pub(crate) fn start(state: &mut ViewState, delay: Duration) -> Vec<Effect> {
    if state.overlays.capture.is_some() {
        return vec![Effect::Notice(
            "A capture of the screen is being taken already".into(),
        )];
    }
    let shown = &state.details.shown;
    let Some(worktree) = shown.worktree.clone() else {
        return vec![Effect::Notice(
            "Show a worktree to keep a capture in its project".into(),
        )];
    };
    state.overlays.menu = None;
    state.overlays.capture = Some(Capture {
        place: Place::at(&shown.environment, worktree),
        stage: Stage::Taking,
    });
    vec![Effect::CaptureScreen { delay }]
}

/// The screens asked for were taken, each of this size.
pub(crate) fn taken(state: &mut ViewState, screens: Vec<[u32; 2]>) -> Vec<Effect> {
    match state.overlays.capture.as_mut() {
        Some(capture) if capture.stage == Stage::Taking && !screens.is_empty() => {
            capture.stage = Stage::Choosing {
                screens,
                shown: 0,
                part: None,
            };
            Vec::new()
        }
        Some(capture) if capture.stage == Stage::Taking => {
            state.overlays.capture = None;
            vec![
                Effect::DropCapture,
                Effect::Notice("No screen was captured".into()),
            ]
        }
        _ => vec![Effect::DropCapture],
    }
}

/// The screens could not be taken, for the reason `text` gives.
pub(crate) fn failed(state: &mut ViewState, text: String) -> Vec<Effect> {
    state.overlays.capture = None;
    vec![Effect::Notice(text)]
}

/// A drag over the still shown from `from` to `to`, in pixels of its screen;
/// `done` once it is let go, which keeps the part it marks.
pub(crate) fn drag(state: &mut ViewState, from: [f64; 2], to: [f64; 2], done: bool) -> Vec<Effect> {
    let Some(Capture {
        stage: Stage::Choosing {
            screens,
            shown,
            part,
        },
        ..
    }) = state.overlays.capture.as_mut()
    else {
        return Vec::new();
    };
    *part = marked(screens[*shown], from, to);
    if done && part.is_some() {
        keep(state)
    } else {
        Vec::new()
    }
}

/// The part between the corners `from` and `to`, inside a screen of `size`,
/// none for a click.
fn marked(size: [u32; 2], from: [f64; 2], to: [f64; 2]) -> Option<[u32; 4]> {
    let clamp = |v: f64, most: u32| v.round().clamp(0.0, f64::from(most)) as u32;
    let (x0, x1) = (clamp(from[0], size[0]), clamp(to[0], size[0]));
    let (y0, y1) = (clamp(from[1], size[1]), clamp(to[1], size[1]));
    let (left, top) = (x0.min(x1), y0.min(y1));
    let (width, height) = (x0.abs_diff(x1), y0.abs_diff(y1));
    (width >= LEAST_DRAG && height >= LEAST_DRAG).then_some([left, top, width, height])
}

/// A key's action while the stills are shown.
pub(crate) fn act(state: &mut ViewState, action: Option<Action>) -> Vec<Effect> {
    match action {
        Some(Action::OverlayChoose) => keep(state),
        Some(Action::OverlayClose) => {
            state.overlays.capture = None;
            vec![Effect::DropCapture]
        }
        Some(Action::OverlayDown) => next(state, 1),
        Some(Action::OverlayUp) => next(state, -1),
        _ => Vec::new(),
    }
}

fn next(state: &mut ViewState, delta: isize) -> Vec<Effect> {
    if let Some(Capture {
        stage: Stage::Choosing {
            screens,
            shown,
            part,
        },
        ..
    }) = state.overlays.capture.as_mut()
    {
        *shown = cursor::wrap(*shown, delta, screens.len());
        *part = None;
    }
    Vec::new()
}

/// Keep the part marked of the screen shown, or the whole of it.
fn keep(state: &mut ViewState) -> Vec<Effect> {
    let Some(Capture {
        place,
        stage: Stage::Choosing {
            screens,
            shown,
            part,
        },
    }) = state.overlays.capture.take()
    else {
        return Vec::new();
    };
    let [width, height] = screens[shown];
    vec![Effect::KeepCapture {
        environment: place.environment,
        dir: place.worktree,
        screen: shown,
        part: part.unwrap_or([0, 0, width, height]),
    }]
}

#[cfg(test)]
mod tests;
