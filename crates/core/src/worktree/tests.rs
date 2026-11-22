use tsumugi_model::{
    BranchFate, KeptBranch, Key, LinkedIssue, MainChanges, MergeStanding, Merged, NewWorktree,
    SetupChoice, SetupOutcome, StartChoice, StartPoint, WorktreeRemoved, WorktreeSetup,
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
            (Step::Setup, Progress::Waiting),
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

/// A worktree being made, which the host made at `/r-wt/review`.
fn made() -> (Scenario, u64) {
    let run = typed(composing(vec![]), Field::Name, "review");
    let run = pressed(run, "Enter");
    let (id, _) = created(&run);
    (
        run.send(Msg::WorktreeMade {
            id,
            result: Ok("/r-wt/review".into()),
        }),
        id,
    )
}

fn set_up(run: Scenario, id: u64, setup: WorktreeSetup) -> Scenario {
    run.send(Msg::WorktreeSetUp {
        id,
        result: Ok(setup),
    })
}

fn draft_is_open(run: &Scenario) -> bool {
    run.state().tree.making.is_empty()
        && run.state().content.focus == Area::Prompt
        && matches!(
            run.state().content.active(),
            Some(crate::content::Tab::Draft { worktree, .. }) if worktree == std::path::Path::new("/r-wt/review")
        )
}

#[test]
fn once_the_host_made_it_the_host_is_asked_to_set_it_up_as_its_policy_says() {
    let (run, id) = made();

    assert_eq!(
        run.last_effects(),
        [Effect::WorktreeSetup {
            environment: here(),
            repository: "/r".into(),
            worktree: "/r-wt/review".into(),
            id,
            choice: SetupChoice::Inherit,
        }]
    );
    assert_eq!(
        run.state().tree.making[0].steps()[1],
        (Step::Setup, Progress::Running)
    );
}

#[test]
fn once_it_is_set_up_a_session_s_draft_opens_in_it_and_the_row_goes() {
    let (run, id) = made();

    let run = set_up(run, id, WorktreeSetup::default());

    assert!(draft_is_open(&run));
}

#[test]
fn a_script_that_ran_does_not_keep_the_draft_from_opening() {
    let (run, id) = made();

    let run = set_up(
        run,
        id,
        WorktreeSetup {
            outcome: SetupOutcome::Ran,
            ..WorktreeSetup::default()
        },
    );

    assert!(draft_is_open(&run));
}

#[test]
fn what_could_not_be_given_to_the_worktree_is_told_and_the_draft_still_opens() {
    let (run, id) = made();

    let run = set_up(
        run,
        id,
        WorktreeSetup {
            warnings: vec![".env is not ignored by Git, so it is not given".into()],
            outcome: SetupOutcome::Nothing,
        },
    );

    assert!(draft_is_open(&run));
    let told = run.state().chrome.status.line.clone().unwrap();
    assert!(told.contains(".env is not ignored"), "{told}");
}

fn asked(script: &str) -> WorktreeSetup {
    WorktreeSetup {
        warnings: Vec::new(),
        outcome: SetupOutcome::Asked {
            script: script.into(),
        },
    }
}

#[test]
fn a_script_the_user_has_not_decided_on_is_shown_in_a_question_and_nothing_goes_on_until_it_is_answered()
 {
    let (run, id) = made();

    let run = set_up(run, id, asked("pnpm install"));

    let Some(Ask::SetupWorktree { script, name, .. }) = run.state().overlays.confirm.clone() else {
        panic!("{:?}", run.state().overlays.confirm);
    };
    assert_eq!(script, "pnpm install");
    assert_eq!(name, "review");
    assert!(run.last_effects().is_empty());
    assert_eq!(run.state().tree.making.len(), 1);
    let (_, _, body, action) = run.state().overlays.confirm.as_ref().unwrap().dialog();
    assert!(body.contains("pnpm install"), "{body}");
    assert_eq!(action, "Run setup");
}

#[test]
fn answering_yes_runs_the_script_and_no_leaves_it_unrun_either_way_the_worktree_goes_on() {
    for (yes, choice) in [(true, SetupChoice::Run), (false, SetupChoice::Skip)] {
        let (run, id) = made();
        let run = set_up(run, id, asked("make")).send(Msg::ConfirmAnswer(yes));

        assert_eq!(
            run.last_effects(),
            [Effect::WorktreeSetup {
                environment: here(),
                repository: "/r".into(),
                worktree: "/r-wt/review".into(),
                id,
                choice,
            }]
        );
        assert_eq!(run.state().overlays.confirm, None);
        // The answer, not the question, moves the worktree on.
        let run = set_up(run, id, WorktreeSetup::default());
        assert!(draft_is_open(&run));
    }
}

#[test]
fn a_setup_that_fails_keeps_the_worktree_and_its_row_with_why_and_starts_no_session() {
    let (run, id) = made();

    let run = run.send(Msg::WorktreeSetUp {
        id,
        result: Err("the setup script ended with status 3".into()),
    });

    let pending = &run.state().tree.making[0];
    assert_eq!(pending.steps()[1], (Step::Setup, Progress::Failed));
    assert_eq!(pending.steps()[2], (Step::Start, Progress::Waiting));
    // Where the worktree is stays known, for the row to say.
    assert_eq!(
        pending.made.as_deref(),
        Some(std::path::Path::new("/r-wt/review"))
    );
    let told = run.state().chrome.status.line.clone().unwrap();
    assert!(told.contains("status 3"), "{told}");
    assert!(
        run.last_effects()
            .iter()
            .all(|e| matches!(e, Effect::LapseNotice { .. }))
    );
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
    assert_eq!(pending[0].steps()[1], (Step::Setup, Progress::Waiting));
    assert_eq!(pending[0].steps()[2], (Step::Start, Progress::Waiting));
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
fn the_pipeline_runs_the_creating_step_first_the_setup_before_starting_a_session_last() {
    assert_eq!(PIPELINE, [Step::Create, Step::Setup, Step::Start]);
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

fn mergeable() -> MergeStanding {
    MergeStanding {
        branch: Some("a".into()),
        base: Some("main".into()),
        ahead: 2,
        ..MergeStanding::default()
    }
}

fn merging() -> Scenario {
    on(Item::Worktree(here(), "/r/a".into()), Action::WorktreeMerge)(repository())
}

fn inspected_for_merge(run: Scenario, standing: MergeStanding) -> Scenario {
    run.send(Msg::WorktreeMergeInspected {
        environment: here(),
        path: "/r/a".into(),
        result: Ok(standing),
    })
}

#[test]
fn merging_asks_the_host_how_it_stands_and_merges_nothing_yet() {
    let run = merging();

    assert_eq!(
        run.last_effects(),
        [Effect::WorktreeMergeInspect {
            environment: here(),
            path: "/r/a".into()
        }]
    );
    assert_eq!(run.state().overlays.confirm, None);
}

#[test]
fn the_main_checkout_is_not_offered_for_merging() {
    let run = on(Item::Worktree(here(), "/r".into()), Action::WorktreeMerge)(repository());

    assert!(
        run.last_effects()
            .iter()
            .all(|e| !matches!(e, Effect::WorktreeMergeInspect { .. }))
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
fn a_clean_main_checkout_is_asked_plainly_and_nothing_is_set_aside() {
    let run = inspected_for_merge(merging(), mergeable());

    let ask = run.state().overlays.confirm.clone().expect("asked");
    let (_, title, body, action) = ask.dialog();
    assert_eq!(title, "Merge a into main?");
    assert!(body.contains("2 commits"), "{body}");
    assert_eq!(action, "Merge");
    assert_eq!(ask.option(), None);

    let answered = run.send(Msg::ConfirmAnswer(true));
    assert_eq!(
        answered.last_effects(),
        [Effect::WorktreeMerge {
            environment: here(),
            path: "/r/a".into(),
            changes: MainChanges::Refuse,
            close_issue: false,
        }]
    );
}

#[test]
fn changes_in_the_main_checkout_are_stashed_unless_the_person_chooses_to_commit_them() {
    let dirty = || {
        inspected_for_merge(
            merging(),
            MergeStanding {
                main_changed: 3,
                ..mergeable()
            },
        )
    };
    let run = dirty();
    let ask = run.state().overlays.confirm.clone().expect("asked");
    let (_, _, body, action) = ask.dialog();
    assert!(body.contains("3 files with uncommitted changes"), "{body}");
    assert!(body.contains("stash"), "{body}");
    assert_eq!(action, "Stash and merge");
    let (label, chosen) = ask.option().expect("stash or commit is a choice");
    assert!(label.contains("Commit"), "{label}");
    assert!(!chosen);

    let stashed = dirty().send(Msg::ConfirmAnswer(true));
    assert!(matches!(
        stashed.last_effects(),
        [Effect::WorktreeMerge {
            changes: MainChanges::Stash,
            ..
        }]
    ));

    let flipped = run.send(Msg::ConfirmToggle);
    let ask = flipped.state().overlays.confirm.clone().unwrap();
    assert_eq!(ask.dialog().3, "Commit and merge");
    assert!(matches!(
        flipped.send(Msg::ConfirmAnswer(true)).last_effects(),
        [Effect::WorktreeMerge {
            changes: MainChanges::Commit,
            ..
        }]
    ));
}

#[test]
fn the_question_names_the_issue_the_merge_closes_and_the_merge_closes_it() {
    let run = inspected_for_merge(
        merging(),
        MergeStanding {
            issue: Some(LinkedIssue {
                number: 44,
                title: "Integrate".into(),
            }),
            changed: 2,
            ..mergeable()
        },
    );
    let (_, _, body, _) = run.state().overlays.confirm.as_ref().unwrap().dialog();
    assert!(body.contains("#44"), "{body}");
    assert!(
        body.contains("2 files with uncommitted changes in the worktree"),
        "{body}"
    );

    let answered = run.send(Msg::ConfirmAnswer(true));
    assert!(matches!(
        answered.last_effects(),
        [Effect::WorktreeMerge {
            close_issue: true,
            ..
        }]
    ));
}

#[test]
fn answering_no_merges_nothing() {
    let run = inspected_for_merge(merging(), mergeable()).send(Msg::ConfirmAnswer(false));

    assert!(run.last_effects().is_empty());
    assert_eq!(run.state().overlays.confirm, None);
}

#[test]
fn what_cannot_be_merged_is_told_instead_of_asked() {
    let told = |standing: MergeStanding| {
        let run = inspected_for_merge(merging(), standing);
        assert_eq!(run.state().overlays.confirm, None);
        run.state().chrome.status.line.clone().unwrap()
    };
    assert!(
        told(MergeStanding {
            ahead: 0,
            ..mergeable()
        })
        .contains("lacks")
    );
    assert!(
        told(MergeStanding {
            main_operation: Some(tsumugi_model::repository::Operation::Merge),
            ..mergeable()
        })
        .contains("merge is in progress")
    );
    let failed = merging().send(Msg::WorktreeMergeInspected {
        environment: here(),
        path: "/r/a".into(),
        result: Err("not a repository".into()),
    });
    assert!(
        failed
            .state()
            .chrome
            .status
            .line
            .as_deref()
            .unwrap()
            .contains("not a repository")
    );
}

#[test]
fn a_merge_that_would_conflict_offers_to_resolve_it_in_the_branch_and_touches_nothing() {
    let run = inspected_for_merge(
        merging(),
        MergeStanding {
            conflicts: vec!["src/a.rs".into(), "b.toml".into()],
            ..mergeable()
        },
    );

    let ask = run.state().overlays.confirm.clone().expect("asked");
    let (_, title, body, action) = ask.dialog();
    assert!(title.contains("conflict"), "{title}");
    assert!(
        body.contains("src/a.rs") && body.contains("b.toml"),
        "{body}"
    );
    assert!(body.contains("main checkout is not touched"), "{body}");
    assert_eq!(action, "Resolve in the branch");
    assert_eq!(
        run.send(Msg::ConfirmAnswer(true)).last_effects(),
        [Effect::WorktreeResolve {
            environment: here(),
            path: "/r/a".into(),
        }]
    );
}

#[test]
fn a_merged_branch_is_said_and_its_worktree_is_offered_for_deleting() {
    let run = merging().send(Msg::WorktreeMerged {
        environment: here(),
        path: "/r/a".into(),
        result: Ok(Merged::Done {
            branch: "a".into(),
            base: "main".into(),
            commit: "abc1234".into(),
            stash: Some("tsumugi: before merging a".into()),
            closed: Some(LinkedIssue {
                number: 44,
                title: "Integrate".into(),
            }),
        }),
    });

    let said = run.state().chrome.status.line.clone().unwrap();
    assert!(said.contains("Merged a into main at abc1234"), "{said}");
    assert!(said.contains("closed #44"), "{said}");
    assert!(said.contains("stash"), "{said}");
    assert!(run.last_effects().contains(&Effect::WorktreeInspect {
        environment: here(),
        path: "/r/a".into()
    }));
}

fn only_a_notice(effects: &[Effect]) -> bool {
    effects
        .iter()
        .all(|e| matches!(e, Effect::LapseNotice { .. }))
}

#[test]
fn a_merge_that_turned_out_to_conflict_or_failed_is_said_and_deletes_nothing() {
    let conflicted = merging().send(Msg::WorktreeMerged {
        environment: here(),
        path: "/r/a".into(),
        result: Ok(Merged::Conflicts {
            files: vec!["a.rs".into()],
        }),
    });
    assert!(only_a_notice(conflicted.last_effects()));
    assert!(
        conflicted
            .state()
            .chrome
            .status
            .line
            .as_deref()
            .unwrap()
            .contains("conflict in a.rs")
    );

    let failed = merging().send(Msg::WorktreeMerged {
        environment: here(),
        path: "/r/a".into(),
        result: Err("the merge failed".into()),
    });
    assert!(only_a_notice(failed.last_effects()));
    assert!(
        failed
            .state()
            .chrome
            .status
            .line
            .as_deref()
            .unwrap()
            .contains("cannot merge a: the merge failed")
    );
}

#[test]
fn what_became_of_resolving_is_said() {
    let said = |result| {
        merging()
            .send(Msg::WorktreeResolved {
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
    assert_eq!(
        said(Ok("a session resolves them".into())),
        "a session resolves them"
    );
    assert!(said(Err("dirty".into())).contains("cannot resolve a: dirty"));
}
