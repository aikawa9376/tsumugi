//! Editing a session's name, or an environment's, where it is shown.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_model::EnvironmentId;

pub use tsumugi_core::overlay::rename::*;

use crate::bridge::AppCtx;
use crate::components::InlineField;

#[component]
pub fn SessionName(
    environment: EnvironmentId,
    id: String,
    name: String,
    place: Place,
    #[props(default)] sidebar_panel: tsumugi_core::sidebar::Panel,
) -> Element {
    let app = use_context::<AppCtx>();
    let active_panel = use_memo(move || app.sidebar.read().panel);
    let rename = app.overlays.read().rename.clone().filter(|r| {
        r.is_for(&environment, &id, place)
            && (place == Place::Tab || active_panel() == sidebar_panel)
    });
    let finish = {
        let (environment, id) = (environment.clone(), id.clone());
        move |commit: bool| {
            app.dispatch(Msg::RenameEnd {
                environment: environment.clone(),
                id: id.clone(),
                place,
                commit,
            })
        }
    };
    rsx! {
        if let Some(rename) = rename {
            InlineField {
                class: "session-rename-input",
                value: rename.text,
                aria_label: if place == Place::Heading { "Environment name" } else { "Session name" },
                focus: true,
                contained: true,
                onmounted: move |_| {
                    document::eval("document.querySelector('.session-rename-input')?.select()");
                },
                oninput: {
                    let (environment, id) = (environment.clone(), id.clone());
                    move |text: String| {
                        app.dispatch(Msg::RenameText {
                            environment: environment.clone(),
                            id: id.clone(),
                            place,
                            text,
                        })
                    }
                },
                onenter: {
                    let finish = finish.clone();
                    move |()| finish(true)
                },
                onescape: {
                    let finish = finish.clone();
                    move |()| finish(false)
                },
                onblur: move |()| finish(false),
            }
        } else {
            span {
                class: "session-name",
                ondoubleclick: move |e: Event<MouseData>| {
                    e.stop_propagation();
                    if place != Place::Tab { app.dispatch(Msg::SidebarPanel(sidebar_panel)); }
                    app.dispatch(Msg::RenameStart {
                        environment: environment.clone(),
                        id: id.clone(),
                        place,
                    });
                },
                "{name}"
            }
        }
    }
}
