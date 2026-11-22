//! What a window shows of a session it has no screen of yet: one still
//! starting, one the sessions' daemon took with it, or one whose program
//! exited.

use dioxus::prelude::*;
use tsumugi_core::content::{Tab, rest_of, waits_to_come_back};
use tsumugi_model::Rest;

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
    // Shown, a session that sleeps is no longer left alone: it wakes as its
    // tab comes up, and the tab takes what stands for it once that is listed.
    let asleep = tab.clone();
    let waking = ui.clone();
    use_effect(move || {
        if let Tab::Session(environment, id) = &asleep
            && rest_of(&app.hosts.read(), &asleep) == Some(Rest::Hibernated)
        {
            waking.bring_back(environment.clone(), vec![id.clone()]);
        }
    });
    if waits_to_come_back(&app.hosts.read(), &tab) {
        return rsx! {
            EmptyState { "The sessions' daemon took this with it when it went. Bring it back from the banner, or with Bring Back Sessions… in the Session menu." }
        };
    }
    if let (Some(rest), Tab::Session(environment, id)) = (rest_of(&app.hosts.read(), &tab), &tab) {
        let (environment, id) = (environment.clone(), id.clone());
        if rest == Rest::Hibernated {
            return rsx! {
                EmptyState { kind: "detached", icon: Icon::SunMoon, title: "Hibernating",
                    div { "Waking it with its conversation…" }
                }
            };
        }
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
