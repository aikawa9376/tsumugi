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
        entries(&target, Some(&session), &here(), Reach::Here),
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
