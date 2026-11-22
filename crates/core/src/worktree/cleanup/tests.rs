use tsumugi_model::{
    BranchFate, CleanupEntry, CleanupPull, Key, PullState, WorktreeRemoved, WorktreeStanding,
};

use super::Row;
use crate::actions::Action;
use crate::console::Probe;
use crate::content::Area;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::confirm::Ask;
use crate::scenario::Scenario;
use crate::testing::{here, listed, session, shell, snapshot, worktree};
use crate::tree::nav::Item;

/// The repository `/r` with worktrees `/r/a` (a session working in it),
/// `/r/b` and `/r/c`.
fn repository() -> Scenario {
    Scenario::new()
        .event(
            snapshot().listed(
                listed()
                    .session(session("a"))
                    .terminal(shell(7, "/r/b"))
                    .worktree(worktree("/r", "/r", "main"))
                    .worktree(worktree("/r/a", "/r", "a"))
                    .worktree(worktree("/r/b", "/r", "b"))
                    .worktree(worktree("/r/c", "/r", "c")),
            ),
        )
        .send(Msg::Focus(Area::Sidebar))
}

fn entry(path: &str, branch: &str) -> CleanupEntry {
    CleanupEntry {
        path: path.into(),
        standing: WorktreeStanding {
            branch: Some(branch.into()),
            ..WorktreeStanding::default()
        },
        bytes: 1024,
        merged: false,
        last_commit: Some(1_700_000_000),
        pull: None,
    }
}

fn merged_pull(mut e: CleanupEntry) -> CleanupEntry {
    e.pull = Some(CleanupPull {
        number: 3,
        state: PullState::Merged,
    });
    e
}

fn perform(action: Action) -> Msg {
    Msg::Perform {
        action,
        key: None,
        probe: Box::new(Probe::default()),
    }
}

fn opened() -> Scenario {
    repository()
        .send(Msg::TreeSelect(Some(Item::Workspace(here(), "/r".into()))))
        .send(perform(Action::WorktreeCleanup))
}

fn read(run: Scenario, entries: Vec<CleanupEntry>) -> Scenario {
    run.send(Msg::CleanupRead {
        environment: here(),
        repository: "/r".into(),
        result: Ok(entries),
    })
}

fn press(run: Scenario, key: &str) -> Scenario {
    run.send(Msg::CleanupKey(Key::new(key)))
}

fn selected(run: &Scenario) -> Vec<String> {
    run.state()
        .overlays
        .cleanup
        .as_ref()
        .unwrap()
        .rows()
        .iter()
        .filter(|r| r.selected)
        .map(|r| r.entry.path.display().to_string())
        .collect()
}

#[test]
fn the_action_opens_the_list_and_asks_the_host_for_the_worktrees() {
    let run = opened();

    assert_eq!(
        run.last_effects(),
        [Effect::CleanupScan {
            environment: here(),
            repository: "/r".into()
        }]
    );
    let cleanup = run.state().overlays.cleanup.clone().expect("open");
    assert_eq!(cleanup.repository, std::path::PathBuf::from("/r"));
    assert!(cleanup.rows().is_empty());
}

#[test]
fn a_row_outside_git_opens_nothing_and_says_so() {
    let run = Scenario::new()
        .event(snapshot().listed(listed().outside_git().kept("/notes")))
        .send(Msg::Focus(Area::Sidebar))
        .send(Msg::TreeSelect(Some(Item::Workspace(
            here(),
            "/notes".into(),
        ))))
        .send(perform(Action::WorktreeCleanup));

    assert!(run.state().overlays.cleanup.is_none());
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("not in a Git repository"))
    );
}

#[test]
fn worktrees_whose_pull_request_is_merged_or_closed_are_chosen_beforehand() {
    let closed = {
        let mut e = entry("/r/c", "c");
        e.pull = Some(CleanupPull {
            number: 5,
            state: PullState::Closed,
        });
        e
    };
    let mut open = entry("/r/b", "b");
    open.pull = Some(CleanupPull {
        number: 4,
        state: PullState::Open,
    });

    let run = read(
        opened(),
        vec![merged_pull(entry("/r/a", "a")), open, closed],
    );

    let mut chosen = selected(&run);
    chosen.sort();
    assert_eq!(chosen, ["/r/a", "/r/c"]);
}

#[test]
fn a_worktree_with_changes_or_unpushed_commits_is_never_chosen_beforehand() {
    let mut dirty = merged_pull(entry("/r/a", "a"));
    dirty.standing.changed = 2;
    let mut ahead = merged_pull(entry("/r/b", "b"));
    ahead.standing.unpushed = 1;
    let mut locked = merged_pull(entry("/r/c", "c"));
    locked.standing.locked = true;

    let run = read(opened(), vec![dirty, ahead, locked]);

    assert_eq!(selected(&run), Vec::<String>::new());
}

#[test]
fn the_main_checkout_is_not_listed_even_when_the_host_sends_it() {
    let mut main = entry("/r", "main");
    main.standing.main = true;

    let run = read(opened(), vec![main, entry("/r/a", "a")]);

    assert_eq!(
        run.state().overlays.cleanup.as_ref().unwrap().rows().len(),
        1
    );
}

#[test]
fn what_can_be_given_up_comes_first_and_the_longest_idle_before_the_rest() {
    let mut old = entry("/r/a", "a");
    old.last_commit = Some(100);
    let mut recent = entry("/r/b", "b");
    recent.last_commit = Some(900);
    let done = merged_pull(entry("/r/c", "c"));

    let run = read(opened(), vec![recent, old, done]);

    let paths: Vec<_> = run
        .state()
        .overlays
        .cleanup
        .as_ref()
        .unwrap()
        .rows()
        .iter()
        .map(|r| r.entry.path.display().to_string())
        .collect();
    assert_eq!(paths, ["/r/c", "/r/a", "/r/b"]);
}

#[test]
fn space_chooses_and_unchooses_the_row_under_the_cursor_and_the_keys_move() {
    let run = read(opened(), vec![entry("/r/a", "a"), entry("/r/c", "c")]);
    assert_eq!(selected(&run), Vec::<String>::new());

    let run = press(run, " ");
    assert_eq!(selected(&run), ["/r/a"]);
    let run = press(run, "ArrowDown");
    let run = press(run, " ");
    assert_eq!(selected(&run).len(), 2);
    let run = press(run, " ");
    assert_eq!(selected(&run), ["/r/a"]);
}

#[test]
fn a_locked_worktree_cannot_be_chosen() {
    let mut locked = entry("/r/a", "a");
    locked.standing.locked = true;

    let run = press(read(opened(), vec![locked]), " ");

    assert_eq!(selected(&run), Vec::<String>::new());
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("locked"))
    );
}

#[test]
fn a_chooses_every_worktree_with_nothing_to_lose_and_again_chooses_none() {
    let mut dirty = entry("/r/a", "a");
    dirty.standing.changed = 1;
    let run = read(opened(), vec![dirty, entry("/r/c", "c")]);

    let run = press(run, "a");
    assert_eq!(selected(&run), ["/r/c"]);
    let run = press(run, "a");
    assert_eq!(selected(&run), Vec::<String>::new());
}

#[test]
fn choosing_nothing_deletes_nothing() {
    let run = read(opened(), vec![entry("/r/c", "c")]);

    let run = press(run, "Enter");

    assert!(run.last_effects().is_empty());
    assert!(run.state().overlays.confirm.is_none());
    assert!(run.state().overlays.cleanup.is_some());
}

#[test]
fn escape_puts_the_list_away() {
    let run = press(read(opened(), vec![entry("/r/c", "c")]), "Escape");

    assert!(run.state().overlays.cleanup.is_none());
}

#[test]
fn worktrees_with_nothing_to_lose_are_deleted_without_a_question_keeping_branches_with_commits() {
    let run = read(opened(), vec![merged_pull(entry("/r/c", "c"))]);

    let run = press(run, "Enter");

    assert_eq!(
        run.last_effects(),
        [Effect::WorktreeDelete {
            environment: here(),
            path: "/r/c".into(),
            force: false,
            branch: BranchFate::Delete,
        }]
    );
    assert!(run.state().overlays.cleanup.is_none());
    assert!(run.state().overlays.confirm.is_none());
}

#[test]
fn changes_unpushed_commits_or_running_work_are_never_deleted_before_the_question_is_answered() {
    for (path, edit) in [
        (
            "/r/c",
            (|e: &mut CleanupEntry| e.standing.changed = 3) as fn(&mut _),
        ),
        ("/r/c", |e: &mut CleanupEntry| e.standing.unpushed = 2),
        ("/r/c", |e: &mut CleanupEntry| e.standing.branch = None),
        // /r/a has a session, /r/b a shell.
        ("/r/a", |_: &mut CleanupEntry| {}),
        ("/r/b", |_: &mut CleanupEntry| {}),
    ] {
        let mut risky = entry(path, "x");
        edit(&mut risky);
        let run = press(read(opened(), vec![risky]), " ");

        let run = press(run, "Enter");

        assert!(
            run.last_effects().is_empty(),
            "{path}: {:?}",
            run.last_effects()
        );
        let Some(Ask::CleanUpWorktrees { .. }) = run.state().overlays.confirm.clone() else {
            panic!("{path}: {:?}", run.state().overlays.confirm);
        };
    }
}

#[test]
fn the_question_names_each_worktree_and_what_it_loses() {
    let mut dirty = entry("/r/c", "c");
    dirty.standing.changed = 3;
    dirty.standing.unpushed = 1;
    let run = press(read(opened(), vec![dirty, entry("/r/a", "a")]), " ");
    let run = press(press(run, "ArrowDown"), " ");
    let run = press(run, "Enter");

    let ask = run.state().overlays.confirm.clone().expect("asked");
    let (_, title, body, action) = ask.dialog();

    assert_eq!(title, "Delete 2 worktrees?");
    assert!(
        body.contains("c: 3 files with changes not committed"),
        "{body}"
    );
    assert!(body.contains("1 commit not pushed"), "{body}");
    assert!(body.contains("a: 1 session"), "{body}");
    assert_eq!(action, "Delete and discard changes");
    assert!(ask.destroys());
}

#[test]
fn answering_yes_ends_what_runs_then_deletes_each_with_its_changes_only_where_there_are_some() {
    let mut dirty = entry("/r/c", "c");
    dirty.standing.changed = 3;
    let run = press(read(opened(), vec![dirty, entry("/r/b", "b")]), " ");
    let run = press(press(run, "ArrowDown"), " ");
    let run = press(run, "Enter").send(Msg::ConfirmAnswer(true));

    let effects = run.last_effects();

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::EndShell { id: 7, .. })),
        "{effects:?}"
    );
    let deletes: Vec<_> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::WorktreeDelete {
                path,
                force,
                branch,
                ..
            } => Some((path.display().to_string(), *force, *branch)),
            _ => None,
        })
        .collect();
    assert_eq!(
        deletes,
        [
            ("/r/c".to_string(), true, BranchFate::Delete),
            ("/r/b".to_string(), false, BranchFate::Delete)
        ]
    );
}

#[test]
fn answering_no_deletes_nothing() {
    let mut dirty = entry("/r/c", "c");
    dirty.standing.changed = 3;
    let run = press(read(opened(), vec![dirty]), " ");
    let run = press(run, "Enter").send(Msg::ConfirmAnswer(false));

    assert!(run.last_effects().is_empty());
}

#[test]
fn a_list_the_host_could_not_read_says_why_and_deletes_nothing() {
    let run = opened().send(Msg::CleanupRead {
        environment: here(),
        repository: "/r".into(),
        result: Err("git is gone".into()),
    });

    assert_eq!(
        run.state().overlays.cleanup.as_ref().unwrap().failed(),
        Some("git is gone")
    );
    assert!(press(run, "Enter").last_effects().is_empty());
}

#[test]
fn a_list_read_for_another_repository_is_ignored() {
    let run = opened().send(Msg::CleanupRead {
        environment: here(),
        repository: "/elsewhere".into(),
        result: Ok(vec![entry("/elsewhere/a", "a")]),
    });

    assert!(
        run.state()
            .overlays
            .cleanup
            .as_ref()
            .unwrap()
            .rows()
            .is_empty()
    );
}

#[test]
fn deleting_says_what_became_of_the_worktree_as_deleting_one_does() {
    let run = repository().send(Msg::WorktreeDeleted {
        path: "/r/c".into(),
        result: Ok(WorktreeRemoved::default()),
    });

    assert_eq!(run.state().chrome.status.line.as_deref(), Some("Deleted c"));
}

fn row(entry: CleanupEntry) -> Row {
    Row {
        entry,
        selected: false,
    }
}

#[test]
fn a_row_says_in_words_what_is_weighed_of_the_worktree() {
    let mut e = entry("/r/a", "a");
    e.standing.changed = 2;
    e.standing.unpushed = 1;
    e.merged = true;
    e.bytes = 5 * 1024 * 1024;
    e.last_commit = Some(1_000_000 - 3 * 86_400);
    e.pull = Some(CleanupPull {
        number: 9,
        state: PullState::Merged,
    });

    assert_eq!(
        row(e).facts(1_000_000),
        [
            "2 files changed",
            "1 commit not pushed",
            "merged into the base",
            "#9 merged",
            "5.0 MiB",
            "3d ago",
        ]
    );
}

#[test]
fn a_worktree_with_nothing_to_say_says_only_its_size_and_age() {
    let mut e = entry("/r/a", "a");
    e.bytes = 900;
    e.last_commit = None;

    assert_eq!(row(e).facts(1_000_000), ["900 B"]);
}

#[test]
fn a_detached_or_locked_worktree_says_so() {
    let mut e = entry("/r/a", "a");
    e.standing.branch = None;
    e.standing.locked = true;

    let facts = row(e).facts(1_000_000);

    assert!(facts.contains(&"no branch".to_string()), "{facts:?}");
    assert!(facts.contains(&"locked".to_string()), "{facts:?}");
}

#[test]
fn a_click_moves_the_cursor_and_the_box_chooses() {
    let run = read(opened(), vec![entry("/r/a", "a"), entry("/r/c", "c")]);

    let run = run.send(Msg::CleanupToggle(1));

    assert_eq!(selected(&run), ["/r/c"]);
    assert_eq!(run.state().overlays.cleanup.as_ref().unwrap().cursor, 1);
}
