use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use tsumugi_term::Daemon;

use super::*;
use crate::delegation::protocol::Command;
use crate::delegation::service::fixtures::conversation_watch;
use crate::settings::Settings;
use crate::testing::record;
use tsumugi_model::LocalIssueAction;

#[tokio::test]
async fn research_can_be_saved_as_a_local_issue_in_the_callers_directory_without_starting() {
    let (dir, context, caller, _) = conversation_watch("research");
    let result = create_local_issue(
        &context,
        &caller,
        protocol::CreateLocalIssue {
            title: "Compare Orca".into(),
            body: "Findings and proposed follow-up".into(),
            dir: None,
        },
    )
    .await
    .unwrap();
    let saved: tsumugi_model::LocalIssue = serde_json::from_value(result).unwrap();
    assert_eq!(
        saved.dir,
        tsumugi_sys::fs::canonicalize(dir.path()).unwrap()
    );
    assert_eq!(saved.title, "Compare Orca");
    assert_eq!(saved.body, "Findings and proposed follow-up");
    assert_eq!(saved.state, tsumugi_model::LocalIssueState::Open);
    assert!(saved.session.is_none());
    assert!(
        context
            .store
            .lock()
            .unwrap()
            .claim_local_issues()
            .unwrap()
            .is_empty()
    );
    assert!(
        Command::CreateLocalIssue(protocol::CreateLocalIssue {
            title: "Follow-up".into(),
            body: String::new(),
            dir: None,
        })
        .acts()
    );
}

#[tokio::test]
async fn an_agent_keeps_a_resource_in_its_project_and_finds_it_by_loose_words() {
    let (dir, context, caller, _) = conversation_watch("resources");
    std::fs::write(dir.path().join("nginx.conf"), "").unwrap();
    for (target, label) in [
        ("nginx.conf", Some("Web server")),
        ("https://example.com/guide", None),
    ] {
        add_resource(
            &context,
            &caller,
            protocol::AddResource {
                target: target.into(),
                label: label.map(Into::into),
                note: None,
                dir: None,
            },
        )
        .await
        .unwrap();
    }
    let found = resources(
        &context,
        &caller,
        protocol::Resources {
            query: Some("CONF web".into()),
            dir: None,
        },
    )
    .await
    .unwrap();
    let root = tsumugi_sys::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(found["project"], json!(root));
    let kept = found["resources"].as_array().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0]["title"], "Web server");
    assert_eq!(kept[0]["kind"], "path");
    assert_eq!(kept[0]["exists"], true);
    assert_eq!(kept[0]["target"], json!(root.join("nginx.conf")));
}

#[tokio::test]
async fn a_resource_whose_file_is_gone_is_still_listed_and_said_to_be_missing() {
    let (_dir, context, caller, _) = conversation_watch("resources");
    add_resource(
        &context,
        &caller,
        protocol::AddResource {
            target: "gone.md".into(),
            label: None,
            note: Some("Old design".into()),
            dir: None,
        },
    )
    .await
    .unwrap();
    let found = resources(&context, &caller, protocol::Resources::default())
        .await
        .unwrap();
    assert_eq!(found["resources"][0]["exists"], false);
    assert_eq!(found["resources"][0]["note"], "Old design");
}

#[tokio::test]
async fn an_address_is_kept_as_written_and_never_said_to_be_missing() {
    let (_dir, context, caller, _) = conversation_watch("resources");
    add_resource(
        &context,
        &caller,
        protocol::AddResource {
            target: "https://example.com/a".into(),
            label: None,
            note: None,
            dir: None,
        },
    )
    .await
    .unwrap();
    let found = resources(&context, &caller, protocol::Resources::default())
        .await
        .unwrap();
    assert_eq!(found["resources"][0]["kind"], "address");
    assert_eq!(found["resources"][0]["target"], "https://example.com/a");
    assert_eq!(found["resources"][0]["exists"], Value::Null);
}

#[tokio::test]
async fn a_project_s_resources_stay_out_of_the_sight_of_another_project() {
    let (_dir, context, caller, _) = conversation_watch("resources");
    let other = tempfile::tempdir().unwrap();
    add_resource(
        &context,
        &caller,
        protocol::AddResource {
            target: "https://example.com/a".into(),
            label: None,
            note: None,
            dir: Some(other.path().to_owned()),
        },
    )
    .await
    .unwrap();
    let own = resources(&context, &caller, protocol::Resources::default())
        .await
        .unwrap();
    assert!(own["resources"].as_array().unwrap().is_empty());
    let theirs = resources(
        &context,
        &caller,
        protocol::Resources {
            query: None,
            dir: Some(other.path().to_owned()),
        },
    )
    .await
    .unwrap();
    assert_eq!(theirs["resources"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn keeping_a_resource_acts_for_the_session_and_reading_them_does_not() {
    assert!(
        Command::AddResource(protocol::AddResource {
            target: "https://example.com".into(),
            label: None,
            note: None,
            dir: None,
        })
        .acts()
    );
    assert!(!Command::Resources(protocol::Resources::default()).acts());
}

#[tokio::test]
async fn an_agent_removes_a_resource_by_its_id_and_cannot_remove_one_twice() {
    let (_dir, context, caller, _) = conversation_watch("resources");
    let kept = add_resource(
        &context,
        &caller,
        protocol::AddResource {
            target: "https://example.com/a".into(),
            label: None,
            note: None,
            dir: None,
        },
    )
    .await
    .unwrap();
    let id = kept["id"].as_str().unwrap().to_owned();
    remove_resource(&context, protocol::RemoveResource { id: id.clone() })
        .await
        .unwrap();
    let left = resources(&context, &caller, protocol::Resources::default())
        .await
        .unwrap();
    assert!(left["resources"].as_array().unwrap().is_empty());
    let again = remove_resource(&context, protocol::RemoveResource { id })
        .await
        .unwrap_err();
    assert_eq!(again.code, Code::NotFound);
    assert!(Command::RemoveResource(protocol::RemoveResource { id: "x".into() }).acts());
}

/// A terminal host serving in `dir`, and a terminal running there.
async fn serving(dir: &Path) -> (Daemon, u128) {
    let socket = dir.join("daemon.sock");
    let serving = socket.clone();
    crate::environment::spawn(async move {
        Daemon::serve(&serving, Duration::from_millis(20), |_| {})
            .await
            .unwrap()
    });
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let terminal = daemon
        .spawn(tsumugi_term::Spawn {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "read answer".into()],
            cwd: dir.into(),
            env: Vec::new(),
            tags: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .await
        .unwrap();
    (daemon, terminal)
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs its terminal's program with /bin/sh")]
async fn a_session_closing_itself_is_answered_before_it_ends_and_is_recorded_ended() {
    let dir = tempfile::Builder::new()
        .prefix("tsumugi-")
        .tempdir_in(tsumugi_sys::local::temp_dir())
        .unwrap();
    tsumugi_sys::fs::set_mode(dir.path(), 0o700).unwrap();
    let (_state, mut context, caller, _) = conversation_watch("review");
    let (daemon, terminal) = serving(dir.path()).await;
    context.daemon = daemon.clone();
    context
        .store
        .lock()
        .unwrap()
        .attach(caller.id, terminal, None)
        .unwrap();
    let caller = context.store.lock().unwrap().known(caller.id).unwrap();

    close(&context, &caller, protocol::CloseSession::default())
        .await
        .unwrap();

    let runs =
        |sessions: &[tsumugi_term::Session]| sessions.iter().any(|s| s.id == terminal && !s.exited);
    assert!(runs(&daemon.sessions().await.unwrap()));
    let mut ended = false;
    for _ in 0..200 {
        let gone = !runs(&daemon.sessions().await.unwrap());
        if gone
            && context
                .store
                .lock()
                .unwrap()
                .known(caller.id)
                .unwrap()
                .ended
        {
            ended = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ended, "the session still runs or is not recorded ended");
}

#[tokio::test]
async fn a_session_with_no_terminal_yet_cannot_close_itself() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let refused = close(&context, &caller, protocol::CloseSession::default())
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
}

#[test]
fn a_codex_subagent_cannot_close_its_session() {
    assert!(Command::Close(protocol::CloseSession::default()).acts());
}

fn renaming(name: &str) -> protocol::Rename {
    protocol::Rename { name: name.into() }
}

#[tokio::test]
async fn a_session_is_not_renamed_by_its_agent_while_renaming_is_switched_off() {
    let (_dir, mut context, caller, _) = conversation_watch("review");
    context.settings = Arc::new(|| Ok(Settings::parse("rename_sessions = false\n").unwrap()));

    let refused = rename(&context, &caller, renaming("レビュー"))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(refused.message.contains("keep"), "{}", refused.message);
}

#[tokio::test]
async fn a_name_a_person_gave_is_not_changed_by_the_session_s_agent() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    caller.name_given = true;

    let refused = rename(&context, &caller, renaming("レビュー"))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(refused.message.contains("review"), "{}", refused.message);
}

#[tokio::test]
async fn a_name_without_letters_or_digits_is_refused() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let refused = rename(&context, &caller, renaming(" ！？ "))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
}

fn tasks(ops: Value) -> protocol::TimelineTasks {
    serde_json::from_value(serde_json::json!({ "ops": ops })).unwrap()
}

#[tokio::test]
async fn a_timeline_call_is_journaled_and_answered_with_the_trees() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let answer = record_timeline(
        &context,
        &caller,
        tasks(json!([
            {"op": "add", "id": "goal", "title": "Review"},
            {"op": "add", "id": "a", "parent": "goal", "title": "Read", "status": "completed"},
            {"op": "add", "id": "b", "parent": "goal", "title": "Write"},
        ])),
    )
    .await
    .unwrap();

    let text = answer.as_str().unwrap();
    assert!(text.contains("50.00% completed"), "{text}");
    let kept = context
        .store
        .lock()
        .unwrap()
        .timeline_of(&caller.code)
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].ops.len(), 3);
}

#[tokio::test]
async fn the_last_note_of_a_timeline_call_becomes_the_line_of_the_callers_worktree() {
    let (dir, context, caller, _) = conversation_watch("review");

    record_timeline(
        &context,
        &caller,
        tasks(json!([
            {"op": "add", "id": "a", "title": "Read", "note": "first"},
            {"op": "add", "id": "b", "title": "Write", "note": "  tests are red\nmore"},
            {"op": "add", "id": "c", "title": "Ship"},
        ])),
    )
    .await
    .unwrap();
    record_timeline(
        &context,
        &caller,
        tasks(json!([{"op": "update", "id": "c", "status": "in_progress"}])),
    )
    .await
    .unwrap();

    let kept = context.store.lock().unwrap().checkpoints().unwrap();
    let [said] = &kept[..] else {
        panic!("one checkpoint: {kept:?}")
    };
    assert_eq!(
        said.worktree,
        tsumugi_sys::fs::canonicalize(dir.path()).unwrap()
    );
    assert_eq!(said.noted, "tests are red");
    assert_eq!(said.comment, "");
}

fn checkpoint(words: Value) -> protocol::WorktreeCheckpoint {
    serde_json::from_value(words).unwrap()
}

#[tokio::test]
async fn an_agent_says_how_its_worktree_stands_and_reads_it_back_without_changing_it() {
    let (dir, context, caller, _) = conversation_watch("review");
    let worktree = tsumugi_sys::fs::canonicalize(dir.path()).unwrap();

    let said = worktree_checkpoint(
        &context,
        &caller,
        checkpoint(json!({"comment": "tests are red\nmore", "status": "review"})),
    )
    .await
    .unwrap();
    assert_eq!(said["line"], "In review · tests are red");

    let read = worktree_checkpoint(&context, &caller, checkpoint(json!({})))
        .await
        .unwrap();
    assert_eq!(read["line"], "In review · tests are red");
    let kept = context.store.lock().unwrap().checkpoints().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].worktree, worktree);

    let cleared = worktree_checkpoint(
        &context,
        &caller,
        checkpoint(json!({"clear_comment": true, "clear_status": true})),
    )
    .await
    .unwrap();
    assert_eq!(cleared["line"], "");
}

#[tokio::test]
async fn a_checkpoint_with_a_status_nobody_knows_or_no_directory_is_refused() {
    let (dir, context, caller, _) = conversation_watch("review");
    let refused = worktree_checkpoint(&context, &caller, checkpoint(json!({"status": "blocked"})))
        .await
        .unwrap_err();
    assert_eq!(refused.code, Code::Invalid);
    let gone = dir.path().join("gone");
    let refused = worktree_checkpoint(
        &context,
        &caller,
        checkpoint(json!({"comment": "x", "dir": gone})),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, Code::Invalid);
    assert!(
        context
            .store
            .lock()
            .unwrap()
            .checkpoints()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_timeline_call_says_which_changes_it_could_not_make() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let answer = record_timeline(
        &context,
        &caller,
        tasks(json!([
            {"op": "add", "id": "goal", "title": "Review"},
            {"op": "update", "id": "ghost", "status": "completed"},
        ])),
    )
    .await
    .unwrap();

    let text = answer.as_str().unwrap();
    assert!(text.contains("Not made:\nghost: no such task"), "{text}");
}

#[tokio::test]
async fn a_timeline_call_that_makes_no_change_is_refused_and_not_journaled() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let refused = record_timeline(
        &context,
        &caller,
        tasks(json!([{"op": "remove", "id": "ghost"}])),
    )
    .await
    .unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
    assert!(refused.message.contains("ghost"), "{}", refused.message);
    let kept = context
        .store
        .lock()
        .unwrap()
        .timeline_of(&caller.code)
        .unwrap();
    assert!(kept.is_empty());
}

fn note_of(answer: Value) -> Option<String> {
    answer["note"].as_str().map(String::from)
}

#[tokio::test]
async fn after_a_clear_the_agent_is_asked_to_name_its_session_for_the_work_that_follows() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let note = note_of(prompted(&context, &caller, true).await.unwrap()).unwrap();

    assert!(note.contains("`review`"), "{note}");
    assert!(note.contains("cleared"), "{note}");
    assert!(note.contains("call `rename_session`"), "{note}");
}

#[tokio::test]
async fn at_each_prompt_the_agent_keeps_the_name_unless_the_work_turned() {
    let (_dir, context, caller, _) = conversation_watch("review");

    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();

    assert!(note.contains("`review`"), "{note}");
    assert!(note.contains("`rename_session`"), "{note}");
    assert!(note.contains("keep the name"), "{note}");
}

#[tokio::test]
async fn a_clear_hands_a_name_the_user_gave_back_to_the_agent() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    context
        .store
        .lock()
        .unwrap()
        .rename(caller.id, "review", true)
        .unwrap();
    caller.name_given = true;

    let note = note_of(prompted(&context, &caller, true).await.unwrap()).unwrap();

    assert!(note.contains("call `rename_session`"), "{note}");
    let kept = context.store.lock().unwrap().known(caller.id).unwrap();
    assert!(!kept.name_given);
}

#[tokio::test]
async fn a_session_whose_name_its_agent_may_not_change_is_not_asked_to_name_it() {
    let (_dir, mut context, mut caller, _) = conversation_watch("review");
    caller.name_given = true;
    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();
    assert!(!note.contains("`rename_session`"), "{note}");

    caller.name_given = false;
    context.settings = Arc::new(|| Ok(Settings::parse("rename_sessions = false\n").unwrap()));
    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();
    assert!(!note.contains("`rename_session`"), "{note}");
}

#[tokio::test]
async fn at_each_prompt_a_session_with_nothing_on_its_timeline_is_reminded_to_record_its_plan() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    caller.name_given = true;

    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();

    assert!(note.contains("`timeline_tasks`"), "{note}");
}

#[tokio::test]
async fn once_its_agent_has_recorded_a_timeline_a_prompt_no_longer_reminds_it() {
    let (_dir, context, mut caller, _) = conversation_watch("review");
    record_timeline(
        &context,
        &caller,
        tasks(json!([{"op": "add", "id": "goal", "title": "Review"}])),
    )
    .await
    .unwrap();

    let note = note_of(prompted(&context, &caller, false).await.unwrap()).unwrap();
    assert!(!note.contains("`timeline_tasks`"), "{note}");

    caller.name_given = true;
    assert_eq!(
        note_of(prompted(&context, &caller, false).await.unwrap()),
        None
    );
}

#[tokio::test]
async fn a_start_while_sessions_opening_sessions_is_switched_off_is_refused_and_leaves_nothing() {
    let (dir, mut context, caller, _) = conversation_watch("review");
    let worktrees = dir.path().join("repo");
    std::fs::create_dir(&worktrees).unwrap();
    context.settings = Arc::new(|| Ok(Settings::parse("[delegation]\nenabled = false\n").unwrap()));
    let request = serde_json::from_value(serde_json::json!({
        "prompt": "review the change",
        "dir": worktrees,
        "worktree": true,
    }))
    .unwrap();

    let refused = start(&context, &caller, request).await.unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(
        refused.message.contains("carry on yourself"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("ask the user"),
        "{}",
        refused.message
    );
    let sessions = context.call(|store| store.sessions()).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(std::fs::read_dir(&worktrees).unwrap().count(), 0);
}

#[tokio::test]
async fn sessions_are_listed_with_their_codes_and_the_code_of_the_one_that_opened_them() {
    let opener = record("opener").build();
    let opened = record("opened").parent(opener.id).build();
    let listed = listed(
        vec![opener.clone(), opened.clone()],
        &protocol::Sessions::default(),
    )
    .await
    .unwrap();

    assert_eq!(listed[0]["code"], opener.code.as_str());
    assert_eq!(listed[1]["code"], opened.code.as_str());
    assert_eq!(listed[1]["opened_by"]["code"], opener.code.as_str());
}

#[tokio::test]
async fn a_session_asleep_is_listed_as_asleep_and_one_that_exited_or_ended_is_not_listed() {
    let mut asleep = record("asleep").ended().build();
    asleep.rest = Some(tsumugi_model::Rest::Hibernated);
    let mut exited = record("exited").ended().build();
    exited.rest = Some(tsumugi_model::Rest::Exited);
    let ended = record("ended").ended().build();
    let running = record("running").build();

    let listed = listed(
        vec![asleep.clone(), exited, ended, running.clone()],
        &protocol::Sessions::default(),
    )
    .await
    .unwrap();

    assert_eq!(listed.as_array().unwrap().len(), 2);
    assert_eq!(listed[0]["code"], asleep.code.as_str());
    assert_eq!(listed[0]["asleep"], true);
    assert_eq!(listed[1]["code"], running.code.as_str());
    assert!(listed[1].get("asleep").is_none());
}

async fn saved(context: &Context, caller: &Session, title: &str, dir: &Path) -> String {
    let result = create_local_issue(
        context,
        caller,
        protocol::CreateLocalIssue {
            title: title.into(),
            body: format!("About {title}"),
            dir: Some(dir.into()),
        },
    )
    .await
    .unwrap();
    serde_json::from_value::<tsumugi_model::LocalIssue>(result)
        .unwrap()
        .id
}

fn named(issue: &str) -> protocol::LocalIssueName {
    protocol::LocalIssueName {
        issue: issue.into(),
    }
}

#[tokio::test]
async fn an_agent_lists_the_environments_issues_by_state_and_reads_one_by_number() {
    let (dir, context, caller, _) = conversation_watch("listing");
    saved(&context, &caller, "first", dir.path()).await;
    let second = saved(&context, &caller, "second", dir.path()).await;
    change_local_issue(&context, LocalIssueAction::Close, named(&second))
        .await
        .unwrap();
    let all = local_issues(&context, &caller, protocol::LocalIssues::default())
        .await
        .unwrap();
    assert_eq!(all["issues"].as_array().unwrap().len(), 2);
    let closed = local_issues(
        &context,
        &caller,
        protocol::LocalIssues {
            state: Some("closed".into()),
            dir: Some(dir.path().into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(closed["issues"][0]["title"], "second");
    assert_eq!(closed["issues"][0]["number"], 2);
    let elsewhere = local_issues(
        &context,
        &caller,
        protocol::LocalIssues {
            state: None,
            dir: Some(dir.path().join("nowhere")),
        },
    )
    .await
    .unwrap();
    assert!(elsewhere["issues"].as_array().unwrap().is_empty());
    let bad = local_issues(
        &context,
        &caller,
        protocol::LocalIssues {
            state: Some("stuck".into()),
            dir: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(bad.code, Code::Invalid);
    let read = local_issue(&context, named("1")).await.unwrap();
    assert_eq!(read["issue"]["body"], "About first");
}

#[tokio::test]
async fn an_agent_closes_reopens_and_cancels_issues_but_never_starts_one() {
    use LocalIssueAction as Action;
    let (dir, context, caller, _) = conversation_watch("closing");
    let id = saved(&context, &caller, "kept", dir.path()).await;
    let state = |done: Value| done["issue"]["state"].as_str().unwrap().to_owned();
    let act = |action, issue: String| {
        let context = context.clone();
        async move { change_local_issue(&context, action, named(&issue)).await }
    };
    assert_eq!(
        state(act(Action::Close, id.clone()).await.unwrap()),
        "closed"
    );
    assert_eq!(
        state(act(Action::Reopen, id.clone()).await.unwrap()),
        "open"
    );
    assert_eq!(
        state(act(Action::Cancel, id.clone()).await.unwrap()),
        "cancelled"
    );
    assert_eq!(
        // Not a hex digit, so no random issue id can start with it.
        act(Action::Close, "zz".into()).await.unwrap_err().code,
        Code::NotFound
    );
    act(Action::Reopen, id.clone()).await.unwrap();
    for action in [Action::Start, Action::Retry, Action::Delete] {
        let error = act(action, id.clone()).await.unwrap_err();
        assert_eq!(error.code, Code::Refused, "{}", action.word());
    }
    assert_eq!(
        state(local_issue(&context, named(&id)).await.unwrap()),
        "open"
    );
    assert!(
        context
            .store
            .lock()
            .unwrap()
            .claim_local_issues()
            .unwrap()
            .is_empty()
    );
    for command in [
        Command::CloseLocalIssue(named("1")),
        Command::ReopenLocalIssue(named("1")),
        Command::CancelLocalIssue(named("1")),
    ] {
        assert!(command.acts());
    }
    assert!(!Command::LocalIssues(Default::default()).acts());
    assert!(!Command::ReadLocalIssue(named("1")).acts());
}

#[tokio::test]
async fn an_issue_at_work_is_not_closed_by_an_agent() {
    let (dir, context, caller, _) = conversation_watch("busy");
    let id = saved(&context, &caller, "busy", dir.path()).await;
    {
        let mut store = context.store.lock().unwrap();
        store.start_local_issue(&id).unwrap();
        store.claim_local_issues().unwrap();
    }
    let error = change_local_issue(&context, LocalIssueAction::Close, named("1"))
        .await
        .unwrap_err();
    assert_eq!(error.code, Code::Refused);
}

/// A session `caller` opened and supervises.
async fn worker_of(context: &Context, caller: &Session) -> Session {
    let (opener, worker) = (caller.id, "worker");
    context
        .call(move |store| {
            let (id, _) = store.register(crate::delegation::store::NewSession {
                parent: Some(opener),
                agent: tsumugi_model::Agent::Claude,
                profile: tsumugi_model::profile::ProfileId::builtin(tsumugi_model::Agent::Claude),
                home: None,
                name: worker.into(),
                cwd: "/repo".into(),
                launch: Settings::default(),
                model: None,
                permissions: Default::default(),
                started_ms: 1,
                name_given: false,
                code: None,
            })?;
            store.supervise(id)?;
            store.known(id)
        })
        .await
        .unwrap()
}

fn done(outcome: protocol::Outcome, summary: &str) -> protocol::ReportDone {
    protocol::ReportDone {
        outcome,
        summary: summary.into(),
        files: vec!["src/lib.rs".into()],
    }
}

#[test]
fn reporting_and_waiting_act_for_the_session() {
    assert!(Command::ReportDone(done(protocol::Outcome::Succeeded, "done")).acts());
    assert!(Command::Wait(protocol::Wait::default()).acts());
}

#[tokio::test]
async fn a_worker_reports_done_to_the_session_that_opened_it() {
    let (_dir, context, supervisor, _) = conversation_watch("lead");
    let worker = worker_of(&context, &supervisor).await;

    report_done(
        &context,
        &worker,
        done(protocol::Outcome::Succeeded, "Fixed it"),
    )
    .await
    .unwrap();

    let heard = context
        .call(move |store| store.next_undelivered(supervisor.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(heard.sender, Some(worker.id));
    assert!(heard.body.contains("outcome: succeeded"), "{}", heard.body);
}

#[tokio::test]
async fn a_second_report_is_refused_and_tells_the_opener_nothing_more() {
    let (_dir, context, supervisor, _) = conversation_watch("lead");
    let worker = worker_of(&context, &supervisor).await;
    report_done(&context, &worker, done(protocol::Outcome::Failed, "stuck"))
        .await
        .unwrap();

    let refused = report_done(&context, &worker, done(protocol::Outcome::Succeeded, "ok"))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    let count = context
        .call(move |store| Ok(store.messages_of(supervisor.id)?.len()))
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn waiting_answers_with_the_oldest_message_and_takes_it_for_good() {
    let (_dir, context, supervisor, _) = conversation_watch("lead");
    let worker = worker_of(&context, &supervisor).await;
    report_done(
        &context,
        &worker,
        done(protocol::Outcome::Succeeded, "Fixed it"),
    )
    .await
    .unwrap();

    let first = wait(
        &context,
        &supervisor,
        protocol::Wait {
            timeout_seconds: Some(0),
        },
    )
    .await
    .unwrap();
    let again = wait(
        &context,
        &supervisor,
        protocol::Wait {
            timeout_seconds: Some(0),
        },
    )
    .await
    .unwrap();

    assert_eq!(first["timed_out"], false);
    assert_eq!(first["message"]["from"], json!(worker.id));
    assert!(
        first["message"]["content"]
            .as_str()
            .unwrap()
            .contains("Fixed it")
    );
    assert_eq!(again["timed_out"], true);
    assert_eq!(again["message"], Value::Null);
}

#[tokio::test]
async fn a_session_waiting_is_woken_by_a_report_made_meanwhile() {
    let (_dir, context, supervisor, _) = conversation_watch("lead");
    let worker = worker_of(&context, &supervisor).await;
    let waiting = {
        let (context, supervisor) = (context.clone(), supervisor.clone());
        tokio::spawn(async move {
            wait(
                &context,
                &supervisor,
                protocol::Wait {
                    timeout_seconds: Some(20),
                },
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;

    report_done(
        &context,
        &worker,
        done(protocol::Outcome::Failed, "no luck"),
    )
    .await
    .unwrap();

    let woken = waiting.await.unwrap().unwrap();
    assert_eq!(woken["timed_out"], false);
    assert!(
        woken["message"]["content"]
            .as_str()
            .unwrap()
            .contains("outcome: failed")
    );
}

#[test]
fn a_worker_is_told_who_supervises_it_and_to_report_when_done() {
    let supervisor = record("lead").build();

    let brief = supervised_prompt("Fix the flaky test", &supervisor);

    assert!(brief.starts_with("Fix the flaky test"), "{brief}");
    assert!(brief.contains("report_done"), "{brief}");
    assert!(brief.contains(supervisor.code.as_str()), "{brief}");
    assert!(brief.contains("user"), "{brief}");
}

#[test]
fn a_start_supervises_only_when_asked_to() {
    let start = |value: Value| serde_json::from_value::<protocol::Start>(value).unwrap();

    assert!(!start(json!({"prompt": "go"})).supervised);
    assert!(start(json!({"prompt": "go", "supervised": true})).supervised);
}

/// A session `caller` opened, running in `terminal` and working in `cwd`.
async fn opened_by(context: &Context, caller: &Session, cwd: &Path, terminal: u128) -> Session {
    let (opener, cwd) = (caller.id, cwd.to_path_buf());
    context
        .call(move |store| {
            let (id, _) = store.register(crate::delegation::store::NewSession {
                parent: Some(opener),
                agent: tsumugi_model::Agent::Claude,
                profile: tsumugi_model::profile::ProfileId::builtin(tsumugi_model::Agent::Claude),
                home: None,
                name: "child".into(),
                cwd,
                launch: Settings::default(),
                model: None,
                permissions: Default::default(),
                started_ms: 1,
                name_given: false,
                code: None,
            })?;
            store.attach(id, terminal, None)?;
            store.known(id)
        })
        .await
        .unwrap()
}

fn target(session: &Session) -> protocol::CloseSession {
    protocol::CloseSession {
        target: Some(session.code.to_string()),
        force: false,
    }
}

fn private_dir() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("tsumugi-")
        .tempdir_in(tsumugi_sys::local::temp_dir())
        .unwrap();
    tsumugi_sys::fs::set_mode(dir.path(), 0o700).unwrap();
    dir
}

async fn runs(daemon: &Daemon, terminal: u128) -> bool {
    daemon
        .sessions()
        .await
        .unwrap()
        .iter()
        .any(|s| s.id == terminal && !s.exited)
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs its terminal's program with /bin/sh")]
async fn a_session_ends_a_session_it_opened_and_leaves_a_note_the_user_reads_in_both() {
    let dir = private_dir();
    let (_state, mut context, caller, _) = conversation_watch("lead");
    let (daemon, terminal) = serving(dir.path()).await;
    context.daemon = daemon.clone();
    let child = opened_by(&context, &caller, dir.path(), terminal).await;
    let worker = child.id;
    context
        .call(move |store| store.supervise(worker))
        .await
        .unwrap();

    let answered = close(&context, &caller, target(&child)).await.unwrap();

    assert_eq!(answered["closed"], json!(child.id));
    assert_eq!(answered["code"], child.code.as_str());
    assert!(!runs(&daemon, terminal).await);
    let (id, caller_id) = (child.id, caller.id);
    let (ended, notes, told) = context
        .call(move |store| {
            store.follow_workers(&[])?;
            Ok((
                store.known(id)?.ended,
                store.messages_of(id)?,
                store.messages_of(caller_id)?,
            ))
        })
        .await
        .unwrap();
    assert!(ended);
    assert!(
        notes
            .iter()
            .any(|m| m.sender == Some(caller.id) && m.delivered && m.body.contains("closed")),
        "{notes:?}"
    );
    assert!(
        told.iter().all(|m| !m.body.contains("worker_stopped")),
        "a supervisor is not told of the end it made itself: {told:?}"
    );
}

#[tokio::test]
async fn a_session_cannot_end_one_the_user_or_another_session_opened() {
    let (_state, context, caller, _) = conversation_watch("lead");
    let (users, others) = context
        .call(|store| {
            let new = |parent, name: &str| crate::delegation::store::NewSession {
                parent,
                agent: tsumugi_model::Agent::Claude,
                profile: tsumugi_model::profile::ProfileId::builtin(tsumugi_model::Agent::Claude),
                home: None,
                name: name.into(),
                cwd: "/repo".into(),
                launch: Settings::default(),
                model: None,
                permissions: Default::default(),
                started_ms: 1,
                name_given: false,
                code: None,
            };
            let (users, _) = store.register(new(None, "users"))?;
            let (second, _) = store.register(new(None, "second"))?;
            let (others, _) = store.register(new(Some(second), "others"))?;
            Ok((store.known(users)?, store.known(others)?))
        })
        .await
        .unwrap();

    for session in [&users, &others] {
        let refused = close(&context, &caller, target(session)).await.unwrap_err();
        assert_eq!(refused.code, Code::Refused, "{}", session.name);
        assert!(refused.message.contains("opened"), "{}", refused.message);
    }
    let all = context.call(|store| store.sessions()).await.unwrap();
    assert!(all.iter().all(|s| !s.ended));
}

#[tokio::test]
async fn a_session_that_does_not_exist_or_ended_already_is_named_as_such() {
    let (_state, context, caller, _) = conversation_watch("lead");
    let child = opened_by(&context, &caller, Path::new("/repo"), 7).await;
    let id = child.id;
    context.call(move |store| store.end(id)).await.unwrap();

    let unknown = close(
        &context,
        &caller,
        protocol::CloseSession {
            target: Some("zzzz".into()),
            force: false,
        },
    )
    .await
    .unwrap_err();
    let ended = close(&context, &caller, target(&child)).await.unwrap_err();

    assert_eq!(unknown.code, Code::NotFound);
    assert_eq!(ended.code, Code::Refused);
    assert!(ended.message.contains("already"), "{}", ended.message);
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs its terminal's program with /bin/sh")]
async fn uncommitted_changes_refuse_the_end_of_a_session_until_it_is_forced() {
    let dir = private_dir();
    let work = tempfile::tempdir().unwrap();
    let repo = crate::testing::repository(work.path());
    std::fs::write(repo.join("draft.txt"), "unsaved").unwrap();
    let (_state, mut context, caller, _) = conversation_watch("lead");
    let (daemon, terminal) = serving(dir.path()).await;
    context.daemon = daemon.clone();
    let child = opened_by(&context, &caller, &repo, terminal).await;

    let refused = close(&context, &caller, target(&child)).await.unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(
        refused.message.contains("uncommitted"),
        "{}",
        refused.message
    );
    assert!(refused.message.contains("force"), "{}", refused.message);
    assert!(runs(&daemon, terminal).await);
    let forced = protocol::CloseSession {
        force: true,
        ..target(&child)
    };
    close(&context, &caller, forced).await.unwrap();
    assert!(!runs(&daemon, terminal).await);
}

#[test]
fn what_ending_a_session_may_lose_is_its_work_in_hand_and_its_unsaved_changes() {
    use tsumugi_model::{State, Wait};
    assert!(losses(Some(&State::Idle), 0).is_empty());
    assert!(losses(None, 0).is_empty());
    assert!(losses(Some(&State::Done), 0).is_empty());
    assert!(losses(Some(&State::Working), 0)[0].contains("working"));
    assert!(losses(Some(&State::Waiting(Wait::Permission)), 0)[0].contains("waits"));
    let both = losses(Some(&State::Working), 2);
    assert_eq!(both.len(), 2);
    assert!(both[1].contains("2 uncommitted"), "{both:?}");
}

fn ended_with(name: &str, conversation: &str) -> Session {
    record(name).conversation(conversation).ended().build()
}

#[test]
fn an_ended_session_is_found_to_resume_by_its_code_its_id_or_its_conversation() {
    let old = ended_with("old", "c-1");
    let all = [old.clone(), record("other").build()];

    for wanted in [old.code.to_string(), old.id.to_string(), "c-1".to_string()] {
        assert_eq!(resumption(&all, &wanted).unwrap().id, old.id, "{wanted}");
    }
    assert_eq!(resumption(&all, "nobody").unwrap_err().code, Code::NotFound);
}

#[test]
fn a_session_that_runs_a_conversation_is_not_resumed_a_second_time() {
    let running = record("running").conversation("c-1").build();
    let ended = ended_with("ended", "c-1");

    let by_code = resumption(std::slice::from_ref(&running), running.code.as_str()).unwrap_err();
    let by_conversation = resumption(&[ended.clone(), running], "c-1").unwrap_err();
    let none = resumption(&[record("none").ended().build()], "none").unwrap_err();

    assert_eq!(by_code.code, Code::Refused);
    assert!(by_code.message.contains("runs"), "{}", by_code.message);
    assert_eq!(by_conversation.code, Code::Refused);
    assert_eq!(none.code, Code::NotFound);
}

#[test]
fn a_session_ended_without_a_conversation_cannot_be_resumed() {
    let old = record("old").ended().build();

    let refused = resumption(std::slice::from_ref(&old), old.code.as_str()).unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(
        refused.message.contains("conversation"),
        "{}",
        refused.message
    );
}

#[test]
fn a_resumed_session_runs_where_the_old_one_ran_as_the_old_one_was_started() {
    let mut old = ended_with("old", "c-1");
    old.agent = tsumugi_model::Agent::Codex;
    old.profile = tsumugi_model::profile::ProfileId::builtin(tsumugi_model::Agent::Codex);
    old.home = Some("/login".into());
    old.cwd = "/repo/wt".into();
    old.model = Some("m".into());
    old.name_given = true;

    let request = resumed_request(&old, "old-2".into());

    assert_eq!(request.resume.as_deref(), Some("c-1"));
    assert_eq!(request.agent, old.agent);
    assert_eq!(request.profile.as_ref(), Some(&old.profile));
    assert_eq!(request.home, old.home);
    assert_eq!(request.cwd, old.cwd);
    assert_eq!(request.model, old.model);
    assert_eq!(request.name, "old-2");
    assert!(request.name_given);
    assert!(request.code.is_none());
}

#[tokio::test]
async fn a_resume_names_no_place_of_its_own_and_one_that_cannot_be_found_leaves_nothing() {
    let (_state, context, caller, _) = conversation_watch("lead");
    let request = |value: Value| serde_json::from_value::<protocol::Start>(value).unwrap();

    for (field, value) in [
        ("dir", json!("/repo")),
        ("worktree", json!(true)),
        ("agent", json!("codex")),
        ("profile", json!("claude")),
    ] {
        let mut asked = json!({"resume": "zzzz"});
        asked[field] = value;
        let refused = start(&context, &caller, request(asked)).await.unwrap_err();
        assert_eq!(refused.code, Code::Invalid, "{field}");
    }
    let missing = start(&context, &caller, request(json!({"resume": "zzzz"})))
        .await
        .unwrap_err();

    assert_eq!(missing.code, Code::NotFound);
    assert_eq!(context.call(|s| s.sessions()).await.unwrap().len(), 1);
}

#[test]
fn an_issue_is_worked_from_its_own_text_followed_by_what_the_opener_adds() {
    let issue = tsumugi_model::LocalIssue {
        id: "a".into(),
        dir: "/repo".into(),
        title: "Fix login".into(),
        body: "It fails.".into(),
        state: tsumugi_model::LocalIssueState::Open,
        record: None,
        session: None,
        error: None,
        schedule: None,
    };

    let alone = issue_prompt(41, &issue, "");
    let added = issue_prompt(41, &issue, "Start with the tests.");

    assert!(alone.contains("#41"), "{alone}");
    assert!(alone.contains("Fix login") && alone.contains("It fails."));
    assert!(added.starts_with(&alone), "{added}");
    assert!(added.ends_with("Start with the tests."), "{added}");
}

#[tokio::test]
async fn an_issue_that_does_not_exist_or_cannot_be_worked_on_refuses_the_start_and_leaves_nothing()
{
    let (dir, context, caller, _) = conversation_watch("lead");
    let id = saved(&context, &caller, "later", dir.path()).await;
    context
        .call(move |store| store.close_local_issue(&id))
        .await
        .unwrap();
    let request = |issue: &str| {
        serde_json::from_value::<protocol::Start>(json!({"issue": issue, "background": true}))
            .unwrap()
    };

    let missing = start(&context, &caller, request("99")).await.unwrap_err();
    let closed = start(&context, &caller, request("1")).await.unwrap_err();

    assert_eq!(missing.code, Code::NotFound);
    assert_eq!(closed.code, Code::Refused);
    assert_eq!(context.call(|s| s.sessions()).await.unwrap().len(), 1);
}

/// A terminal host with one agent session of Claude Code running in `dir`,
/// and the private folder its socket is in.
async fn serving_an_agent(dir: &Path) -> (Daemon, tempfile::TempDir) {
    let sockets = tempfile::Builder::new()
        .prefix("tsumugi-")
        .tempdir_in(tsumugi_sys::local::temp_dir())
        .unwrap();
    tsumugi_sys::fs::set_mode(sockets.path(), 0o700).unwrap();
    let socket = sockets.path().join("agents.sock");
    let serving = socket.clone();
    crate::environment::spawn(async move {
        Daemon::serve(&serving, Duration::from_millis(20), |_| {})
            .await
            .unwrap()
    });
    let daemon = Daemon::new(socket);
    for _ in 0..100 {
        if daemon.sessions().await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    daemon
        .spawn(tsumugi_term::Spawn {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "read answer".into()],
            cwd: dir.into(),
            env: Vec::new(),
            tags: crate::sessions::agent_tags(
                tsumugi_model::Agent::Claude,
                "busy",
                dir,
                "",
                &tsumugi_model::AgentPermissions::Default,
                1,
            ),
            cols: 80,
            rows: 24,
        })
        .await
        .unwrap();
    (daemon, sockets)
}

fn plain_start(dir: &Path) -> protocol::Start {
    serde_json::from_value(serde_json::json!({
        "prompt": "review the change",
        "dir": dir,
    }))
    .unwrap()
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs its terminal's program with /bin/sh")]
async fn a_start_beyond_the_most_sessions_is_refused_with_how_many_run_and_leaves_nothing() {
    let (dir, mut context, caller, _) = conversation_watch("review");
    let (daemon, _sockets) = serving_an_agent(dir.path()).await;
    context.daemon = daemon;
    context.settings = Arc::new(|| {
        Ok(Settings::parse("[governor]\nmax_sessions = 1\nwait_seconds = 0\n").unwrap())
    });
    let work = dir.path().join("repo");
    std::fs::create_dir(&work).unwrap();

    let refused = start(&context, &caller, plain_start(&work))
        .await
        .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(refused.message.contains("1 of 1"), "{}", refused.message);
    let sessions = context.call(|store| store.sessions()).await.unwrap();
    assert_eq!(sessions.len(), 1);
}

#[tokio::test]
#[cfg_attr(windows, ignore = "runs its terminal's program with /bin/sh")]
async fn with_no_limit_set_the_same_start_is_not_held_by_the_running_session() {
    let (dir, mut context, caller, _) = conversation_watch("review");
    let (daemon, _sockets) = serving_an_agent(dir.path()).await;
    context.daemon = daemon;
    context.settings = Arc::new(|| Ok(Settings::parse("[governor]\nwait_seconds = 0\n").unwrap()));
    let work = dir.path().join("repo");
    std::fs::create_dir(&work).unwrap();

    let result = start(&context, &caller, plain_start(&work)).await;

    // It goes on to start its agent, which this test has no program for; what
    // matters is that no limit refused it.
    if let Err(error) = result {
        assert!(!error.message.contains("of 0"), "{}", error.message);
        assert!(!error.message.contains("agent sessions run"));
    }
}

#[tokio::test]
async fn a_gate_is_asked_only_where_the_user_set_a_command_for_the_repository() {
    let (dir, context, caller, _) = conversation_watch("review");

    let refused = request_gate(&context, &caller, protocol::RequestGate::default())
        .await
        .unwrap_err();

    // The directory is no repository, or has no gate command: either way no
    // command of the repository's own is run.
    assert!(matches!(refused.code, Code::Invalid | Code::Refused));
    assert!(dir.path().exists());
}

#[tokio::test]
async fn a_gate_for_a_repository_without_a_command_says_where_the_user_sets_one() {
    let (dir, context, caller, _) = conversation_watch("review");
    let primary = dir.path().join("api");
    std::fs::create_dir(&primary).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&primary)
            .status()
            .unwrap()
            .success()
    );

    let refused = request_gate(
        &context,
        &caller,
        protocol::RequestGate {
            dir: Some(primary),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();

    assert_eq!(refused.code, Code::Refused);
    assert!(
        refused.message.contains("[[gate.repositories]]"),
        "{}",
        refused.message
    );
}

#[test]
fn asking_for_the_gate_and_for_a_cleanup_act_for_the_session() {
    assert!(Command::RequestGate(Default::default()).acts());
    assert!(Command::CleanupWorktrees(Default::default()).acts());
    assert!(Command::MergeWorktree(Default::default()).acts());
}

fn tiered(tier: &str) -> protocol::Start {
    serde_json::from_value(json!({"prompt": "go", "tier": tier})).unwrap()
}

fn with_settings(context: &mut Context, text: &'static str) {
    context.settings = Arc::new(move || Ok(Settings::parse(text).unwrap()));
}

#[tokio::test]
async fn a_start_by_a_tier_the_user_has_not_defined_fails_and_starts_nothing_else() {
    let (_state, mut context, caller, _) = conversation_watch("lead");
    with_settings(&mut context, "[worker_tiers.light]\nmodel = 'haiku'\n");

    let refused = start(&context, &caller, tiered("heavy")).await.unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
    assert!(
        refused.message.contains("`heavy` is not defined"),
        "{}",
        refused.message
    );
    assert!(
        refused.message.contains("[worker_tiers]"),
        "{}",
        refused.message
    );
    assert_eq!(context.call(|s| s.sessions()).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_tier_naming_a_profile_that_is_gone_fails_naming_both() {
    let (_state, mut context, caller, _) = conversation_watch("lead");
    with_settings(
        &mut context,
        "[worker_tiers.light]\nprofile = 'gone'\nmodel = 'haiku'\n",
    );

    let refused = start(&context, &caller, tiered("light")).await.unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
    assert!(refused.message.contains("`light`"), "{}", refused.message);
    assert!(refused.message.contains("`gone`"), "{}", refused.message);
    assert_eq!(context.call(|s| s.sessions()).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_tier_whose_model_its_profile_does_not_list_fails_rather_than_starting_another() {
    let (_state, mut context, caller, _) = conversation_watch("lead");
    with_settings(
        &mut context,
        "[worker_tiers.light]\nprofile = 'claude'\nmodel = 'gpt-5'\n",
    );
    {
        let mut models = context.models.lock().unwrap();
        let claude = tsumugi_model::profile::ProfileId::builtin(tsumugi_model::Agent::Claude);
        models.configure(
            vec![tsumugi_model::Asked {
                profile: claude.clone(),
                agent: tsumugi_model::Agent::Claude,
                home: None,
                through: "x".into(),
            }],
            claude,
        );
        let generation = models.to_ask()[0].1;
        models.learn(
            generation,
            vec![tsumugi_model::ProviderModel {
                id: "haiku".into(),
                resolved: None,
                name: "Haiku".into(),
                description: String::new(),
            }],
        );
    }

    let refused = start(&context, &caller, tiered("light")).await.unwrap_err();

    assert_eq!(refused.code, Code::Invalid);
    assert!(
        refused.message.contains("worker tier `light`"),
        "{}",
        refused.message
    );
    assert!(refused.message.contains("`gpt-5`"), "{}", refused.message);
    assert!(refused.message.contains("haiku"), "{}", refused.message);
    assert_eq!(context.call(|s| s.sessions()).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_tier_stands_in_for_the_agent_profile_and_model_and_for_a_resume() {
    let (_state, mut context, caller, _) = conversation_watch("lead");
    with_settings(&mut context, "[worker_tiers.light]\nmodel = 'haiku'\n");

    for (field, value) in [
        ("agent", json!("codex")),
        ("profile", json!("claude")),
        ("model", json!("opus")),
        ("resume", json!("zzzz")),
    ] {
        let mut asked = json!({"prompt": "go", "tier": "light"});
        asked[field] = value;
        let refused = start(&context, &caller, serde_json::from_value(asked).unwrap())
            .await
            .unwrap_err();
        assert_eq!(refused.code, Code::Invalid, "{field}");
        assert!(
            refused.message.contains("tier"),
            "{field}: {}",
            refused.message
        );
    }
}

#[test]
fn a_tier_is_started_by_the_profile_and_model_the_settings_give_it() {
    let settings = Settings::parse(
        "[worker_tiers.light]\nmodel = 'haiku'\n[worker_tiers.heavy]\nprofile = 'codex'\nmodel = 'gpt-5'\n",
    )
    .unwrap();

    let light = with_tier(&settings, tiered("light")).unwrap();
    assert_eq!(light.profile, None);
    assert_eq!(light.model.as_deref(), Some("haiku"));
    let heavy = with_tier(&settings, tiered("heavy")).unwrap();
    assert_eq!(heavy.profile.map(|p| p.to_string()), Some("codex".into()));
    assert_eq!(heavy.model.as_deref(), Some("gpt-5"));
    let plain = with_tier(&settings, plain_start(Path::new("/r"))).unwrap();
    assert_eq!((plain.profile, plain.model), (None, None));
}

#[test]
fn a_tier_with_no_model_starts_its_profile_s_own_default_model() {
    let settings = Settings::parse("[worker_tiers.standard]\nprofile = 'codex'\n").unwrap();
    let standard = with_tier(&settings, tiered("standard")).unwrap();
    assert_eq!(standard.model, None);
}
