//! The floating terminal: a panel of shells over the window. It only draws
//! and passes on what the mouse does; the view state holds whether it is
//! open and has the keys, and the work host holds its shells.

use dioxus::prelude::*;
use tsumugi_core::actions::Action;
use tsumugi_core::content::Tab;
use tsumugi_core::msg::Msg;

use super::ui::Ui;
use crate::components::{EmptyState, IconButton};
use crate::console::ConsoleScreen;
use crate::icons::Icon;
use crate::panes::{ensure_pane, panel_size};

/// The panel when it is open, or nothing.
#[component]
pub(crate) fn FloatingTerminal() -> Element {
    let ui = use_context::<Ui>();
    if !ui.app.floating.read().open {
        return rsx! {};
    }
    rsx! {
        Panel {}
    }
}

#[component]
fn Panel() -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let local = ui.local;
    let focused = app.floating.read().focused;
    // Read so the tabs follow the listing, the environment shown, and the
    // tab chosen.
    let _ = (app.hosts.read(), app.content.read(), app.floating.read());
    let (tabs, shown) = app.floating_tabs();
    let mut body = use_signal(|| (0.0, 0.0));
    let background = ui.hub.palette.read().background.hex();

    // The shown shell is attached at the size of the panel's body, and sized
    // again as the body is.
    use_effect({
        let hub = ui.hub.clone();
        move || {
            let (_, shown) = app.floating_tabs();
            let _ = (app.hosts.read(), app.floating.read());
            let mut attached = local.attached;
            let (width, height) = body();
            let Some(tab) = shown.filter(|_| width > 0.0 && height > 0.0) else {
                return;
            };
            let size = panel_size(width, height, (local.cell_width)(), (local.cell_height)());
            let me = tab.clone();
            if let Some(pane) = ensure_pane(app, &hub, &tab, size, move || {
                *attached.write() += 1;
                app.dispatch(Msg::InputMayHold(me));
            }) {
                pane.resize(size);
            }
        }
    });

    let _ = (local.attached)();
    let cell_width = (local.cell_width)();
    rsx! {
        div {
            class: if focused { "floating floating-focused" } else { "floating" },
            onmousedown: move |_| app.dispatch(Msg::FloatingFocus),
            div { class: "floating-bar",
                for (n , tab) in tabs.iter().enumerate() {
                    if let Tab::Terminal(environment, id) = tab {
                        {
                            let (environment, id) = (environment.clone(), *id);
                            let selected = shown.as_ref() == Some(tab);
                            rsx! {
                                button {
                                    key: "{id}",
                                    r#type: "button",
                                    class: if selected { "floating-tab selected" } else { "floating-tab" },
                                    onmousedown: move |e: MouseEvent| e.stop_propagation(),
                                    onclick: move |_| {
                                        app.dispatch(Msg::FloatingSelect {
                                            environment: environment.clone(),
                                            id,
                                        })
                                    },
                                    "{n + 1}"
                                }
                            }
                        }
                    }
                }
                IconButton {
                    icon: Icon::Plus,
                    title: "New shell",
                    place: "floating-new",
                    onclick: move |_| app.dispatch(Msg::Window(Action::FloatingNew)),
                }
                span { class: "floating-spacer" }
                IconButton {
                    icon: Icon::Terminal,
                    title: "End this shell",
                    place: "floating-end",
                    onclick: move |_| app.dispatch(Msg::Window(Action::FloatingCloseTab)),
                }
                IconButton {
                    icon: Icon::X,
                    title: "Close, leaving the shells running",
                    place: "floating-close",
                    onclick: move |_| app.dispatch(Msg::FloatingClose),
                }
            }
            div {
                class: "floating-body console",
                style: "--console-bg:{background};background:{background}",
                onresize: move |e: Event<ResizeData>| {
                    e.stop_propagation();
                    if let Ok(sz) = e.data().get_content_box_size() {
                        let size = (sz.width, sz.height);
                        if *body.peek() != size {
                            body.set(size);
                        }
                    }
                },
                match shown {
                    Some(tab) if ui.hub.panes.borrow().contains_key(&tab) => rsx! {
                        ConsoleScreen {
                            tab,
                            frame: local.frame,
                            cell_width,
                            focused,
                        }
                    },
                    _ => rsx! {
                        EmptyState { "Starting a shell…" }
                    },
                }
            }
        }
    }
}
