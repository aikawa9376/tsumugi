use tsumugi_model::{
    BranchFate, KeptBranch, Key, NewWorktree, StartChoice, StartPoint, WorktreeRemoved,
    WorktreeStanding,
};

use super::*;
use crate::actions::Action;
use crate::console::Probe;
use crate::content::Area;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::confirm::Ask;
use crate::scenario::Scenario;
use crate::testing::{here, listed, session, shell, snapshot, worktree};
use crate::tree::nav::Item;

/// The repository `/r`, its main checkout and a worktree `/r/a` with the
/// session `a` and the shell 7 running in it.
fn repository() -> Scenario {
    Scenario::new()
        .event(
            snapshot().listed(
                listed()
                    .session(session("a"))
                    .terminal(shell(7, "/r/a"))
                    .worktree(worktree("/r", "/r", "main"))
                    .worktree(worktree("/r/a", "/r", "a")),
            ),
        )
        .send(Msg::Focus(Area::Sidebar))
}

fn workspace() -> Item {
    Item::Workspace(here(), "/r".into())
}

fn perform(action: Action) -> Msg {
    Msg::Perform {
        action,
        key: None,
        probe: Box::new(Probe::default()),
    }
}

fn on(row: Item, action: Action) -> impl Fn(Scenario) -> Scenario {
    move |run| {
        run.send(Msg::TreeSelect(Some(row.clone())))
            .send(perform(action.clone()))
    }
}

fn choice(point: StartPoint, subject: &str) -> StartChoice {
    StartChoice {
        point,
        subject: subject.into(),
    }
}

fn local(branch: &str) -> StartPoint {
    StartPoint::Local {
        branch: branch.into(),
    }
}

fn remote(branch: &str) -> StartPoint {
    StartPoint::Remote {
        remote: "origin".into(),
        branch: branch.into(),
    }
}

/// The dialog open on the workspace, the host having listed these.
fn composing(starts: Vec<StartChoice>) -> Scenario {
    on(workspace(), Action::WorktreeNew)(repository()).send(Msg::WorktreeStarts {
        environment: here(),
        repository: "/r".into(),
        result: Ok(starts),
    })
}

fn typed(run: Scenario, field: Field, text: &str) -> Scenario {
    run.send(Msg::ComposerText {
        field,
        text: text.into(),
    })
}

fn pressed(run: Scenario, key: &str) -> Scenario {
    run.send(Msg::ComposerKey(Key::new(key)))
}

fn composer(run: &Scenario) -> Composer {
    run.state()
        .overlays
        .composer
        .clone()
        .expect("the dialog is open")
}

#[test]
fn new_worktree_opens_the_dialog_on_the_repository_and_asks_where_it_may_start() {
    let run = on(workspace(), Action::WorktreeNew)(repository());

    let open = composer(&run);
    assert_eq!(open.repository, std::path::PathBuf::from("/r"));
    assert_eq!(open.field, Field::Name);
    assert_eq!(open.starts, Starts::Reading);
    assert_eq!(
        run.last_effects(),
        [Effect::WorktreeStarts {
            environment: here(),
            repository: "/r".into()
        }]
    );
}

#[test]
fn new_worktree_works_from_a_worktree_row_too_and_makes_it_of_the_same_repository() {
    let run = on(Item::Worktree(here(), "/r/a".into()), Action::WorktreeNew)(repository());

    assert_eq!(composer(&run).repository, std::path::PathBuf::from("/r"));
}

#[test]
fn new_worktree_outside_a_repository_says_so_and_opens_nothing() {
    let run = Scenario::new()
        .event(snapshot().listed(listed().outside_git().kept("/notes")))
        .send(Msg::Focus(Area::Sidebar));
    let run = on(
        Item::Workspace(here(), "/notes".into()),
        Action::WorktreeNew,
    )(run);

    assert_eq!(run.state().overlays.composer, None);
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
fn the_places_to_start_are_the_checkouts_commit_then_the_branches_narrowed_as_typed() {
    let run = composing(vec![
        choice(StartPoint::Head, "tip"),
        choice(local("main"), "tip"),
        choice(local("feature/login"), "add login"),
        choice(remote("main"), "tip"),
    ]);
    let all: Vec<_> = composer(&run).rows().into_iter().map(|c| c.point).collect();
    assert_eq!(
        all,
        [
            StartPoint::Head,
            local("main"),
            local("feature/login"),
            remote("main")
        ]
    );

    let narrowed = typed(run, Field::Start, "login");
    let rows: Vec<_> = composer(&narrowed)
        .rows()
        .into_iter()
        .map(|c| c.point)
        .collect();
    assert_eq!(rows[0], local("feature/login"));
}

#[test]
fn what_is_typed_that_no_branch_names_can_be_chosen_as_a_revision() {
    let run = typed(
        composing(vec![choice(StartPoint::Head, "")]),
        Field::Start,
        "9fceb02",
    );

    let rows: Vec<_> = composer(&run).rows().into_iter().map(|c| c.point).collect();

    assert_eq!(
        rows.last(),
        Some(&StartPoint::Rev {
            rev: "9fceb02".into()
        })
    );
}

#[test]
fn the_checkouts_commit_can_be_chosen_before_the_host_lists_anything_or_when_it_cannot() {
    let reading = on(workspace(), Action::WorktreeNew)(repository());
    assert_eq!(composer(&reading).chosen(), StartPoint::Head);

    let failed = reading.send(Msg::WorktreeStarts {
        environment: here(),
        repository: "/r".into(),
        result: Err("git failed".into()),
    });
    assert_eq!(
        composer(&failed).starts,
        Starts::Failed("git failed".into())
    );
    assert_eq!(composer(&failed).chosen(), StartPoint::Head);
}

#[test]
fn tab_goes_from_field_to_field_and_shift_tab_back() {
    let run = composing(vec![]);
    let next = pressed(run, "Tab");
    assert_eq!(composer(&next).field, Field::Start);
    let next = pressed(next, "Tab");
    assert_eq!(composer(&next).field, Field::Branch);
    let next = pressed(next, "Tab");
    assert_eq!(composer(&next).field, Field::Name);
    let back = next.send(Msg::ComposerKey(Key {
        shift: true,
        ..Key::new("Tab")
    }));
    assert_eq!(composer(&back).field, Field::Branch);
}

#[test]
fn escape_puts_the_dialog_away_and_asks_nothing() {
    let run = pressed(typed(composing(vec![]), Field::Name, "x"), "Escape");

    assert_eq!(run.state().overlays.composer, None);
    assert!(run.state().tree.making.is_empty());
}

#[test]
fn enter_without_a_name_keeps_the_dialog_and_asks_for_one() {
    let run = pressed(composing(vec![]), "Enter");

    assert!(run.state().overlays.composer.is_some());
    assert!(run.state().tree.making.is_empty());
    assert_eq!(composer(&run).field, Field::Name);
}

#[test]
fn the_keys_that_move_through_a_list_choose_the_place_to_start_from() {
    let run = composing(vec![
        choice(StartPoint::Head, ""),
        choice(local("main"), ""),
        choice(local("topic"), ""),
    ]);
    let run = pressed(pressed(run, "ArrowDown"), "ArrowDown");

    assert_eq!(composer(&run).chosen(), local("topic"));
    let run = pressed(run, "ArrowUp");
    assert_eq!(composer(&run).chosen(), local("main"));
}

#[test]
fn clicking_a_place_in_the_list_chooses_it_and_a_row_that_is_not_there_chooses_nothing() {
    let run = composing(vec![
        choice(StartPoint::Head, ""),
        choice(local("main"), ""),
    ]);

    let run = run.send(Msg::ComposerCursor(1));
    assert_eq!(composer(&run).chosen(), local("main"));
    assert_eq!(composer(&run).field, Field::Start);
    let run = run.send(Msg::ComposerCursor(9));
    assert_eq!(composer(&run).chosen(), local("main"));
}

fn created(run: &Scenario) -> (u64, NewWorktree) {
    match run.last_effects() {
        [Effect::WorktreeCreate { id, new, .. }] => (*id, new.clone()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn enter_closes_the_dialog_and_makes_the_worktree_in_the_background_as_asked() {
    let run = composing(vec![
        choice(StartPoint::Head, ""),
        choice(remote("main"), "tip"),
    ]);
    let run = typed(run, Field::Name, "  Fix login ");
    let run = typed(run, Field::Branch, "feat/login");
    let run = pressed(pressed(run, "ArrowDown"), "Enter");

    assert_eq!(run.state().overlays.composer, None);
    let (id, new) = created(&run);
    assert_eq!(
        new,
        NewWorktree {
            name: "Fix login".into(),
            start: remote("main"),
            branch: Some("feat/login".into()),
        }
    );
    // Its row is listed under its workspace, running its first step.
    let pending = &run.state().tree.making;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, id);
    assert_eq!(
        pending[0].steps(),
        [
            (Step::Create, Progress::Running),
            (Step::Start, Progress::Waiting)
        ]
    );
    let rows = run.state().tree.rows();
    let at = rows.iter().position(|r| *r == workspace()).unwrap();
    assert_eq!(rows[at + 1], pending[0].row());
}

#[test]
fn a_branch_left_blank_is_left_to_the_host_to_name() {
    let run = typed(composing(vec![]), Field::Name, "review");
    let run = pressed(run, "Enter");

    assert_eq!(created(&run).1.branch, None);
}

#[test]
fn once_the_host_made_it_a_session_s_draft_opens_in_it_and_the_row_goes() {
    let run = typed(composing(vec![]), Field::Name, "review");
    let run = pressed(run, "Enter");
    let (id, _) = created(&run);

    let run = run.send(Msg::WorktreeMade {
        id,
        result: Ok("/r-wt/review".into()),
    });

    assert!(run.state().tree.making.is_empty());
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert!(matches!(
        run.state().content.active(),
        Some(crate::content::Tab::Draft { worktree, .. }) if worktree == std::path::Path::new("/r-wt/review")
    ));
}

#[test]
fn a_worktree_that_could_not_be_made_keeps_its_row_with_why_until_it_is_put_away() {
    let run = typed(composing(vec![]), Field::Name, "review");
    let run = pressed(run, "Enter");
    let (id, _) = created(&run);

    let run = run.send(Msg::WorktreeMade {
        id,
        result: Err("the branch review already exists".into()),
    });

    let pending = &run.state().tree.making;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].steps()[0], (Step::Create, Progress::Failed));
    assert_eq!(pending[0].steps()[1], (Step::Start, Progress::Waiting));
    let told = run.state().chrome.status.line.clone().unwrap();
    assert!(told.contains("cannot make review"), "{told}");
    assert!(told.contains("already exists"), "{told}");
    // No session started; nothing else was asked.
    assert!(
        run.last_effects()
            .iter()
            .all(|e| matches!(e, Effect::LapseNotice { .. }))
    );

    let run = run.send(Msg::WorktreeDismiss(id));
    assert!(run.state().tree.making.is_empty());
}

#[test]
fn a_row_still_running_is_not_put_away() {
    let run = typed(composing(vec![]), Field::Name, "review");
    let run = pressed(run, "Enter");
    let (id, _) = created(&run);

    let run = run.send(Msg::WorktreeDismiss(id));

    assert_eq!(run.state().tree.making.len(), 1);
}

#[test]
fn the_pipeline_runs_the_creating_step_first_and_starting_a_session_last() {
    assert_eq!(PIPELINE.first(), Some(&Step::Create));
    assert_eq!(PIPELINE.last(), Some(&Step::Start));
}

fn standing() -> WorktreeStanding {
    WorktreeStanding {
        branch: Some("a".into()),
        ..WorktreeStanding::default()
    }
}

fn inspected(run: Scenario, standing: WorktreeStanding) -> Scenario {
    run.send(Msg::WorktreeInspected {
        environment: here(),
        path: "/r/a".into(),
        result: Ok(standing),
    })
}

fn deleting() -> Scenario {
    on(
        Item::Worktree(here(), "/r/a".into()),
        Action::WorktreeDelete,
    )(repository())
}

#[test]
fn delete_asks_the_host_how_the_worktree_stands_and_deletes_nothing_yet() {
    let run = deleting();

    assert_eq!(
        run.last_effects(),
        [Effect::WorktreeInspect {
            environment: here(),
            path: "/r/a".into()
        }]
    );
    assert_eq!(run.state().overlays.confirm, None);
}

#[test]
fn the_main_checkout_is_not_offered_for_deleting() {
    let run = on(Item::Worktree(here(), "/r".into()), Action::WorktreeDelete)(repository());

    assert!(
        run.last_effects()
            .iter()
            .all(|e| !matches!(e, Effect::WorktreeInspect { .. }))
    );
    assert!(
        run.state()
            .chrome
            .status
            .line
            .as_deref()
            .is_some_and(|l| l.contains("main checkout"))
    );
}

#[test]
fn a_clean_worktree_asks_plainly_and_deletes_with_its_merged_branch() {
    let run = inspected(deleting(), standing());

    let Some(Ask::DeleteWorktree { delete_branch, .. }) = run.state().overlays.confirm.clone()
    else {
        panic!("{:?}", run.state().overlays.confirm);
    };
    assert!(delete_branch);
    let (_, title, _, action) = run.state().overlays.confirm.as_ref().unwrap().dialog();
    assert_eq!(title, "Delete a?");
    assert_eq!(action, "Delete");

    let answered = run.send(Msg::ConfirmAnswer(true));
    let effects = answered.last_effects();
    assert!(
        matches!(
            effects,
            [
                Effect::Host {
                    request: tsumugi_protocol::Request::Stop { .. },
                    ..
                },
                Effect::EndShell { id: 7, .. },
                Effect::WorktreeDelete {
                    force: false,
                    branch: BranchFate::Delete,
                    ..
                },
            ]
        ),
        "{effects:?}"
    );
}

#[test]
fn a_dirty_worktree_is_deleted_only_after_the_question_says_what_is_lost() {
    let run = inspected(
        deleting(),
        WorktreeStanding {
            changed: 3,
            unpushed: 2,
            ..standing()
        },
    );

    let ask = run.state().overlays.confirm.clone().expect("asked");
    let (_, _, body, action) = ask.dialog();
    assert!(
        body.contains("3 files with changes not committed will be lost"),
        "{body}"
    );
    assert!(body.contains("2 commits not pushed"), "{body}");
    assert_eq!(action, "Delete and discard changes");
    assert!(ask.destroys());
    // Nothing goes until yes: no is the way out.
    let answered = run.send(Msg::ConfirmAnswer(true));
    assert!(
        answered
            .last_effects()
            .iter()
            .any(|e| matches!(e, Effect::WorktreeDelete { force: true, .. }))
    );
}

#[test]
fn answering_no_deletes_nothing() {
    let run = inspected(
        deleting(),
        WorktreeStanding {
            changed: 1,
            ..standing()
        },
    );

    let declined = run.send(Msg::ConfirmAnswer(false));

    assert!(declined.last_effects().is_empty());
    assert_eq!(declined.state().overlays.confirm, None);
}

#[test]
fn a_branch_with_commits_nothing_else_has_is_kept_unless_the_person_chooses_otherwise() {
    let run = inspected(
        deleting(),
        WorktreeStanding {
            unmerged: 2,
            ..standing()
        },
    );
    let ask = run.state().overlays.confirm.clone().unwrap();
    let (label, chosen) = ask.option().expect("the branch is a choice");
    assert!(label.contains("losing 2 commits"), "{label}");
    assert!(!chosen);

    let kept = run.send(Msg::ConfirmAnswer(true));
    assert!(kept.last_effects().iter().any(|e| matches!(
        e,
        Effect::WorktreeDelete {
            branch: BranchFate::Keep,
            ..
        }
    )));
}

#[test]
fn the_branch_is_deleted_with_its_unmerged_commits_when_the_choice_is_flipped() {
    let run = inspected(
        deleting(),
        WorktreeStanding {
            unmerged: 2,
            ..standing()
        },
    );

    let flipped = run.send(Msg::ConfirmToggle);
    assert!(
        flipped
            .state()
            .overlays
            .confirm
            .as_ref()
            .unwrap()
            .option()
            .unwrap()
            .1
    );
    assert!(
        flipped
            .state()
            .overlays
            .confirm
            .as_ref()
            .unwrap()
            .destroys()
    );
    let answered = flipped.send(Msg::ConfirmAnswer(true));
    assert!(answered.last_effects().iter().any(|e| matches!(
        e,
        Effect::WorktreeDelete {
            branch: BranchFate::DeleteUnmerged,
            ..
        }
    )));
}

#[test]
fn the_letter_b_flips_the_choice_while_the_question_holds_the_keys() {
    let run = inspected(
        deleting(),
        WorktreeStanding {
            unmerged: 1,
            ..standing()
        },
    );

    let run = run.send(Msg::Key {
        key: Key::new("b"),
        probe: Box::new(Probe::default()),
    });

    assert!(
        run.state()
            .overlays
            .confirm
            .as_ref()
            .unwrap()
            .option()
            .unwrap()
            .1
    );
}

#[test]
fn a_detached_worktree_has_no_branch_to_choose_about() {
    let run = inspected(
        deleting(),
        WorktreeStanding {
            branch: None,
            ..standing()
        },
    );

    assert_eq!(
        run.state().overlays.confirm.as_ref().unwrap().option(),
        None
    );
    let run = run.send(Msg::ConfirmToggle).send(Msg::ConfirmAnswer(true));
    assert!(run.last_effects().iter().any(|e| matches!(
        e,
        Effect::WorktreeDelete {
            branch: BranchFate::Keep,
            ..
        }
    )));
}

#[test]
fn a_locked_worktree_and_a_host_that_cannot_say_ask_nothing_and_say_why() {
    let locked = inspected(
        deleting(),
        WorktreeStanding {
            locked: true,
            ..standing()
        },
    );
    assert_eq!(locked.state().overlays.confirm, None);
    assert!(
        locked
            .state()
            .chrome
            .status
            .line
            .as_deref()
            .unwrap()
            .contains("locked")
    );

    let failed = deleting().send(Msg::WorktreeInspected {
        environment: here(),
        path: "/r/a".into(),
        result: Err("not a worktree".into()),
    });
    assert_eq!(failed.state().overlays.confirm, None);
    assert!(
        failed
            .state()
            .chrome
            .status
            .line
            .as_deref()
            .unwrap()
            .contains("not a worktree")
    );
}

#[test]
fn what_became_of_the_deletion_is_said_with_the_branch_git_left() {
    let said = |result| {
        deleting()
            .send(Msg::WorktreeDeleted {
                path: "/r/a".into(),
                result,
            })
            .state()
            .chrome
            .status
            .line
            .clone()
            .unwrap()
    };
    assert_eq!(said(Ok(WorktreeRemoved::default())), "Deleted a");
    let kept = said(Ok(WorktreeRemoved {
        kept: Some(KeptBranch {
            name: "a".into(),
            why: "not fully merged".into(),
        }),
    }));
    assert!(
        kept.contains("kept the branch a: not fully merged"),
        "{kept}"
    );
    assert!(said(Err("locked".into())).contains("cannot delete a: locked"));
}
