//! What a browser page is asked beyond its address field: to search its
//! text, and to come back after it was closed.

use tsumugi_model::EnvironmentId;

use crate::content::{Area, Opening, Tab};
use crate::effect::Effect;
use crate::settings::BrowserSettings;
use crate::state::ViewState;

#[cfg(test)]
mod tests;

/// Whether a browser page is what the keys are in.
pub(crate) fn has_the_keys(state: &ViewState) -> bool {
    state.content.focus == Area::Console
        && matches!(state.content.active(), Some(Tab::Browser { .. }))
}

/// Ask the browser page shown to open its search bar.
pub(crate) fn find(state: &mut ViewState) -> Vec<Effect> {
    match (
        state.content.active().cloned(),
        state.content.windows.focused_window(),
    ) {
        (Some(Tab::Browser { .. }), Some(window)) => vec![Effect::FindInPage(window)],
        _ => vec![Effect::Notice("Show a browser page to search it".into())],
    }
}

/// Ask for the page closed last in the worktree the keys are in, as a new
/// page is opened in it.
pub(crate) fn reopen(state: &mut ViewState) -> Vec<Effect> {
    let environment = crate::content::shown_environment(state);
    let worktree = crate::content::browser_worktree(state, &environment, false);
    vec![Effect::ReopenPage {
        environment,
        worktree,
    }]
}

/// Where a link opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Destination {
    /// A page of Tsumugi's browser.
    App,
    /// The system's browser.
    System,
}

/// Where a link opens, given `modifier`, whether it was pressed with the
/// keys that choose the other way for one link. Without `modifier_inverts`
/// those keys only ever send a link to the system's browser.
pub(crate) fn destination(settings: &BrowserSettings, modifier: bool) -> Destination {
    let app = match (modifier, settings.modifier_inverts) {
        (false, _) => settings.open_links_in_app,
        (true, false) => false,
        (true, true) => !settings.open_links_in_app,
    };
    if app {
        Destination::App
    } else {
        Destination::System
    }
}

/// Open the web link `url` pressed in a window of `environment`, in a page
/// of the worktree the keys are in, or in the system's browser.
pub(crate) fn open_link(
    state: &mut ViewState,
    environment: EnvironmentId,
    url: String,
    modifier: bool,
) -> Vec<Effect> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Vec::new();
    }
    match destination(&state.config.settings.browser, modifier) {
        Destination::System => vec![Effect::Open {
            environment,
            target: url,
        }],
        Destination::App => {
            let worktree = crate::content::browser_worktree(state, &environment, false);
            vec![Effect::OpenAddress {
                environment,
                worktree,
                url,
                how: Opening::Existing,
            }]
        }
    }
}
