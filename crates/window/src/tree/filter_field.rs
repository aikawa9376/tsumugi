//! The fixed Find field searches whichever sidebar section has the keys.

use std::rc::Rc;

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::sidebar::Panel;

use crate::bridge::AppCtx;
use crate::components::InlineField;
use crate::icons::{Icon, IconView};

/// The field mounts into `field`, through which the window gives it the
/// keys and takes them back; `onfocus` asks for that.
#[component]
pub(crate) fn FilterField(
    #[props(default)] panel: Panel,
    field: Signal<Option<Rc<MountedData>>>,
    onfocus: EventHandler<bool>,
    ontaken: EventHandler<()>,
) -> Element {
    let app = use_context::<AppCtx>();
    let (on, text) = {
        match panel {
            Panel::Tree => {
                let view = app.tree.read();
                (view.filtering, view.filter.clone())
            }
            Panel::Agents => {
                let view = app.sidebar.read();
                (view.filtering, view.filter.clone())
            }
        }
    };
    rsx! {
        div {
            class: if on { "filter-field on" } else { "filter-field" },
            title: "Find in the focused section; Enter keeps it, Esc clears it",
            // A click goes on with what it holds, where `/` starts afresh.
            onclick: move |_| onfocus.call(true),
            span { class: "filter-field-funnel", IconView { icon: Icon::Filter, size: 13 } }
            InlineField {
                class: "filter-field-input",
                placeholder: "Find",
                aria_label: "Find in the focused sidebar section",
                value: text,
                onmounted: move |data| field.set(Some(data)),
                onfocus: move |()| {
                    ontaken.call(());
                    app.dispatch(match panel { Panel::Tree => Msg::TreeFiltering(true), Panel::Agents => Msg::SidebarFiltering(true) });
                },
                onblur: move |()| app.dispatch(match panel { Panel::Tree => Msg::TreeFiltering(false), Panel::Agents => Msg::SidebarFiltering(false) }),
                oninput: move |text| app.dispatch(match panel { Panel::Tree => Msg::TreeFilter(text), Panel::Agents => Msg::SidebarFilter(text) }),
                onenter: move |()| onfocus.call(false),
                onescape: move |()| {
                    app.dispatch(match panel { Panel::Tree => Msg::TreeFilter(String::new()), Panel::Agents => Msg::SidebarFilter(String::new()) });
                    onfocus.call(false);
                },
            }
        }
    }
}
