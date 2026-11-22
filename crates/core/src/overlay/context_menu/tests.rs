use super::*;
use crate::testing::here;

fn grouped(target: Target, s: Option<&Session>) -> Vec<Vec<Action>> {
    groups(&target, s, &here(), Reach::Here)
        .into_iter()
        .map(|g| g.into_iter().map(|(a, _)| a).collect())
        .collect()
}

#[test]
fn every_entry_of_every_menu_shows_an_icon_and_has_a_row_in_the_table() {
    let own = session();
    let targets = [
        Target::Agent(here(), "a".into()),
        Target::Row(Item::Workspace(here(), "/api".into())),
        Target::Row(Item::Worktree(here(), "/api".into())),
        Target::Row(Item::Session(here(), "a".into())),
        Target::Row(Item::Waiting(here(), "a".into())),
        Target::Row(Item::Terminal(here(), 1)),
        Target::Tab(Tab::Session(here(), "a".into())),
        Target::Tab(Tab::Terminal(here(), 1)),
        Target::Link { at: 0, file: true },
        Target::Pinned { at: 0, file: true },
        Target::Commit { at: 0 },
        Target::Panel,
    ];
    for target in targets {
        for s in [None, Some(&own)] {
            for (action, label) in entries(&target, s, &here(), Reach::Here) {
                assert!(action.icon().is_some(), "{label}");
                assert!(action.spec().row.is_some(), "{label}");
            }
        }
    }
}

#[test]
fn a_link_of_the_overview_is_opened_or_copied_and_a_file_revealed_too() {
    assert_eq!(
        actions(Target::Link { at: 2, file: false }, None),
        [Action::CursorGo, Action::CursorCopy, Action::ResourceKeep]
    );
    assert_eq!(
        actions(Target::Link { at: 2, file: true }, None),
        [
            Action::CursorGo,
            Action::CursorCopy,
            Action::CursorReveal,
            Action::ResourceKeep
        ]
    );
}

#[test]
fn a_pinned_resource_opens_where_a_file_does_and_is_copied_edited_and_removed() {
    assert_eq!(
        grouped(Target::Pinned { at: 1, file: false }, None),
        [
            [&OPENS[..], &[Action::CursorCopy, Action::ResourceEdit]].concat(),
            vec![Action::ResourceRemove]
        ]
    );
    assert!(actions(Target::Pinned { at: 1, file: true }, None).contains(&Action::CursorReveal));
}

#[test]
fn the_panel_itself_offers_to_keep_a_resource() {
    assert_eq!(
        actions(Target::Panel, None),
        [Action::ResourceAdd, Action::ResourceScreenshot]
    );
}

#[test]
fn a_menu_draws_going_starting_and_ending_apart() {
    let s = session();
    assert_eq!(
        grouped(Target::Row(Item::Session(here(), "a".into())), Some(&s)),
        [
            [
                &OPENS[..],
                &[
                    Action::SessionRename,
                    Action::SessionMarkUnread,
                    Action::NotificationsMute,
                    Action::CursorCopy
                ]
            ]
            .concat(),
            vec![
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::BrowserOpen,
                Action::LocalIssues
            ],
            vec![Action::CursorClose, Action::SessionStop],
        ]
    );
    assert_eq!(
        grouped(Target::Tab(Tab::Terminal(here(), 1)), None),
        [vec![Action::TabClose, Action::TerminalStop]]
    );
    let draft = Tab::draft(here(), "/a");
    assert!(grouped(Target::Tab(draft), None).is_empty());
}

/// What every row that opens offers first: where to open it.
const OPENS: [Action; 4] = [
    Action::CursorGo,
    Action::CursorSplit,
    Action::CursorVsplit,
    Action::CursorTab,
];

fn session() -> Session {
    tsumugi_model::testing::session("a").build()
}

fn actions(target: Target, s: Option<&Session>) -> Vec<Action> {
    entries(&target, s, &here(), Reach::Here)
        .into_iter()
        .map(|(a, _)| a)
        .collect()
}

#[test]
fn a_session_tsumugi_runs_can_be_opened_renamed_started_beside_and_ended() {
    let s = session();
    assert_eq!(
        actions(Target::Row(Item::Session(here(), "a".into())), Some(&s)),
        [
            &OPENS[..],
            &[
                Action::SessionRename,
                Action::SessionMarkUnread,
                Action::NotificationsMute,
                Action::CursorCopy,
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::BrowserOpen,
                Action::LocalIssues,
                Action::CursorClose,
                Action::SessionStop,
            ],
        ]
        .concat()
    );
}

#[test]
fn a_workspace_or_worktree_starts_sessions_and_shells_in_it() {
    let starts = [
        Action::SessionNew,
        Action::ShellOpen,
        Action::NeovimOpen,
        Action::BrowserOpen,
        Action::LocalIssues,
    ];
    assert_eq!(
        actions(Target::Row(Item::Workspace(here(), "/api".into())), None),
        [
            &[
                Action::CursorGo,
                Action::CursorCopy,
                Action::NotificationsMute
            ][..],
            &starts,
            &[Action::WorktreeNew, Action::WorkspaceRemove]
        ]
        .concat()
    );
    assert_eq!(
        actions(Target::Row(Item::Worktree(here(), "/api".into())), None),
        [
            &OPENS[..],
            &[Action::CursorCopy, Action::NotificationsMute],
            &starts,
            &[
                Action::WorktreeNew,
                Action::WorktreeMerge,
                Action::WorktreeClose,
                Action::WorktreeDelete
            ]
        ]
        .concat()
    );
}

#[test]
fn the_plus_of_a_workspace_or_worktree_offers_a_new_worktree_beside_what_starts_work() {
    for row in [
        Item::Workspace(here(), "/api".into()),
        Item::Worktree(here(), "/api".into()),
    ] {
        let offered = actions(Target::Start(row), None);
        assert_eq!(offered.last(), Some(&Action::WorktreeNew));
    }
}

#[test]
fn the_row_of_a_worktree_being_made_offers_nothing() {
    let row = Item::Pending(here(), "/api".into(), 1);
    assert_eq!(actions(Target::Row(row), None), Vec::<Action>::new());
}

#[test]
fn the_plus_of_an_environment_s_heading_offers_only_what_starts_work_there() {
    let start = Target::Start(Item::Environment(crate::testing::there()));
    assert_eq!(actions(start.clone(), None), STARTS);
    for (action, label) in entries(&start, None, &here(), Reach::Here) {
        assert!(action.icon().is_some(), "{label}");
        assert!(action.spec().row.is_some(), "{label}");
    }
}

#[test]
fn a_terminal_row_opens_it_starts_beside_it_and_offers_an_explicit_stop() {
    assert_eq!(
        actions(Target::Row(Item::Terminal(here(), 1)), None),
        [
            &OPENS[..],
            &[
                Action::CursorCopy,
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::BrowserOpen,
                Action::LocalIssues,
                Action::CursorClose,
                Action::TerminalStop,
            ],
        ]
        .concat()
    );
}

#[test]
fn a_file_opens_where_its_key_says_copies_its_path_makes_beside_it_and_closes_its_windows() {
    let file = Item::File {
        environment: here(),
        worktree: "/api".into(),
        path: "/api/a.rs".into(),
        dir: false,
    };
    assert_eq!(
        actions(Target::Row(file), None),
        [
            &OPENS[..],
            &[
                Action::CursorCopy,
                Action::SessionNew,
                Action::ShellOpen,
                Action::NeovimOpen,
                Action::BrowserOpen,
                Action::LocalIssues,
                Action::FileNew,
                Action::DirectoryNew,
                Action::CursorClose,
            ],
        ]
        .concat()
    );
}

#[test]
fn a_directory_and_the_files_heading_make_files_and_directories_in_them() {
    let dir = Item::File {
        environment: here(),
        worktree: "/api".into(),
        path: "/api/src".into(),
        dir: true,
    };
    let files = Item::Heading(here(), "/api".into(), crate::tree::nav::Heading::Files);
    let sessions = Item::Heading(here(), "/api".into(), crate::tree::nav::Heading::Sessions);
    for row in [dir, files] {
        let got = actions(Target::Row(row.clone()), None);
        assert!(
            got.ends_with(&[Action::FileNew, Action::DirectoryNew]),
            "{row:?}"
        );
    }
    assert!(!actions(Target::Row(sessions), None).contains(&Action::FileNew));
}

#[test]
fn a_heading_folds_and_has_nothing_to_copy() {
    let got = actions(
        Target::Row(Item::Heading(
            here(),
            "/api".into(),
            crate::tree::nav::Heading::Files,
        )),
        None,
    );
    assert_eq!(got[0], Action::CursorGo);
    assert!(!got.contains(&Action::CursorCopy));
}

#[test]
fn closing_a_tab_and_stopping_its_agent_or_terminal_are_distinct_actions() {
    let s = session();
    assert_eq!(
        actions(Target::Tab(Tab::Session(here(), "a".into())), Some(&s)),
        [Action::TabClose, Action::SessionRename, Action::SessionStop]
    );
    assert_eq!(
        actions(Target::Tab(Tab::Terminal(here(), 1)), None),
        [Action::TabClose, Action::TerminalStop]
    );
}

fn heading_groups(environment: EnvironmentId, reach: Reach) -> Vec<Vec<Action>> {
    groups(
        &Target::Row(Item::Environment(environment)),
        None,
        &here(),
        reach,
    )
    .into_iter()
    .map(|g| g.into_iter().map(|(a, _)| a).collect())
    .collect()
}

const STARTS: [Action; 6] = [
    Action::SessionNew,
    Action::ShellOpen,
    Action::NeovimOpen,
    Action::BrowserOpen,
    Action::LocalIssues,
    Action::WorkspaceAdd,
];
const CONFIGURES: [Action; 3] = [
    Action::SettingsOpen,
    Action::EnvironmentRename,
    Action::CursorGo,
];

#[test]
fn an_environment_s_heading_starts_work_in_it_configures_it_and_removes_it() {
    use crate::testing::there;

    assert_eq!(
        heading_groups(there(), Reach::Here),
        [
            STARTS.to_vec(),
            CONFIGURES.to_vec(),
            vec![Action::EnvironmentRemove],
        ]
    );
}

#[test]
fn the_window_s_own_environment_is_never_offered_for_removal() {
    assert_eq!(
        heading_groups(here(), Reach::Here),
        [STARTS.to_vec(), CONFIGURES.to_vec()]
    );
}

#[test]
fn a_work_machine_s_heading_pairs_again_and_retries_only_while_the_window_waits_to() {
    use crate::testing::there;

    let reaching = |retryable| heading_groups(there(), Reach::Machine { retryable })[2].clone();
    assert_eq!(reaching(false), [Action::EnvironmentPair]);
    assert_eq!(
        reaching(true),
        [Action::EnvironmentRetry, Action::EnvironmentPair]
    );
}

#[test]
fn every_entry_of_a_heading_s_menu_shows_an_icon_and_has_a_row_in_the_table() {
    let target = Target::Row(Item::Environment(crate::testing::there()));
    for (action, label) in entries(&target, None, &here(), Reach::Machine { retryable: true }) {
        assert!(action.icon().is_some(), "{label}");
        assert!(action.spec().row.is_some(), "{label}");
    }
}

#[test]
fn a_commit_of_the_log_offers_what_can_be_done_with_it_and_runs_it_on_that_row() {
    let offered = grouped(Target::Commit { at: 3 }, None);
    let all: Vec<Action> = offered.concat();
    for action in [
        Action::LogOpenRemote,
        Action::LogCopyHash,
        Action::LogExplain,
        Action::LogBranch,
        Action::LogWorktree,
        Action::LogCherryPick,
        Action::LogRevert,
    ] {
        assert!(all.contains(&action), "{action:?}");
    }
    assert_eq!(all.len(), 7);
}

#[test]
fn a_session_whose_program_exited_is_offered_a_restart_first() {
    let mut s = session();
    s.rest = Some(tsumugi_model::Rest::Exited);
    let offered = actions(Target::Row(Item::Session(here(), "a".into())), Some(&s));
    assert_eq!(offered[OPENS.len()], Action::SessionRestart);
    assert!(
        !actions(
            Target::Row(Item::Session(here(), "a".into())),
            Some(&session())
        )
        .contains(&Action::SessionRestart)
    );
}
