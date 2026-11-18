//! The header: the panels' switches, where the content's focused window is,
//! in which environment, and how many sessions wait or work.

use dioxus::prelude::*;
use tsumugi_core::tree;

use crate::bridge::AppCtx;
use crate::components::{Spacer, StateGlyph};
use crate::icons::{Icon, IconView};
use crate::layout::Side;

/// A click on a panel's switch goes to `ontoggle`, which also moves the keys
/// out of a panel it hides.
#[component]
pub(crate) fn Header(ontoggle: EventHandler<Side>) -> Element {
    let app = use_context::<AppCtx>();
    let layout = app.chrome.read().panels.layout;
    let (environment, name, crumbs, waiting, working) = {
        let view = app.tree.read();
        let hosts = app.hosts.read();
        let showing = app.content.read().showing();
        let environment = hosts.environment_of(showing.tab.as_ref()).clone();
        let work = hosts.work(&environment);
        (
            environment.clone(),
            hosts.name(&environment),
            tree::crumbs(&showing, work),
            view.waiting().len(),
            hosts
                .each()
                .map(|(_, host)| tree::working(host.work.as_ref()))
                .sum::<usize>(),
        )
    };
    let kind = crate::connection::standing_of(&app, &environment).kind;
    rsx! {
        div { class: if cfg!(target_os = "linux") { "header desktop-header" } else { "header" },
            if cfg!(target_os = "linux") {
                button {
                    class: "desktop-brand",
                    title: "Application menu",
                    aria_label: "Application menu",
                    onclick: move |_| crate::menu_bar::popup(),
                    span { class: "desktop-brand-mark", dangerous_inner_html: include_str!("../../../../assets/logo/tsumugi-icon.svg") }
                    span { class: "desktop-brand-name", "Tsumugi" }
                    IconView { icon: Icon::ChevronDown, size: 11 }
                }
                span { class: "desktop-divider" }
            }
            span {
                class: if layout.show_sidebar { "pane-toggle on" } else { "pane-toggle" },
                title: "Show or hide the tree",
                onclick: move |_| ontoggle.call(Side::Sidebar),
                IconView { icon: Icon::LayoutSidebar, size: 16 }
            }
            div { class: "header-crumbs",
                onmousedown: move |event| {
                    if cfg!(target_os = "linux") && event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                        dioxus::desktop::window().drag();
                    }
                },
                ondoubleclick: move |_| {
                    if cfg!(target_os = "linux") { dioxus::desktop::window().toggle_maximized(); }
                },
                span { class: "header-crumbs-item header-crumbs-environment", IconView { icon: kind.icon(), size: 14 } "{name}" }
                if let Some(c) = &crumbs {
                    span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                    if let Some((name, scratch)) = &c.workspace {
                        span { class: "header-crumbs-item", IconView { icon: if *scratch { Icon::Notes } else { Icon::Folder }, size: 14 } "{name}" }
                        span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                    }
                    span { class: "header-crumbs-item header-crumbs-path", IconView { icon: Icon::GitBranch, size: 14 } "{tsumugi_core::picker::short(&c.worktree, &crate::paths::home())}" }
                    span { class: "header-crumbs-sep", IconView { icon: Icon::ChevronRight, size: 12 } }
                    span { "{c.target}" }
                }
            }
            if cfg!(target_os = "linux") {
                div {
                    class: "desktop-drag-space",
                    onmousedown: move |event| {
                        if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                            dioxus::desktop::window().drag();
                        }
                    },
                    ondoubleclick: move |_| dioxus::desktop::window().toggle_maximized(),
                }
            } else {
                Spacer {}
            }
            if waiting > 0 {
                span { class: "header-chip wait", StateGlyph { class: "state-glyph wait", icon: Icon::Bell, size: 12 } "{waiting} waiting" }
            }
            if working > 0 {
                span { class: "header-chip", StateGlyph { class: "state-glyph run", icon: Icon::Loader, size: 12 } "{working} working" }
            }
            span {
                class: if layout.show_details { "pane-toggle on" } else { "pane-toggle" },
                title: "Show or hide the details panel",
                onclick: move |_| ontoggle.call(Side::Details),
                IconView { icon: Icon::LayoutSidebarRight, size: 16 }
            }
            span {
                class: "pane-toggle",
                title: "Settings",
                onclick: move |_| app.open_settings(&app.active_environment()),
                IconView { icon: Icon::Settings, size: 16 }
            }
            if cfg!(target_os = "linux") {
                span { class: "desktop-divider" }
                div { class: "desktop-window-controls",
                    button { class: "desktop-window-button", title: "Minimize", aria_label: "Minimize window",
                        onclick: move |_| dioxus::desktop::window().set_minimized(true),
                        span { class: "desktop-minimize-mark" }
                    }
                    button { class: "desktop-window-button", title: "Maximize or restore", aria_label: "Maximize or restore window",
                        onclick: move |_| dioxus::desktop::window().toggle_maximized(),
                        IconView { icon: Icon::Square, size: 12 }
                    }
                    button { class: "desktop-window-button desktop-window-close", title: "Close window", aria_label: "Close window",
                        onclick: move |_| dioxus::desktop::window().close(),
                        IconView { icon: Icon::X, size: 14 }
                    }
                }
            }
        }
    }
}

/// Frameless floating windows keep their edge and corner resize gestures.
#[component]
pub(crate) fn ResizeEdges() -> Element {
    use dioxus::desktop::tao::window::ResizeDirection;
    let edges = [
        ("desktop-resize desktop-resize-n", ResizeDirection::North),
        ("desktop-resize desktop-resize-s", ResizeDirection::South),
        ("desktop-resize desktop-resize-w", ResizeDirection::West),
        ("desktop-resize desktop-resize-e", ResizeDirection::East),
        (
            "desktop-resize desktop-resize-nw",
            ResizeDirection::NorthWest,
        ),
        (
            "desktop-resize desktop-resize-ne",
            ResizeDirection::NorthEast,
        ),
        (
            "desktop-resize desktop-resize-sw",
            ResizeDirection::SouthWest,
        ),
        (
            "desktop-resize desktop-resize-se",
            ResizeDirection::SouthEast,
        ),
    ];
    rsx! {
        for (class, direction) in edges {
            div {
                key: "{class}",
                class,
                onmousedown: move |event| {
                    if event.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                        let _ = dioxus::desktop::window().drag_resize_window(direction);
                    }
                },
            }
        }
    }
}
