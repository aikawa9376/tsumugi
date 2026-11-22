//! A window on the agent board: a column for each lane, a card for each
//! session, read with the board's keys or the pointer.

use dioxus::prelude::*;
use tsumugi_core::board::{Card, Child, Lane, LaneCards};
use tsumugi_core::content::{Area, Opening, WindowId};
use tsumugi_core::msg::Msg;
use tsumugi_core::tree::nav::Item;
use tsumugi_model::time::now_ms;

use crate::bridge::AppCtx;
use crate::components::{AgentBadge, InlineField, StateGlyph, glyph};
use crate::icons::{Icon, IconView};

/// How often the board looks at the clock, which tells how long its cards
/// have been quiet.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

#[component]
pub(crate) fn BoardWindow(window: WindowId) -> Element {
    let app = use_context::<AppCtx>();
    use_future(move || async move {
        loop {
            app.dispatch(Msg::BoardClock(now_ms()));
            tokio::time::sleep(LOOK_EVERY).await;
        }
    });
    let mut field = use_context::<crate::shell::Ui>().local.board_field;
    let board = app.board.read().clone();
    let (lanes, projects) = {
        let hosts = app.hosts.read();
        let tree = app.tree.read();
        (
            board.lanes(&hosts, &tree.completions),
            board.projects(&hosts, &tree.completions),
        )
    };
    let total: usize = lanes.iter().map(|l| l.cards.len()).sum();
    let narrowed =
        !board.query.trim().is_empty() || !board.projects.is_empty() || !board.states.is_empty();
    let selected = board.selected().cloned();
    // The card under the cursor stays in view as it moves.
    use_effect(move || {
        let _ = app.board.read().selected();
        document::eval(
            "document.querySelector('.board-card.selected')?.scrollIntoView({block:'nearest',inline:'nearest'});",
        );
    });
    rsx! {
        div {
            class: "board",
            onmousedown: move |_| {
                app.dispatch(Msg::WindowFocus(window));
                app.dispatch(Msg::Focus(Area::Console));
            },
            div { class: "board-bar",
                span { class: "board-title", "Agents" }
                span { class: "board-total", "{total} total" }
                div { class: "board-search",
                    IconView { icon: Icon::Search, size: 13 }
                    InlineField {
                        class: "board-search-input",
                        placeholder: "Search agents, worktrees, messages",
                        aria_label: "Search the agent board",
                        value: board.query.clone(),
                        onmounted: move |data| field.set(Some(data)),
                        onfocus: move |()| app.dispatch(Msg::BoardSearching(true)),
                        onblur: move |()| app.dispatch(Msg::BoardSearching(false)),
                        oninput: move |text| app.dispatch(Msg::BoardQuery(text)),
                        onenter: move |()| app.dispatch(Msg::Focus(Area::Console)),
                        onescape: move |()| {
                            app.dispatch(Msg::BoardQuery(String::new()));
                            app.dispatch(Msg::Focus(Area::Console));
                        },
                    }
                }
                button {
                    class: if board.show_idle { "board-chip on" } else { "board-chip" },
                    title: "Show agents that finished and have been quiet for half an hour",
                    onclick: move |_| app.dispatch(Msg::Window(tsumugi_core::actions::Action::BoardIdle)),
                    "Show idle"
                }
                button {
                    class: if board.group { "board-chip on" } else { "board-chip" },
                    title: "Fold the sessions a session opened under its card",
                    onclick: move |_| app.dispatch(Msg::Window(tsumugi_core::actions::Action::BoardGroup)),
                    "Group by opener"
                }
            }
            div { class: "board-chips",
                for lane in Lane::ALL {
                    if lane != Lane::Idle || board.show_idle {
                        button {
                            key: "lane-{lane.label()}",
                            class: if board.states.contains(&lane) { "board-chip on" } else { "board-chip" },
                            onclick: move |_| app.dispatch(Msg::BoardLane(lane)),
                            "{lane.label()}"
                        }
                    }
                }
                span { class: "board-chips-gap" }
                for project in projects {
                    button {
                        key: "project-{project.name}",
                        class: if board.projects.contains(&project.name) { "board-chip on" } else { "board-chip" },
                        onclick: {
                            let name = project.name.clone();
                            move |_| app.dispatch(Msg::BoardProject(name.clone()))
                        },
                        "{project.name}"
                        span { class: "board-chip-count", "{project.count}" }
                    }
                }
                if narrowed {
                    button {
                        class: "board-chip clear",
                        onclick: move |_| app.dispatch(Msg::Window(tsumugi_core::actions::Action::BoardClear)),
                        "Clear all"
                    }
                }
            }
            div { class: "board-lanes",
                for LaneCards { lane , cards } in lanes {
                    section {
                        key: "{lane.label()}",
                        class: "board-lane",
                        header { class: "board-lane-head",
                            span { class: "board-lane-name", "{lane.label()}" }
                            span { class: "board-lane-count", "{cards.len()}" }
                        }
                        div { class: "board-cards",
                            if cards.is_empty() {
                                p { class: "board-none", "None" }
                            }
                            for card in cards {
                                BoardCard {
                                    key: "{card.environment}-{card.id}",
                                    selected: selected.as_ref() == Some(&card.key()),
                                    open: board.expanded.contains(&card.key()),
                                    card,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BoardCard(card: Card, selected: bool, open: bool) -> Element {
    let app = use_context::<AppCtx>();
    let class = format!(
        "board-card{}{}{}",
        match card.lane {
            Lane::NeedsYou => " needs",
            Lane::Done => " done",
            _ => "",
        },
        if selected { " selected" } else { "" },
        if card.unread { " unread" } else { "" },
    );
    let (environment, id) = (card.environment.clone(), card.id.clone());
    let when = if card.lane == Lane::Idle {
        card.quiet()
    } else {
        card.age()
    };
    let mark = glyph(&card.state);
    let children = card.children.len();
    rsx! {
        div {
            class,
            onclick: {
                let (environment, id) = (environment.clone(), id.clone());
                move |_| {
                    app.dispatch(Msg::BoardSelect(environment.clone(), id.clone()));
                    app.dispatch(Msg::OpenRow {
                        item: Item::Session(environment.clone(), id.clone()),
                        how: Some(Opening::Existing),
                        take_keys: true,
                    });
                }
            },
            div { class: "board-card-head",
                AgentBadge { label: card.agent.label().to_string() }
                span { class: "board-card-name", "{card.name}" }
                if let Some((class, icon, word)) = mark {
                    StateGlyph { class: class.to_string(), icon, size: 13, title: Some(word.to_string()) }
                }
            }
            if let Some(said) = &card.last_user {
                div { class: "board-said user",
                    span { class: "board-said-who", "You" }
                    " {said}"
                }
            }
            if let Some(said) = &card.last_agent {
                div { class: "board-said agent",
                    span { class: "board-said-who", "{card.agent.label()}" }
                    " {said}"
                }
            }
            if let Some(ask) = &card.ask {
                div { class: "board-ask", "{ask}" }
            }
            if children > 0 {
                button {
                    class: "board-children-toggle",
                    onclick: {
                        let (environment, id) = (environment.clone(), id.clone());
                        move |e| {
                            e.stop_propagation();
                            app.dispatch(Msg::BoardSelect(environment.clone(), id.clone()));
                            app.dispatch(Msg::Window(tsumugi_core::actions::Action::BoardExpand));
                        }
                    },
                    if open {
                        "▾ "
                    } else {
                        "▸ "
                    }
                    "{children} opened"
                    if card.asking > 0 {
                        span { class: "board-children-asking", " · {card.asking} need you" }
                    }
                }
                if open {
                    div { class: "board-children",
                        for child in card.children.clone() {
                            BoardChild { key: "{child.id}", child }
                        }
                    }
                }
            }
            div { class: "board-card-foot",
                span { class: "board-card-where", title: "{card.worktree}", "{card.project} · {card.worktree}" }
                span { class: "board-card-age", "{when}" }
            }
        }
    }
}

#[component]
fn BoardChild(child: Child) -> Element {
    let app = use_context::<AppCtx>();
    let mark = glyph(&child.state);
    rsx! {
        div {
            class: "board-child",
            onclick: move |e| {
                e.stop_propagation();
                app.dispatch(Msg::OpenRow {
                    item: Item::Session(child.environment.clone(), child.id.clone()),
                    how: Some(Opening::Existing),
                    take_keys: true,
                });
            },
            if let Some((class, icon, word)) = mark {
                StateGlyph { class: class.to_string(), icon, size: 12, title: Some(word.to_string()) }
            }
            span { class: "board-child-name", "{child.name}" }
        }
    }
}
