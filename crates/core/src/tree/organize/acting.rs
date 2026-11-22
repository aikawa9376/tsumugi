//! Arranging the tree from its rows: what the actions do, and what the rows
//! look like once they have.

use tsumugi_model::State;

use crate::actions::Action;
use crate::console::Probe;
use crate::content::Area;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{here, listed, session, session_tab, shell, show, snapshot, worktree};
use crate::tree::nav::Item;

use super::{Hide, Organize, Sort};

/// The repository `/r` whose worktrees `/r/b` (a done session), `/r/a` (a
/// working one) and `/r/c` (nothing) are listed in that order after the main
/// checkout, a shell running in `/r/d`.
fn repository() -> Scenario {
    Scenario::new()
        .event(
            snapshot().listed(
                listed()
                    .session(session("sb").cwd("/r/b").started_ms(100).state(State::Done))
                    .session(
                        session("sa")
                            .cwd("/r/a")
                            .started_ms(900)
                            .state(State::Working),
                    )
                    .terminal(shell(7, "/r/d"))
                    .found("/r/b", Some(worktree("/r/b", "/r", "b")))
                    .found("/r/a", Some(worktree("/r/a", "/r", "a")))
                    .found("/r/d", Some(worktree("/r/d", "/r", "d")))
                    .worktree(worktree("/r", "/r", "main"))
                    .worktree(worktree("/r/b", "/r", "b"))
                    .worktree(worktree("/r/a", "/r", "a"))
                    .worktree(worktree("/r/c", "/r", "c"))
                    .worktree(worktree("/r/d", "/r", "d")),
            ),
        )
        .send(Msg::Focus(Area::Sidebar))
}

fn perform(action: Action) -> Msg {
    Msg::Perform {
        action,
        key: None,
        probe: Box::new(Probe::default()),
    }
}

fn on(run: Scenario, row: Item, action: Action) -> Scenario {
    run.send(Msg::TreeSelect(Some(row))).send(perform(action))
}

fn worktree_row(path: &str) -> Item {
    Item::Worktree(here(), path.into())
}

/// The worktrees' rows, top to bottom, by directory name.
fn worktree_rows(run: &Scenario) -> Vec<String> {
    run.state()
        .tree
        .rows()
        .iter()
        .filter_map(|row| match row {
            Item::Worktree(_, path) => path.file_name().map(|f| f.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn kept(run: &Scenario) -> Option<Organize> {
    run.last_effects().iter().find_map(|e| match e {
        Effect::KeepOrganize(organize) => Some(organize.clone()),
        _ => None,
    })
}

#[test]
fn pinning_a_worktree_puts_its_row_first_among_its_workspaces_and_keeps_the_arrangement() {
    let before = worktree_rows(&repository());
    assert_ne!(before[0], "c", "{before:?}");

    let run = on(repository(), worktree_row("/r/c"), Action::WorkspacePin);

    assert_eq!(worktree_rows(&run)[0], "c");
    let written = kept(&run).expect("kept for the next run");
    assert!(written.is_pinned(&here(), std::path::Path::new("/r/c")));
}

#[test]
fn pinning_again_unpins_and_the_row_goes_back() {
    let run = on(repository(), worktree_row("/r/c"), Action::WorkspacePin);
    let run = on(run, worktree_row("/r/c"), Action::WorkspacePin);

    assert_ne!(worktree_rows(&run)[0], "c");
    assert!(
        !kept(&run)
            .unwrap()
            .is_pinned(&here(), std::path::Path::new("/r/c"))
    );
}

#[test]
fn a_workspace_is_pinned_from_its_own_row() {
    let run = on(
        repository(),
        Item::Workspace(here(), "/r".into()),
        Action::WorkspacePin,
    );

    assert!(
        kept(&run)
            .unwrap()
            .is_pinned(&here(), std::path::Path::new("/r"))
    );
}

#[test]
fn a_row_that_cannot_be_pinned_does_nothing() {
    let run = on(
        repository(),
        Item::Session(here(), "sa".into()),
        Action::WorkspacePin,
    );

    assert!(kept(&run).is_none());
}

#[test]
fn the_sort_steps_through_the_orders_and_says_which_it_is() {
    let run = repository().send(perform(Action::TreeSort));

    assert_eq!(run.state().tree.organize.sort, Sort::Activity);
    assert_eq!(kept(&run).unwrap().sort, Sort::Activity);
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("activity"))
    );
    let run = run.send(perform(Action::TreeSort));
    assert_eq!(run.state().tree.organize.sort, Sort::Name);
    assert_eq!(&worktree_rows(&run)[..3], ["a", "b", "c"]);
}

#[test]
fn by_activity_the_worktree_written_to_last_is_first() {
    let run = repository().send(perform(Action::TreeSort));

    let rows = worktree_rows(&run);
    let at = |name: &str| rows.iter().position(|r| r == name).unwrap();
    assert!(at("a") < at("b"), "{rows:?}");
}

#[test]
fn hiding_idle_leaves_out_the_worktrees_where_nothing_runs_but_keeps_their_sessions_listed() {
    let run = repository().send(perform(Action::TreeHideIdle));

    let rows = worktree_rows(&run);
    assert!(rows.contains(&"a".to_string()), "{rows:?}");
    assert!(rows.contains(&"d".to_string()), "a terminal runs there");
    assert!(!rows.contains(&"b".to_string()), "{rows:?}");
    assert!(!rows.contains(&"c".to_string()), "{rows:?}");
    assert!(
        run.state()
            .tree
            .rows()
            .iter()
            .any(|r| matches!(r, Item::Gathered(g) if **g == Item::Session(here(), "sb".into()))),
        "the workspace still lists every session of all its worktrees"
    );
    assert!(kept(&run).unwrap().hide.idle);
}

#[test]
fn each_hide_filter_toggles_on_and_off_by_itself() {
    let mut run = repository();
    for (action, on) in [
        (
            Action::TreeHideSleeping,
            (|h: Hide| h.sleeping) as fn(Hide) -> bool,
        ),
        (Action::TreeHideIdle, |h: Hide| h.idle),
        (Action::TreeHideMain, |h: Hide| h.main),
    ] {
        run = run.send(perform(action.clone()));
        assert!(on(run.state().tree.organize.hide), "{action:?} on");
        run = run.send(perform(action.clone()));
        assert!(!on(run.state().tree.organize.hide), "{action:?} off");
    }
}

#[test]
fn what_was_kept_comes_back_without_undoing_what_was_arranged_since() {
    let mut earlier = Organize::default();
    earlier.toggle_pin(&here(), std::path::Path::new("/r/c"));
    earlier.sort = Sort::Name;
    let run = on(repository(), worktree_row("/r/a"), Action::WorkspacePin)
        .send(Msg::OrganizeRestored(earlier));

    let organize = &run.state().tree.organize;
    assert!(organize.is_pinned(&here(), std::path::Path::new("/r/c")));
    assert!(organize.is_pinned(&here(), std::path::Path::new("/r/a")));
    assert_eq!(organize.sort, Sort::Name);
}

#[test]
fn revealing_the_current_session_selects_its_row_unfolding_the_way_to_it() {
    let run = repository()
        .send(show(session_tab("sa")))
        .send(Msg::TreeSelect(None))
        .send(perform(Action::TreeRevealCurrent));

    assert_eq!(
        run.state().tree.selected,
        Some(Item::Session(here(), "sa".into()))
    );
    assert!(
        run.state()
            .tree
            .rows()
            .contains(&Item::Session(here(), "sa".into()))
    );
}

#[test]
fn revealing_a_worktree_a_filter_leaves_out_shows_it_again() {
    let run = repository()
        .send(perform(Action::TreeHideIdle))
        .send(show(session_tab("sb")))
        .send(perform(Action::TreeRevealCurrent));

    assert!(
        worktree_rows(&run).contains(&"b".to_string()),
        "{:?}",
        worktree_rows(&run)
    );
    assert_eq!(
        run.state().tree.selected,
        Some(Item::Session(here(), "sb".into()))
    );
}

#[test]
fn with_nothing_shown_there_is_nothing_to_reveal_and_it_says_so() {
    let run = repository().send(perform(Action::TreeRevealCurrent));

    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("Nothing"))
    );
}
