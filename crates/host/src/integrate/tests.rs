use std::process::Command;

use tsumugi_model::profile::ProfileId;
use tsumugi_model::{Agent, LocalIssueState, NewLocalIssue, SessionId};

use super::*;
use crate::delegation::store::{NewSession, Store};
use crate::settings::Settings;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", name]);
}

struct Work {
    _dir: tempfile::TempDir,
    state: PathBuf,
    main: PathBuf,
    feature: PathBuf,
}

/// A repository with the worktree `feature`, and an issue a session works on
/// in that worktree.
async fn work() -> Work {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let main = root.join("api");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    commit(&main, "shared.txt", "one\n");
    let feature = tsumugi_vcs::worktree::create(&main, "feature", None)
        .await
        .unwrap()
        .path;
    let state = root.join("state");
    std::fs::create_dir(&state).unwrap();
    let mut store = Store::open(&crate::delegation::app::database(&state)).unwrap();
    let new = NewLocalIssue {
        id: SessionId::new().to_string(),
        dir: main.clone(),
        title: "Do the thing".into(),
        body: "body".into(),
    };
    store.add_local_issue(new).unwrap();
    let (_, begun) = store.begin_local_issue("1").unwrap();
    let record = store
        .register(NewSession {
            parent: None,
            agent: Agent::Claude,
            profile: ProfileId::builtin(Agent::Claude),
            home: None,
            name: "worker".into(),
            cwd: feature.clone(),
            launch: Settings::default(),
            model: None,
            permissions: tsumugi_model::AgentPermissions::Default,
            started_ms: 1,
            name_given: true,
            code: None,
        })
        .unwrap()
        .0;
    store.attach_local_issue(&begun.id, record).unwrap();
    Work {
        _dir: dir,
        state,
        main,
        feature,
    }
}

fn issue_state(state: &Path) -> LocalIssueState {
    let store = Store::open(&crate::delegation::app::database(state)).unwrap();
    store.local_issue_named("1").unwrap().1.state
}

#[tokio::test]
async fn the_standing_names_the_issue_the_worktree_s_session_works_on() {
    let w = work().await;
    commit(&w.feature, "new.txt", "new\n");

    let standing = standing_in(&w.state, &w.feature).await.unwrap();

    let issue = standing.issue.unwrap();
    assert_eq!((issue.number, issue.title.as_str()), (1, "Do the thing"));
    assert_eq!(standing.ahead, 1);
}

#[tokio::test]
async fn a_merge_closes_the_issue_it_finishes_when_asked() {
    let w = work().await;
    commit(&w.feature, "new.txt", "new\n");

    let merged = merge_in(&w.state, &w.feature, MainChanges::Refuse, None, true)
        .await
        .unwrap();

    let Merged::Done { closed, .. } = merged else {
        panic!("{merged:?}");
    };
    assert_eq!(closed.unwrap().number, 1);
    assert_eq!(issue_state(&w.state), LocalIssueState::Closed);
}

#[tokio::test]
async fn merging_one_attempt_closes_the_issue_and_names_the_others_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let main = root.join("api");
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    commit(&main, "shared.txt", "one\n");
    let base = git(&main, &["rev-parse", "HEAD"]);
    let mut worktrees = Vec::new();
    for name in ["try-1", "try-2", "try-3"] {
        let made = tsumugi_vcs::worktree::create(&main, name, Some(&base))
            .await
            .unwrap();
        worktrees.push(made.path);
    }
    let state = root.join("state");
    std::fs::create_dir(&state).unwrap();
    let mut store = Store::open(&crate::delegation::app::database(&state)).unwrap();
    let id = SessionId::new().to_string();
    store
        .add_local_issue(NewLocalIssue {
            id: id.clone(),
            dir: main.clone(),
            title: "Try it three ways".into(),
            body: "body".into(),
        })
        .unwrap();
    let tries: Vec<_> = ["claude", "codex", "pi"]
        .iter()
        .map(|word| tsumugi_model::TryAs::parse(word).unwrap())
        .collect();
    store.begin_attempts(&id, &tries, &base).unwrap();
    for (number, worktree) in (1..).zip(&worktrees) {
        let record = store
            .register(NewSession {
                parent: None,
                agent: Agent::Claude,
                profile: ProfileId::builtin(Agent::Claude),
                home: None,
                name: format!("try {number}"),
                cwd: worktree.clone(),
                launch: Settings::default(),
                model: None,
                permissions: tsumugi_model::AgentPermissions::Default,
                started_ms: 1,
                name_given: true,
                code: None,
            })
            .unwrap()
            .0;
        store.attach_attempt(&id, number, record).unwrap();
    }
    drop(store);
    commit(&worktrees[1], "chosen.txt", "chosen\n");

    let merged = merge_in(&state, &worktrees[1], MainChanges::Refuse, None, true)
        .await
        .unwrap();

    let Merged::Done { closed, .. } = merged else {
        panic!("{merged:?}");
    };
    let closed = closed.unwrap();
    assert_eq!(closed.others, [worktrees[0].clone(), worktrees[2].clone()]);
    assert_eq!(issue_state(&state), LocalIssueState::Closed);
}

#[tokio::test]
async fn a_merge_leaves_the_issue_alone_unless_asked_to_close_it() {
    let w = work().await;
    commit(&w.feature, "new.txt", "new\n");

    let merged = merge_in(&w.state, &w.feature, MainChanges::Refuse, None, false)
        .await
        .unwrap();

    let Merged::Done { closed, .. } = merged else {
        panic!("{merged:?}");
    };
    assert!(closed.is_none());
    assert_eq!(issue_state(&w.state), LocalIssueState::Starting);
}

#[tokio::test]
async fn a_merge_that_conflicts_or_is_refused_closes_nothing() {
    let w = work().await;
    commit(&w.main, "shared.txt", "main\n");
    commit(&w.feature, "shared.txt", "feature\n");

    let merged = merge_in(&w.state, &w.feature, MainChanges::Refuse, None, true)
        .await
        .unwrap();
    assert!(matches!(merged, Merged::Conflicts { .. }));
    assert_eq!(issue_state(&w.state), LocalIssueState::Starting);

    std::fs::write(w.main.join("stray.txt"), "mine").unwrap();
    git(&w.feature, &["reset", "-q", "--hard", "HEAD~1"]);
    commit(&w.feature, "new.txt", "new\n");
    let refused = merge_in(&w.state, &w.feature, MainChanges::Refuse, None, true)
        .await
        .unwrap_err();
    assert!(refused.contains("uncommitted"), "{refused}");
    assert_eq!(issue_state(&w.state), LocalIssueState::Starting);
}

#[tokio::test]
async fn the_conflict_prompt_names_the_branches_and_the_files() {
    let told = conflict_prompt("feature", "main", &["a.rs".into(), "b/c.rs".into()]);

    assert!(told.contains("main has been merged into your branch feature"));
    assert!(told.contains("- a.rs\n- b/c.rs\n"));
    assert!(told.contains("Do not touch the main checkout"));
}

#[tokio::test]
async fn a_merged_worktree_goes_with_its_branch_unless_something_keeps_it() {
    let w = work().await;
    commit(&w.feature, "new.txt", "new\n");
    merge_in(&w.state, &w.feature, MainChanges::Refuse, None, false)
        .await
        .unwrap();
    let kept = {
        std::fs::write(w.feature.join("stray.txt"), "x").unwrap();
        let kept = remove_unused(&[], &w.feature).await;
        std::fs::remove_file(w.feature.join("stray.txt")).unwrap();
        kept
    };
    assert!(!kept.removed && kept.why.unwrap().contains("changed"));
    assert!(w.feature.is_dir());

    let removed = remove_unused(&[], &w.feature).await;

    assert!(removed.removed, "{removed:?}");
    assert!(!w.feature.exists());
    assert_eq!(git(&w.main, &["branch", "--list", "feature"]), "");
}

#[tokio::test]
async fn a_worktree_a_session_works_in_is_not_deleted() {
    let w = work().await;
    commit(&w.feature, "new.txt", "new\n");
    merge_in(&w.state, &w.feature, MainChanges::Refuse, None, false)
        .await
        .unwrap();

    let kept = remove_unused(std::slice::from_ref(&w.feature), &w.feature).await;

    assert!(!kept.removed && kept.why.unwrap().contains("session"));
    assert!(w.feature.is_dir());
}
