//! A fixed Find field above the Tree and Agents sections, with room following focus.
use dioxus::prelude::*;
use tsumugi_core::actions::Action;
use tsumugi_core::content::Area;
use tsumugi_core::msg::Msg;
use tsumugi_core::sidebar::{Counts, Identity, Panel, Row};

use crate::bridge::AppCtx;
use crate::components::{StateGlyph, glyph};
use crate::connection;
use crate::icons::{Icon, IconView};
use crate::rename::{Place as RenamePlace, SessionName};
use crate::shell::Ui;

#[component]
fn SidebarFrame(
    panel: Panel,
    find: Element,
    tree: Element,
    agents: Element,
    onfocus: EventHandler<Panel>,
) -> Element {
    rsx! {
        div { class: "sidebar-frame",
            div { class: "sidebar-fixed-find", {find} }
            div { class: if panel == Panel::Agents { "sidebar-sections agents-focused" } else { "sidebar-sections" },
                for (section, label, id, heading, body) in [
                    (Panel::Tree, "Tree", "sidebar-tree", "sidebar-heading-tree", tree),
                    (Panel::Agents, "Agents", "sidebar-agents", "sidebar-heading-agents", agents),
                ] {
                    section { key: "{section:?}", id, role: "region", "aria-labelledby": heading,
                        class: if panel == section { "sidebar-section active" } else { "sidebar-section" },
                        onclick: move |_| { if panel != section { onfocus.call(section); } },
                        onfocusin: move |_| onfocus.call(section),
                        div { class: "sidebar-section-header",
                            button { id: heading, r#type: "button", class: "sidebar-section-title",
                                "aria-pressed": panel == section,
                                onclick: move |_| onfocus.call(section),
                                span { "{label}" }
                                span { class: "sidebar-section-key", "Tab" }
                            }
                        }
                        div { class: "sidebar-section-body", {body} }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn SidebarPanel(focused: bool, children: Element) -> Element {
    let app = use_context::<AppCtx>();
    let (shown, width) = {
        let chrome = app.chrome.read();
        (chrome.panels.layout.show_sidebar, chrome.panels.drawn().0)
    };
    let panel = use_memo(move || app.sidebar.read().panel);
    rsx! {
        div {
            class: if focused { "app-panel sidebar focused" } else { "app-panel sidebar" },
            hidden: !shown, style: "width:{width}px",
            onmousedown: move |_| app.dispatch(Msg::Focus(Area::Sidebar)),
            SidebarFrame {
                panel: panel(),
                find: rsx! { SidebarFind {} },
                tree: children,
                agents: rsx! { AgentsPanel {} },
                onfocus: move |panel| {
                    if app.sidebar.peek().panel != panel { app.dispatch(Msg::SidebarPanel(panel)); }
                },
            }
        }
    }
}

#[component]
fn SidebarFind() -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let panel = app.sidebar.read().panel;
    let every = app.tree.read().every_worktree;
    rsx! {
        div { class: "sidebar-bar",
            crate::tree::FilterField {
                panel,
                field: ui.local.filter_field,
                ontaken: move |_| app.dispatch(Msg::Focus(Area::Sidebar)),
                onfocus: move |on| ui.local.focus_filter(on),
            }
            span { class: "sidebar-tools",
                button { r#type: "button", class: if every { "sidebar-tool on" } else { "sidebar-tool" },
                    "aria-pressed": every, title: "Toggle empty worktrees in Tree",
                    onclick: move |_| {
                        app.dispatch(Msg::SidebarPanel(Panel::Tree));
                        ui.perform(Action::WorktreesToggle, None);
                    },
                    IconView { icon: Icon::ListTree }
                }
                button { r#type: "button", class: "sidebar-tool", title: "Start a session",
                    onclick: move |_| app.dispatch(Msg::PickerOpen), IconView { icon: Icon::Plus }
                }
            }
        }
    }
}

#[component]
fn AgentsPanel() -> Element {
    let app = use_context::<AppCtx>();
    let view = use_memo(move || {
        let mut view = app.sidebar.read().clone();
        // Moving the cursor changes only the two rows whose selection changed.
        view.selected = None;
        view.filtering = false;
        view
    });
    let selected = use_memo(move || app.sidebar.read().selected.clone());
    let focused = use_memo(move || app.sidebar.read().panel);
    use_effect(move || {
        let _ = (selected(), focused());
        document::eval(
            "{ const reveal = () => { const list = document.querySelector('#sidebar-agents .agents-list'); const row = list?.querySelector('.agent-row.selected'); if (!row) return; const a = list.getBoundingClientRect(); const b = row.getBoundingClientRect(); if (b.top < a.top) list.scrollTop += b.top - a.top; else if (b.bottom > a.bottom) list.scrollTop += b.bottom - a.bottom; }; requestAnimationFrame(reveal); setTimeout(reveal, 200); }",
        );
    });
    let rows = view.read().rows().to_vec();
    let totals = view.read().totals.clone();
    let mut environment = None;
    rsx! {
        div { class: "sidebar-view",
            div { class: "agents-overview", aria_label: "Agent totals across environments",
                span { class: "agents-overview-title", "{totals.total()} agents" }
                StateCounts { counts: totals }
            }
            div { class: "agents-list", role: "tree", aria_label: "Agents by parent session",
                if rows.is_empty() { div { class: "agents-empty", "No matching agents" } }
                for row in rows.iter() {
                  div { key: "{row.key:?}", role: "none",
                    if environment.as_ref() != Some(&row.key.environment) {
                        { environment = Some(row.key.environment.clone()); rsx! { AgentEnvironment { environment: row.key.environment.clone() } } }
                    }
                    AgentRow {
                        row: row.clone(),
                        parent_name: row.parent.as_ref().and_then(|key| rows.iter().find(|r| &r.key == key)).map(row_name),
                    }
                  }
                }
            }
        }
    }
}

#[component]
fn AgentEnvironment(environment: tsumugi_model::EnvironmentId) -> Element {
    let app = use_context::<AppCtx>();
    let standing = connection::standing_of(&app, &environment);
    let name = app.environment_name(&environment);
    rsx! {
        div { class: "agents-environment",
            connection::StandingDot { mark: standing.mark }
            span { "{name}" }
            if let Some(note) = standing.note { connection::StandingPill { note } }
            if standing.mark == connection::Mark::Stopped { span { "Last reported" } }
        }
    }
}

fn row_name(row: &Row) -> String {
    row.session
        .as_ref()
        .map(|s| s.name.clone())
        .unwrap_or_else(|| match &row.key.identity {
            Identity::Parent(id) => format!("Parent {}", &id[..id.len().min(8)]),
            Identity::Session(id) => id.clone(),
        })
}

#[component]
fn AgentRow(row: Row, parent_name: Option<String>) -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let key = row.key.clone();
    let selected = use_memo(move || app.sidebar.read().selected.as_ref() == Some(&key));
    let name = row_name(&row);
    let title = row
        .session
        .as_ref()
        .map(|s| {
            format!(
                "{} · {}\n{}\n{}",
                s.name,
                s.state.word(),
                s.cwd.display(),
                s.status_message.as_deref().unwrap_or_default()
            )
        })
        .unwrap_or_else(|| match &row.key.identity {
            Identity::Parent(id) => format!("The parent session is no longer listed: {id}"),
            _ => name.clone(),
        });
    let fold_key = row.key.clone();
    let open_key = row.key.clone();
    let can_open = row.session.is_some();
    let menu = row.session.as_ref().map(|session| {
        crate::shell::overlays::menu_opener(
            app,
            tsumugi_core::overlay::context_menu::Target::Agent(
                row.key.environment.clone(),
                session.id.clone(),
            ),
        )
    });
    let depth = row.depth.min(12);
    rsx! {
        div { class: if selected() { "agent-row selected" } else { "agent-row" },
            role: "treeitem", "aria-level": row.depth + 1, "aria-selected": selected(),
            "aria-expanded": if row.children > 0 { Some(row.open) } else { None },
            title, style: "padding-left:{8 + depth * 14}px",
            oncontextmenu: move |event: MouseEvent| {
                if let Some(menu) = menu { menu.call(event); } else { event.prevent_default(); }
            },
            onclick: move |_| {
                app.dispatch(Msg::SidebarPanel(Panel::Agents));
                app.dispatch(Msg::SidebarSelect(open_key.clone()));
                if can_open { ui.perform(Action::CursorGo, None); }
            },
            div { class: "agent-row-heading",
                if row.children > 0 {
                    button { class: "agent-fold", r#type: "button", aria_label: if row.open { "Fold agent family" } else { "Expand agent family" },
                        onclick: move |e| { e.stop_propagation(); app.dispatch(Msg::SidebarPanel(Panel::Agents)); app.dispatch(Msg::SidebarFold(fold_key.clone())); },
                        IconView { icon: if row.open { Icon::ChevronDown } else { Icon::ChevronRight }, size: 12 }
                    }
                } else { span { class: "agent-leaf" } }
                if let Some(session) = &row.session {
                    { let (class, icon, title) = glyph(&session.state); rsx! { StateGlyph { class, icon, title, size: 13 } } }
                    span { class: "agent-name",
                        SessionName { environment: row.key.environment.clone(), id: session.id.clone(), name: name.clone(), place: RenamePlace::Sidebar, sidebar_panel: Panel::Agents }
                    }
                    span { class: "agent-state", "{session.state.word()}" }
                } else {
                    IconView { icon: Icon::GitBranch, size: 13 }
                    span { class: "agent-name", "{name}" }
                    span { class: "agent-state", "Not listed" }
                }
            }
            if let Some(session) = &row.session {
                div { class: "agent-row-meta",
                    span { "{session.agent.label()}" }
                    if let Some(code) = &session.code { span { "{code}" } }
                    span { class: "agent-directory", "{session.cwd.display()}" }
                }
            }
            if let Some(parent) = parent_name { div { class: "agent-origin", "Started by {parent}" } }
            if row.children > 0 {
                div { class: "agent-family", title: "Reported states of this session and its descendants",
                    span { "Family" }
                    StateCounts { counts: row.counts.clone() }
                }
            }
        }
    }
}

#[component]
fn StateCounts(counts: Counts) -> Element {
    rsx! {
        span { class: "agent-counts",
            if counts.active() > 0 { span { class: "agent-total active", "{counts.active()} active" } }
            if counts.waiting > 0 { span { class: "agent-total waiting", "{counts.waiting} waiting" } }
            if counts.failed > 0 { span { class: "agent-total failed", "{counts.failed} failed" } }
            if counts.idle > 0 { span { class: "agent-total idle", "{counts.idle} idle" } }
            if counts.done > 0 { span { class: "agent-total done", "{counts.done} done" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Observed {
        active: Rc<Cell<Option<Signal<Panel>>>>,
        mounts: Rc<RefCell<BTreeMap<&'static str, usize>>>,
    }

    #[component]
    fn Probe(name: &'static str) -> Element {
        let observed = use_context::<Observed>();
        use_hook(move || {
            *observed.mounts.borrow_mut().entry(name).or_default() += 1;
        });
        rsx! { input { id: name } }
    }

    #[component]
    fn Frame() -> Element {
        let observed = use_context::<Observed>();
        let panel = use_signal(Panel::default);
        observed.active.set(Some(panel));
        rsx! { SidebarFrame {
            panel: panel(),
            find: rsx! { Probe { name: "find" } },
            tree: rsx! { Probe { name: "tree" } },
            agents: rsx! { Probe { name: "agents" } },
            onfocus: move |_| {},
        } }
    }

    #[tokio::test]
    async fn both_lists_stay_mounted_and_find_keeps_its_input_when_focus_changes() {
        let observed = Observed::default();
        let mut dom = VirtualDom::new(Frame).with_root_context(observed.clone());
        dom.rebuild_in_place();
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        let expected = BTreeMap::from([("find", 1), ("tree", 1), ("agents", 1)]);
        assert_eq!(*observed.mounts.borrow(), expected);
        for panel in [Panel::Agents, Panel::Tree] {
            dom.in_runtime(|| *observed.active.get().unwrap().write() = panel);
            dom.wait_for_work().await;
            dom.render_immediate(&mut dioxus::core::NoOpMutations);
            assert_eq!(*observed.mounts.borrow(), expected);
        }
    }
}
