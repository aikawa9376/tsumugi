use std::path::{Path, PathBuf};

use tsumugi_model::testing::{session, shell, tree, worktree};
use tsumugi_model::{EnvironmentId, Rest, SessionId, State, Tree};

use super::*;

fn here() -> EnvironmentId {
    EnvironmentId::unbound()
}

/// The repository `/r` with worktrees `/r/b`, `/r/a` and `/r/c` in that
/// order, `a` having a session started late, `b` one started early and `c`
/// nothing running.
fn repository() -> Tree {
    tree()
        .session(session("sb").cwd("/r/b").started_ms(100).state(State::Done))
        .session(
            session("sa")
                .cwd("/r/a")
                .started_ms(900)
                .state(State::Working),
        )
        .checkout("/r", "/r", "main")
        .checkout("/r/b", "/r", "b")
        .checkout("/r/a", "/r", "a")
        .listed(worktree("/r", "/r", "main"))
        .listed(worktree("/r/b", "/r", "b"))
        .listed(worktree("/r/a", "/r", "a"))
        .listed(worktree("/r/c", "/r", "c"))
        .build()
}

fn order(tree: &Tree) -> Vec<String> {
    tree.workspaces[0]
        .worktrees
        .iter()
        .map(|w| w.worktree.branch.clone().unwrap())
        .collect()
}

fn pin(organize: &mut Organize, path: &str) {
    organize.toggle_pin(&here(), Path::new(path));
}

#[test]
fn the_default_order_is_the_one_the_host_listed() {
    let before = order(&repository());

    let mut tree = repository();
    Organize::default().order(&here(), &mut tree);

    assert_eq!(order(&tree), before);
}

#[test]
fn activity_puts_the_worktree_acted_on_last_first_and_those_with_none_at_the_end() {
    let organize = Organize {
        sort: Sort::Activity,
        ..Organize::default()
    };
    let mut tree = repository();

    organize.order(&here(), &mut tree);

    let worked: Vec<_> = order(&tree);
    let at = |name: &str| worked.iter().position(|w| w == name).unwrap();
    assert!(at("a") < at("b"), "{worked:?}");
    assert!(at("b") < at("c"), "{worked:?}");
}

#[test]
fn a_session_written_to_counts_as_activity_over_when_it_started() {
    let organize = Organize {
        sort: Sort::Activity,
        ..Organize::default()
    };
    let mut tree = tree()
        .session(session("s1").cwd("/r/x").started_ms(900).state(State::Done))
        .session(
            session("s2")
                .cwd("/r/y")
                .started_ms(100)
                .last_input_ms(5_000)
                .state(State::Done),
        )
        .checkout("/r/x", "/r", "x")
        .checkout("/r/y", "/r", "y")
        .build();

    organize.order(&here(), &mut tree);

    assert_eq!(order(&tree), ["y", "x"]);
}

#[test]
fn name_sorts_by_branch_ignoring_case() {
    let organize = Organize {
        sort: Sort::Name,
        ..Organize::default()
    };
    let mut tree = tree()
        .session(session("m").cwd("/r"))
        .session(session("z").cwd("/r/z"))
        .session(session("b").cwd("/r/b"))
        .session(session("a").cwd("/r/a"))
        .checkout("/r", "/r", "main")
        .checkout("/r/z", "/r", "Zed")
        .checkout("/r/b", "/r", "beta")
        .checkout("/r/a", "/r", "alpha")
        .build();

    organize.order(&here(), &mut tree);

    assert_eq!(order(&tree), ["alpha", "beta", "main", "Zed"]);
}

#[test]
fn a_pinned_worktree_stays_at_the_top_of_its_workspace_whatever_the_sort() {
    let mut organize = Organize {
        sort: Sort::Activity,
        ..Organize::default()
    };
    pin(&mut organize, "/r/c");
    let mut tree = repository();

    organize.order(&here(), &mut tree);

    assert_eq!(order(&tree)[0], "c");
}

#[test]
fn pinned_worktrees_keep_the_order_the_sort_gives_among_themselves() {
    let mut organize = Organize {
        sort: Sort::Name,
        ..Organize::default()
    };
    pin(&mut organize, "/r/c");
    pin(&mut organize, "/r/b");
    let mut tree = repository();

    organize.order(&here(), &mut tree);

    assert_eq!(&order(&tree)[..2], ["b", "c"]);
}

#[test]
fn a_pinned_workspace_goes_above_the_others_and_scratch_stays_last() {
    let mut organize = Organize::default();
    pin(&mut organize, "/second");
    let mut tree = tree()
        .scratch("/scratch")
        .session(session("s").cwd("/scratch/a"))
        .session(session("f").cwd("/first"))
        .session(session("g").cwd("/second"))
        .checkout("/first", "/first", "main")
        .checkout("/second", "/second", "main")
        .build();

    organize.order(&here(), &mut tree);

    let roots: Vec<PathBuf> = tree.workspaces.iter().map(|w| w.root.clone()).collect();
    assert_eq!(roots[0], Path::new("/second"));
    assert!(tree.workspaces.last().unwrap().scratch, "{roots:?}");
}

#[test]
fn pinning_twice_unpins() {
    let mut organize = Organize::default();
    assert!(organize.toggle_pin(&here(), Path::new("/r/a")));
    assert!(organize.is_pinned(&here(), Path::new("/r/a")));
    assert!(!organize.toggle_pin(&here(), Path::new("/r/a")));
    assert!(!organize.is_pinned(&here(), Path::new("/r/a")));
}

#[test]
fn a_pin_belongs_to_its_environment() {
    let mut organize = Organize::default();
    organize.toggle_pin(&here(), Path::new("/r/a"));

    assert!(!organize.is_pinned(&EnvironmentId::new("there"), Path::new("/r/a")));
}

fn hidden(organize: &Organize, tree: &Tree) -> Vec<String> {
    let hidden = organize.hidden(&here(), tree);
    let mut names: Vec<String> = tree.workspaces[0]
        .worktrees
        .iter()
        .filter(|w| hidden.contains(&w.worktree.path))
        .map(|w| w.worktree.branch.clone().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn nothing_is_hidden_by_default() {
    assert_eq!(
        hidden(&Organize::default(), &repository()),
        Vec::<String>::new()
    );
}

#[test]
fn idle_hides_worktrees_where_nothing_works_waits_or_starts() {
    let organize = Organize {
        hide: Hide {
            idle: true,
            ..Hide::default()
        },
        ..Organize::default()
    };

    // `b`'s session is done, `c` and the main checkout have none; `a`'s works.
    assert_eq!(hidden(&organize, &repository()), ["b", "c", "main"]);
}

#[test]
fn a_worktree_with_a_terminal_running_is_not_idle() {
    let organize = Organize {
        hide: Hide {
            idle: true,
            ..Hide::default()
        },
        ..Organize::default()
    };
    let busy = tree()
        .terminal(shell(7, "/r/b"))
        .checkout("/r/b", "/r", "b")
        .listed(worktree("/r", "/r", "main"))
        .build();

    assert_eq!(hidden(&organize, &busy), ["main"]);
}

#[test]
fn sleeping_hides_the_worktrees_whose_sessions_all_sleep() {
    let organize = Organize {
        hide: Hide {
            sleeping: true,
            ..Hide::default()
        },
        ..Organize::default()
    };
    let asleep = tree()
        .session(
            session("s")
                .cwd("/r/b")
                .state(State::Idle)
                .rest(Rest::Hibernated)
                .session_id("c")
                .delegation(SessionId::new()),
        )
        .session(session("t").cwd("/r/a").state(State::Done))
        .checkout("/r/b", "/r", "b")
        .checkout("/r/a", "/r", "a")
        .listed(worktree("/r", "/r", "main"))
        .build();

    assert_eq!(hidden(&organize, &asleep), ["b"]);
}

#[test]
fn main_hides_the_main_checkout_where_the_workspace_lists_other_worktrees() {
    let organize = Organize {
        hide: Hide {
            main: true,
            ..Hide::default()
        },
        ..Organize::default()
    };

    assert_eq!(hidden(&organize, &repository()), ["main"]);

    let alone = tree().listed(worktree("/r", "/r", "main")).build();
    assert!(
        organize.hidden(&here(), &alone).is_empty(),
        "a workspace of its main checkout alone has nothing else to show"
    );
}

#[test]
fn a_pinned_worktree_is_never_hidden() {
    let mut organize = Organize {
        hide: Hide {
            idle: true,
            ..Hide::default()
        },
        ..Organize::default()
    };
    pin(&mut organize, "/r/c");

    assert_eq!(hidden(&organize, &repository()), ["b", "main"]);
}

#[test]
fn what_is_kept_is_read_again_as_written() {
    let mut organize = Organize::default();
    pin(&mut organize, "/r/a");
    organize.sort = Sort::Name;
    organize.hide.sleeping = true;
    organize.hide.main = true;

    let again = Organize::from_json(organize.to_json().as_bytes()).unwrap();

    assert_eq!(again, organize);
}

#[test]
fn what_an_earlier_version_wrote_is_read_with_the_rest_as_defaults() {
    let again = Organize::from_json(br#"{"pinned":{}}"#).unwrap();

    assert_eq!(again.pinned_count(), 0);
    assert_eq!(again.sort, Sort::Default);
    assert_eq!(again.hide, Hide::default());
}

#[test]
fn the_sort_cycles_through_all_three_and_back() {
    assert_eq!(Sort::Default.next(), Sort::Activity);
    assert_eq!(Sort::Activity.next(), Sort::Name);
    assert_eq!(Sort::Name.next(), Sort::Default);
}
