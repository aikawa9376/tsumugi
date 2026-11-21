//! How the window shows the work: every slice of its state, each changed
//! only by `update`.

use crate::chrome::Chrome;
use crate::config::Config;
use crate::console::ConsoleView;
use crate::content::ContentView;
use crate::details::Details;
use crate::host::Hosts;
use crate::overlay::Overlays;
use crate::picker::PickerView;
use crate::prompt::PromptView;
use crate::tree::TreeView;

#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    pub chrome: Chrome,
    pub config: Config,
    pub hosts: Hosts,
    pub content: ContentView,
    pub tree: TreeView,
    pub sidebar: crate::sidebar::Sidebar,
    pub details: Details,
    pub picker: PickerView,
    pub notices: crate::notices::NoticeView,
    pub overlays: Overlays,
    pub floating: crate::floating::Floating,
    pub console: ConsoleView,
    pub prompt: PromptView,
    pub ai: crate::ai::AiView,
}

impl Default for ViewState {
    fn default() -> Self {
        let mut state = Self {
            chrome: Chrome::default(),
            config: Config::default(),
            hosts: Hosts::default(),
            content: ContentView::default(),
            tree: TreeView::default(),
            sidebar: crate::sidebar::Sidebar::default(),
            details: Details::default(),
            picker: PickerView::default(),
            notices: crate::notices::NoticeView::default(),
            overlays: Overlays::default(),
            floating: crate::floating::Floating::default(),
            console: ConsoleView::default(),
            prompt: PromptView::default(),
            ai: crate::ai::AiView::default(),
        };
        let settings = &state.config.settings;
        crate::details::follow_settings(
            &mut state.details,
            state.chrome.panels.layout.show_details,
            &settings.details,
            &settings.panel,
        );
        // A window that shows nothing asks nothing of the host, which starts
        // out reading nothing.
        let environment = state.hosts.environment.clone();
        state.hosts.own_mut().watched = crate::settle::sight(&state, &environment);
        state
    }
}
