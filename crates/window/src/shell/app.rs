//! The window's root component: it opens the view state and the window's
//! own handles, starts what carries out the view state's effects and takes
//! the keys, and draws the frame around the tree, the content and the
//! details.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;

use super::body::Body;
use super::effects::use_effects;
use super::footer::Footer;
use super::header::Header;
use super::local::use_local;
use super::overlays::{ConfirmDialog, ContextMenu, DaemonBanner, InterruptedBanner, KeysHelp};
use super::socket::use_window_socket;
use super::ui::{Ui, use_ui};
use crate::bridge::{AppCtx, use_app};
use crate::bundled_font::{faces, use_bundled_font};
use crate::components::SettingsFileError;
use crate::hosts::use_hosts;
use crate::input::{KeysField, use_input};
use crate::panes::{use_console_sizes, use_hub, use_redraws_told, use_shown_images};
use crate::picker::Picker;
use crate::style::CSS;
use crate::{menu_bar, settings_window, themes};

/// Open the window.
pub fn launch(build: crate::Build) {
    crate::build::settle(build);
    #[cfg(target_os = "linux")]
    crate::native_terminal::choose_backend();
    use dioxus::desktop::{Config as Desktop, LogicalSize, WindowBuilder};
    let cfg = Desktop::new()
        // Attach the menu before replacing the Linux decorations, so its
        // actions remain registered for the header's application popup.
        .with_menu(menu_bar::build())
        .with_window(
            WindowBuilder::new()
                .with_decorations(!cfg!(target_os = "linux"))
                .with_title(tsumugi_core::chrome::title(
                    crate::build::described().filter(|_| cfg!(debug_assertions)),
                    0,
                ))
                .with_inner_size(LogicalSize::new(1440.0, 900.0)),
        )
        .with_background_color((15, 17, 20, 255))
        .with_custom_head(format!("<style>{}{CSS}</style>", faces()));
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    use_hook(menu_bar::hide_native_bar);
    use_bundled_font();
    crate::viewers::use_arto_assets();
    let app = use_app();
    let user_zoom = use_memo(move || app.chrome.read().zoom);
    let auto_scale = use_memo(move || app.config.read().settings.auto_scale);
    crate::scaling::use_scaling(user_zoom, auto_scale);
    let local = use_local();
    let hub = use_hub(app, local);
    let ui = use_ui(app, hub, local);
    use_hosts(&ui);
    use_window_socket(&ui);
    use_shown_images(&ui);
    #[cfg(target_os = "linux")]
    crate::native_terminal::use_native_layer(&ui);
    use_console_sizes(&ui);
    use_redraws_told(app, local);
    use_scroll_into_view(app);
    use_window_title(app);
    use_effects(&ui);
    use_input(&ui);
    crate::browser::use_browsers(app);
    use_drop(settings_window::close);
    rsx! {
        RootStyle {}
        div { class: "app",
            if cfg!(target_os = "linux") { super::header::ResizeEdges {} }
            KeysField {}
            SettingsFileError { config: app.config }
            Header { ontoggle: move |side| app.dispatch(Msg::PanelToggle(side)) }
            Body {}
            DaemonBanner {}
            InterruptedBanner {}
            Footer {}
            Picker {}
            ContextMenu {}
            KeysHelp {}
            crate::adding::AddingSheet {}
            crate::local_issues::LocalIssues {}
            super::composer::ComposerDialog {}
            super::cleanup::CleanupDialog {}
            crate::resources::ResourceSheet {}
            ConfirmDialog {}
            super::floating::FloatingTerminal {}
            super::lightbox::Lightbox {}
            super::capture::Capture {}
            super::switcher::TabSwitcher {}
        }
    }
}

/// The window's title counts what is unread, so the taskbar or the tab bar
/// of the desktop shows it while the window is behind others.
fn use_window_title(app: AppCtx) {
    let unread = use_memo(move || app.tree.read().completions.unread_count());
    use_effect(move || {
        let title = tsumugi_core::chrome::title(
            crate::build::described().filter(|_| cfg!(debug_assertions)),
            unread(),
        );
        dioxus::desktop::window().set_title(&title);
    });
}

/// The theme's colours and the console's font, over the stylesheet's own.
#[component]
fn RootStyle() -> Element {
    let ui = use_context::<Ui>();
    let config = ui.app.config.read();
    let s = &config.settings;
    let style = format!(
        ":root{{{theme}--term-font:{family};--term-size:{size}px;--term-weight:{weight};--cell-h:{h}px}}",
        theme = themes::css(&themes::window(&ui.hub.palette.read())),
        family = s.console_font(),
        size = s.font_size,
        weight = s.font_weight,
        h = (ui.local.cell_height)(),
    );
    rsx! {
        style { "{style}" }
    }
}

/// Keep what is selected in the sidebar in view, and the tab the console
/// shows along the one line of tabs, after each is drawn.
fn use_scroll_into_view(app: AppCtx) {
    // The rows too: a row gone into is selected once its directory is read,
    // and rows unfolded above it push it down.
    let selected = use_memo(move || {
        let tree = app.tree.read();
        (tree.selected.clone(), tree.rows().to_vec())
    });
    use_effect(move || {
        let _ = selected();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.sidebar .list-row.selected')
               .forEach((r) => r.scrollIntoView({ block: 'nearest' })));",
        );
    });
    let active = use_memo(move || app.content.read().active().cloned());
    use_effect(move || {
        let _ = active();
        document::eval(
            "requestAnimationFrame(() => document.querySelectorAll('.tabs .tab.active')
               .forEach((t) => t.scrollIntoView({ block: 'nearest', inline: 'nearest' })));",
        );
    });
}
