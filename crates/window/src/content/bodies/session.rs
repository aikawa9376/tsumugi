//! What a window shows of a session it has no screen of yet: one still
//! starting, one the sessions' daemon took with it, or one whose program
//! exited.

use dioxus::prelude::*;
use tsumugi_core::content::{Tab, rest_of, waits_to_come_back};

use crate::bridge::AppCtx;
use crate::components::{Button, EmptyState};
use crate::icons::Icon;
use crate::shell::Ui;

/// What a window shows of the session or the shell `tab` names until its
/// screen comes.
#[component]
pub(crate) fn Starting(tab: Tab) -> Element {
    let app = use_context::<AppCtx>();
    let ui = use_context::<Ui>();
    if waits_to_come_back(&app.hosts.read(), &tab) {
        return rsx! {
            EmptyState { "The sessions' daemon took this with it when it went. Bring it back from the banner, or with Bring Back Sessions… in the Session menu." }
        };
    }
    if let (Some(rest), Tab::Session(environment, id)) = (rest_of(&app.hosts.read(), &tab), &tab) {
        let (environment, id) = (environment.clone(), id.clone());
        return rsx! {
            EmptyState { kind: "detached", icon: Icon::PlayerStop, title: "Exited",
                div { "Its program {rest.word()}. Restart continues the conversation where it left off." }
                Button { kind: "primary", onclick: move |_| ui.bring_back(environment.clone(), vec![id.clone()]), "Restart" }
            }
        };
    }
    rsx! {
        EmptyState { "Starting…" }
    }
}
