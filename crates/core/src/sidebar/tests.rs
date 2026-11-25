use super::*;
use tsumugi_model::{SessionId, Wait};

use crate::content::{Area, Tab};
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{here, listing, session, shows_there, there};

trait Press {
    fn press(self, name: &str) -> Self;
}

impl Press for Scenario {
    fn press(self, name: &str) -> Self {
        let run = self.send(Msg::Key {
            key: tsumugi_model::Key::new(name),
            probe: Box::default(),
        });
        let (action, key) = run
            .last_effects()
            .iter()
            .find_map(|effect| match effect {
                crate::effect::Effect::PerformKey(action, key) => {
                    Some((action.clone(), key.clone()))
                }
                _ => None,
            })
            .expect("the sidebar key resolves to an action");
        run.send(Msg::Perform {
            action,
            key: Some(key),
            probe: Box::default(),
        })
    }
}

fn key(id: &str) -> NodeKey {
    NodeKey::session(here(), id)
}

fn family() -> Scenario {
    let parent = SessionId::new();
    let child = SessionId::new();
    Scenario::new().event(listing([
        session("grandchild").parent(child).state(State::Working),
        session("parent").delegation(parent).state(State::Idle),
        session("child")
            .delegation(child)
            .parent(parent)
            .waiting(Wait::Permission),
        session("independent").state(State::Done),
    ]))
}

fn view(run: &Scenario) -> Sidebar {
    let mut view = Sidebar::default();
    view.rebuild(&run.state().hosts);
    view
}

#[test]
fn parents_precede_the_children_they_started_even_in_other_directories() {
    let view = view(&family());
    let rows = view.rows();
    let parent = rows.iter().position(|r| r.key == key("parent")).unwrap();
    assert_eq!(rows[parent].depth, 0);
    assert_eq!(rows[parent + 1].key, key("child"));
    assert_eq!(rows[parent + 1].depth, 1);
    assert_eq!(rows[parent + 2].key, key("grandchild"));
    assert_eq!(rows[parent + 2].depth, 2);
    assert_eq!(rows[parent].counts.total(), 3);
    assert_eq!(rows[parent].counts.working, 1);
    assert_eq!(rows[parent].counts.waiting, 1);
    assert_eq!(view.totals.total(), 4);
}

#[test]
fn folding_a_family_keeps_its_activity_and_does_not_change_global_counts() {
    let run = family();
    let mut view = view(&run);
    view.selected = Some(key("grandchild"));
    view.collapsed.insert(key("parent"));
    view.rebuild(&run.state().hosts);
    assert_eq!(view.rows().len(), 2);
    let parent = view.rows().iter().find(|r| r.key == key("parent")).unwrap();
    assert!(!parent.open);
    assert_eq!(parent.counts.active(), 1);
    assert_eq!(parent.counts.waiting, 1);
    assert_eq!(view.totals.total(), 4);
    assert_eq!(view.selected, Some(key("parent")));
}

#[test]
fn a_parent_no_longer_running_stays_above_its_children_as_an_unavailable_parent() {
    let parent = SessionId::new();
    let run = Scenario::new().event(listing([
        session("one").parent(parent).state(State::Working),
        session("two").parent(parent).state(State::Failed),
    ]));
    let view = view(&run);
    assert_eq!(view.rows().len(), 3);
    assert_eq!(
        view.rows()[0].key.identity,
        Identity::Parent(parent.to_string())
    );
    assert!(view.rows()[0].session.is_none());
    assert_eq!(view.rows()[0].children, 2);
    assert_eq!(view.rows()[0].counts.total(), 2);
    assert_eq!(view.totals.total(), 2);
}

#[test]
fn parent_identities_never_link_sessions_across_environments() {
    let id = SessionId::new();
    let run = shows_there(
        Scenario::new().event(listing([session("same").delegation(id)])),
        [session("same").parent(id).state(State::Working)],
    );
    let view = view(&run);
    assert_eq!(view.rows().len(), 3);
    let remote = view
        .rows()
        .iter()
        .find(|r| r.key == NodeKey::session(there(), "same"))
        .unwrap();
    assert_eq!(remote.depth, 1);
    assert_eq!(remote.parent.as_ref().unwrap().environment, there());
    assert_eq!(view.totals.total(), 2);
}

#[test]
fn a_cycle_lists_every_session_once_without_counting_it_twice() {
    let a = SessionId::new();
    let b = SessionId::new();
    let run = Scenario::new().event(listing([
        session("a").delegation(a).parent(b).state(State::Working),
        session("b").delegation(b).parent(a).state(State::Working),
    ]));
    let view = view(&run);
    assert_eq!(view.rows().len(), 2);
    assert_eq!(view.rows()[0].depth, 0);
    assert_eq!(view.rows()[1].depth, 1);
    assert_eq!(view.rows()[0].counts.total(), 2);
    assert_eq!(view.totals.working, 2);
}

#[test]
fn filtering_a_descendant_keeps_its_ancestors_and_temporarily_opens_its_family() {
    let run = family();
    let mut view = view(&run);
    view.collapsed.insert(key("parent"));
    view.filter = "GRANDCHILD".into();
    view.rebuild(&run.state().hosts);
    assert_eq!(
        view.rows()
            .iter()
            .map(|r| r.key.clone())
            .collect::<Vec<_>>(),
        [key("parent"), key("child"), key("grandchild")]
    );
    assert!(view.rows()[0].open);
    view.filter.clear();
    view.rebuild(&run.state().hosts);
    assert!(!view.rows().iter().any(|r| r.key == key("grandchild")));
    view.selected = Some(key("grandchild"));
    view.filter = "no matching session".into();
    view.rebuild(&run.state().hosts);
    assert!(view.rows().is_empty());
    assert!(view.selected.is_none());
}

#[test]
fn tab_switches_views_without_changing_the_directory_tree_or_its_selection() {
    let run = family().send(Msg::Focus(Area::Sidebar));
    let tree = run.state().tree.clone();
    let run = run.press("Tab");
    assert_eq!(run.state().sidebar.panel, Panel::Agents);
    assert_eq!(run.state().tree.folds, tree.folds);
    assert_eq!(run.state().tree.selected, tree.selected);
    let selected = run.state().sidebar.selected.clone();
    let run = run.press("j");
    let moved = run.state().sidebar.selected.clone();
    assert_ne!(moved, selected);
    let run = run.press("Tab").press("Tab");
    assert_eq!(run.state().sidebar.selected, moved);
    assert_eq!(run.state().tree.selected, tree.selected);
}

#[test]
fn moving_the_agents_cursor_shows_its_tab_and_keeps_the_panel_focused() {
    let mut run = family()
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::SidebarSelect(key("parent")));
    for (pressed, id) in [
        ("j", "child"),
        ("j", "grandchild"),
        ("k", "child"),
        ("h", "child"),
        ("h", "parent"),
        ("l", "child"),
        ("k", "parent"),
        ("G", "child"),
        ("g", "independent"),
    ] {
        run = if pressed == "g" {
            run.send(Msg::Key {
                key: tsumugi_model::Key::new("g"),
                probe: Box::default(),
            })
            .press("g")
        } else {
            run.press(pressed)
        };
        assert_eq!(run.state().sidebar.selected, Some(key(id)), "{pressed}");
        assert_eq!(
            run.state().content.active(),
            Some(&Tab::Session(here(), id.into())),
            "{pressed}"
        );
        assert_eq!(run.state().content.focus, Area::Sidebar, "{pressed}");
        assert_eq!(run.state().details.shown.session.as_deref(), Some(id));
        assert_eq!(run.state().details.shown.environment, here());
    }
    let run = run.press("Enter");
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn moving_to_an_agent_in_another_environment_shows_that_environments_tab() {
    let run = shows_there(
        Scenario::new().event(listing([session("same")])),
        [session("same")],
    )
    .send(Msg::SidebarPanel(Panel::Agents))
    .send(Msg::SidebarSelect(key("same")))
    .press("j");
    assert_eq!(
        run.state().sidebar.selected,
        Some(NodeKey::session(there(), "same"))
    );
    assert_eq!(
        run.state().content.active(),
        Some(&Tab::Session(there(), "same".into()))
    );
    assert_eq!(run.state().content.focus, Area::Sidebar);
    assert_eq!(run.state().details.shown.environment, there());
    assert_eq!(run.state().details.shown.session.as_deref(), Some("same"));
}

#[test]
fn every_details_tab_follows_the_agents_cursor_while_the_keys_stay_in_agents() {
    use crate::chrome::panels::layout::Side;
    use crate::effect::Effect;
    use tsumugi_protocol::Request;

    for tab in 0..5 {
        let run = family()
            .send(crate::testing::show(Tab::Session(here(), "parent".into())))
            .send(Msg::PanelShow(Side::Details))
            .send(Msg::SidebarPanel(Panel::Agents))
            .send(Msg::SidebarSelect(key("parent")))
            .send(Msg::DetailsTab(tab))
            .press("j");
        let details = &run.state().details;
        assert_eq!(details.drill.tab(), tab);
        assert_eq!(details.shown.session.as_deref(), Some("child"));
        assert_eq!(
            details.shown.worktree.as_deref(),
            Some(std::path::Path::new("/r/child"))
        );
        assert_eq!(
            crate::details::shown_session(details, run.state().hosts.work(&here()))
                .unwrap()
                .name,
            "child-name"
        );
        assert!(run.last_effects().iter().any(|effect| matches!(effect,
            Effect::Host { environment, request: Request::Watch(sight) }
                if environment == &here()
                    && sight.details
                    && sight.session.as_deref() == Some("child")
                    && sight.repositories.iter().all(|repo| repo.worktree == std::path::Path::new("/r/child"))
        )), "details tab {tab} must read the session at the cursor");
        assert_eq!(run.state().content.focus, Area::Sidebar);
    }
}

#[test]
fn moving_to_an_unavailable_parent_keeps_the_tab_that_was_shown() {
    let run = Scenario::new()
        .event(listing([session("child").parent(SessionId::new())]))
        .send(crate::testing::show(Tab::Session(here(), "child".into())))
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::SidebarSelect(key("child")))
        .press("k");
    assert!(matches!(
        run.state().sidebar.selected.as_ref().unwrap().identity,
        Identity::Parent(_)
    ));
    assert_eq!(
        run.state().content.active(),
        Some(&Tab::Session(here(), "child".into()))
    );
    assert_eq!(run.state().content.focus, Area::Sidebar);
}

#[test]
fn agents_open_and_stop_the_selected_session_in_its_own_environment() {
    let run = shows_there(family(), [session("remote").state(State::Working)])
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::SidebarSelect(NodeKey::session(there(), "remote")));
    let run = run.press("Enter");
    assert_eq!(
        run.state().content.active(),
        Some(&Tab::Session(there(), "remote".into()))
    );
    let run = run.send(Msg::Focus(Area::Sidebar)).press("X");
    match run.state().overlays.confirm.as_ref() {
        Some(crate::overlay::confirm::Ask::Stop(environment, session)) => {
            assert_eq!(environment, &there());
            assert_eq!(session.id, "remote");
        }
        other => panic!("expected a stop confirmation for the selected agent: {other:?}"),
    }
}

#[test]
fn h_and_l_fold_and_enter_agent_families_without_folding_the_directory_tree() {
    let run = family()
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::SidebarSelect(key("parent")));
    let folds = run.state().tree.folds.clone();
    let run = run.press("h");
    assert_eq!(run.state().sidebar.rows().len(), 2);
    let run = run.press("l").press("l");
    assert_eq!(run.state().sidebar.selected, Some(key("child")));
    assert_eq!(run.state().tree.folds, folds);
}

#[test]
fn tab_in_the_details_panel_does_not_switch_the_left_sidebar() {
    let run = family().send(Msg::Focus(Area::Details)).press("Tab");
    assert_eq!(run.state().sidebar.panel, Panel::Tree);
    assert_eq!(run.state().content.focus, Area::Details);
}

#[test]
fn an_unavailable_parent_does_not_rename_or_stop_the_session_in_the_console() {
    let parent = SessionId::new();
    let run = Scenario::new()
        .event(listing([session("orphan").parent(parent)]))
        .send(Msg::ShowSession(here(), "orphan".into()))
        .send(Msg::SidebarPanel(Panel::Agents));
    let run = run.send(Msg::SidebarSelect(NodeKey {
        environment: here(),
        identity: Identity::Parent(parent.to_string()),
    }));
    let run = run.press("X");
    assert!(run.state().overlays.confirm.is_none());
    let run = run.press("R");
    assert!(run.state().overlays.rename.is_none());
}

#[test]
fn an_agent_context_menu_has_the_tree_session_actions_and_targets_its_own_row() {
    use crate::actions::Action;
    use crate::overlay::context_menu::{Reach, Target, entries};
    let session = session("remote").state(State::Working).build();
    let target = Target::Agent(there(), "remote".into());
    assert_eq!(
        entries(&target, Some(&session), &here(), Reach::Here)
            .into_iter()
            .filter(|(action, _)| *action != Action::AgentsReveal)
            .collect::<Vec<_>>(),
        entries(
            &Target::Row(crate::tree::nav::Item::Session(there(), "remote".into())),
            Some(&session),
            &here(),
            Reach::Here
        )
    );
    let run = shows_there(family(), [session]).send(Msg::SidebarPanel(Panel::Tree));
    let run = run.send(Msg::MenuRun {
        target,
        action: Action::SessionStop,
    });
    assert_eq!(run.state().sidebar.panel, Panel::Agents);
    let run = run.send(Msg::Perform {
        action: Action::SessionStop,
        key: None,
        probe: Box::default(),
    });
    match run.state().overlays.confirm.as_ref() {
        Some(crate::overlay::confirm::Ask::Stop(environment, session)) => {
            assert_eq!(environment, &there());
            assert_eq!(session.id, "remote");
        }
        other => panic!("the context menu must target the clicked agent: {other:?}"),
    }
}

#[test]
fn an_agent_menu_offers_show_in_tree() {
    use crate::overlay::context_menu::{Reach, Target, entries};
    let entries = entries(
        &Target::Agent(here(), "child".into()),
        Some(&session("child").build()),
        &here(),
        Reach::Here,
    );
    assert!(entries.iter().any(|(action, label)| {
        *action == crate::actions::Action::AgentsReveal && *label == "Show in Tree"
    }));
}

#[test]
fn showing_an_agent_in_tree_unfolds_its_directory_path_and_selects_its_own_row() {
    use crate::tree::nav::{Heading, Item};
    for remote in [false, true] {
        for flat in [false, true] {
            let directory = if flat { "/r" } else { "/r/topic" };
            let target = session("target").cwd(directory);
            let run = family().send(Msg::ShowSession(here(), "parent".into()));
            let (run, environment) = if remote {
                (shows_there(run, [target]), there())
            } else {
                (
                    run.event(listing([target, session("parent").cwd(directory)])),
                    here(),
                )
            };
            let ancestors = [
                Item::Environment(environment.clone()),
                Item::Workspace(environment.clone(), "/r".into()),
                Item::Worktree(environment.clone(), "/r/topic".into()),
                Item::Heading(environment.clone(), directory.into(), Heading::Sessions),
            ];
            let unrelated = Item::Workspace(environment.clone(), "/other".into());
            let mut run = run.send(Msg::TreeFold {
                row: unrelated.clone(),
                open: false,
            });
            for row in &ancestors {
                run = run.send(Msg::TreeFold {
                    row: row.clone(),
                    open: false,
                });
            }
            let run = run
                .send(Msg::TreeFilter("no matching session".into()))
                .send(Msg::SidebarPanel(Panel::Agents))
                .send(Msg::SidebarSelect(NodeKey::session(
                    environment.clone(),
                    "target",
                )))
                .send(Msg::SidebarFilter("target".into()));
            let content = run.state().content.clone();
            let run = run
                .send(Msg::Key {
                    key: tsumugi_model::Key::new("g"),
                    probe: Box::default(),
                })
                .press("t");
            let row = Item::Session(environment, "target".into());
            assert_eq!(run.state().sidebar.panel, Panel::Tree);
            assert_eq!(run.state().content.focus, Area::Sidebar);
            assert_eq!(run.state().content.windows, content.windows);
            assert_eq!(run.state().tree.selected.as_ref(), Some(&row));
            assert!(run.state().tree.rows().contains(&row));
            assert!(run.state().tree.filter.is_empty());
            assert!(!run.state().tree.folds.is_open(&unrelated));
            assert_eq!(run.state().sidebar.filter, "target");
            assert_eq!(
                run.state().sidebar.selected,
                Some(NodeKey::session(row.environment().clone(), "target"))
            );
        }
    }
}

#[test]
fn show_in_tree_from_an_agent_menu_targets_the_clicked_session() {
    use crate::overlay::context_menu::Target;
    let action = crate::actions::Action::AgentsReveal;
    let run = shows_there(family(), [session("remote")])
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::SidebarSelect(key("parent")))
        .send(Msg::MenuRun {
            target: Target::Agent(there(), "remote".into()),
            action: action.clone(),
        })
        .send(Msg::Perform {
            action,
            key: None,
            probe: Box::default(),
        });
    assert_eq!(run.state().sidebar.panel, Panel::Tree);
    assert_eq!(
        run.state().tree.selected,
        Some(tree::nav::Item::Session(there(), "remote".into()))
    );
}

#[test]
fn an_unavailable_parent_cannot_reveal_the_session_in_the_console() {
    let parent = SessionId::new();
    let run = Scenario::new()
        .event(listing([session("orphan").parent(parent)]))
        .send(Msg::ShowSession(here(), "orphan".into()))
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::SidebarSelect(NodeKey {
            environment: here(),
            identity: Identity::Parent(parent.to_string()),
        }));
    let tree = run.state().tree.clone();
    let run = run.send(Msg::Perform {
        action: crate::actions::Action::AgentsReveal,
        key: None,
        probe: Box::default(),
    });
    assert_eq!(run.state().sidebar.panel, Panel::Agents);
    assert_eq!(run.state().tree, tree);
}

#[test]
fn a_tree_context_menu_focuses_tree_even_after_an_agent_had_the_cursor() {
    use crate::actions::Action;
    use crate::overlay::context_menu::Target;
    let run = family()
        .send(Msg::SidebarPanel(Panel::Agents))
        .send(Msg::MenuRun {
            target: Target::Row(crate::tree::nav::Item::Session(here(), "child".into())),
            action: Action::SessionStop,
        });
    assert_eq!(run.state().sidebar.panel, Panel::Tree);
}
