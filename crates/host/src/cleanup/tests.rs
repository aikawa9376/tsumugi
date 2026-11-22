use std::process::Command;

use super::*;

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

fn repository(dir: &Path) -> PathBuf {
    let primary = std::fs::canonicalize(dir).unwrap().join("api");
    std::fs::create_dir(&primary).unwrap();
    git(&primary, &["init", "-q", "-b", "main"]);
    std::fs::write(primary.join("a.txt"), "a").unwrap();
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-q", "-m", "first"]);
    primary
}

async fn worktree(primary: &Path, name: &str) -> PathBuf {
    tsumugi_vcs::worktree::create(primary, name, None)
        .await
        .unwrap()
        .path
}

fn candidate<'a>(found: &'a [Candidate], path: &Path) -> &'a Candidate {
    found.iter().find(|c| c.path == path).unwrap()
}

fn nowhere() -> PathBuf {
    PathBuf::from("/nowhere")
}

#[test]
fn a_worktree_is_kept_for_each_thing_it_would_lose() {
    let clean = WorktreeStanding {
        branch: Some("work".into()),
        ..Default::default()
    };
    assert_eq!(reasons(&clean, false), []);
    assert_eq!(reasons(&clean, true), [Why::InUse]);
    let busy = WorktreeStanding {
        changed: 2,
        unpushed: 1,
        unmerged: 3,
        locked: true,
        ..clean.clone()
    };
    assert_eq!(
        reasons(&busy, false),
        [
            Why::Locked,
            Why::Changed(2),
            Why::Unpushed(1),
            Why::Unmerged(3)
        ]
    );
    let detached = WorktreeStanding::default();
    assert_eq!(reasons(&detached, false), [Why::Detached]);
    let main = WorktreeStanding {
        main: true,
        ..Default::default()
    };
    assert_eq!(reasons(&main, false), [Why::Main]);
}

#[tokio::test]
async fn a_worktree_with_nothing_to_lose_and_no_session_can_go_and_the_main_checkout_cannot() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let done = worktree(&primary, "done").await;

    let found = scan(&primary, &[], &nowhere()).await.unwrap();

    assert!(candidate(&found, &done).removable());
    assert_eq!(candidate(&found, &primary).kept, [Why::Main]);
    assert!(candidate(&found, &done).bytes > 0);
}

#[tokio::test]
async fn changes_unmerged_commits_or_a_session_keep_a_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let dirty = worktree(&primary, "dirty").await;
    std::fs::write(dirty.join("new.txt"), "x").unwrap();
    let ahead = worktree(&primary, "ahead").await;
    std::fs::write(ahead.join("b.txt"), "b").unwrap();
    git(&ahead, &["add", "."]);
    git(&ahead, &["commit", "-q", "-m", "work"]);
    let working = worktree(&primary, "working").await;

    let found = scan(&primary, &[working.join("src")], &nowhere())
        .await
        .unwrap();

    assert_eq!(candidate(&found, &dirty).kept, [Why::Changed(1)]);
    assert_eq!(candidate(&found, &ahead).kept, [Why::Unmerged(1)]);
    assert_eq!(candidate(&found, &working).kept, [Why::InUse]);
}

#[tokio::test]
async fn a_merged_branch_is_a_candidate_once_the_main_checkout_holds_its_commits() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let done = worktree(&primary, "merged").await;
    std::fs::write(done.join("b.txt"), "b").unwrap();
    git(&done, &["add", "."]);
    git(&done, &["commit", "-q", "-m", "work"]);
    let branch = git(&done, &["branch", "--show-current"]);
    git(&primary, &["merge", "-q", "--ff-only", &branch]);

    let found = scan(&primary, &[], &nowhere()).await.unwrap();

    assert!(candidate(&found, &done).removable());
}

#[tokio::test]
async fn the_worktrees_under_the_skipped_folder_are_not_listed() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let gate = dir.path().join("gate");
    tsumugi_vcs::worktree::place_gate(&primary, &gate.join("api"), "HEAD")
        .await
        .unwrap();

    let found = scan(&primary, &[], &gate).await.unwrap();

    assert_eq!(found.len(), 1);
}

#[tokio::test]
async fn sweeping_removes_only_what_can_go_and_keeps_every_branch_unless_told() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let done = worktree(&primary, "done").await;
    let dirty = worktree(&primary, "dirty").await;
    std::fs::write(dirty.join("new.txt"), "x").unwrap();
    let found = scan(&primary, &[], &nowhere()).await.unwrap();

    let swept = sweep(&found, false).await;

    assert_eq!(swept.len(), 1);
    assert_eq!(swept[0].path, done);
    assert_eq!(swept[0].error, None);
    assert!(!done.exists());
    assert!(dirty.join("new.txt").exists());
    assert!(git(&primary, &["branch", "--list"]).contains("done"));
}

#[tokio::test]
async fn a_merged_branch_goes_with_its_worktree_when_the_user_asked_for_branches() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let done = worktree(&primary, "done").await;
    let found = scan(&primary, &[], &nowhere()).await.unwrap();

    let swept = sweep(&found, true).await;

    assert_eq!(swept[0].branch_kept, None);
    assert!(!done.exists());
    assert!(!git(&primary, &["branch", "--list"]).contains("done"));
}

#[tokio::test]
async fn a_worktree_changed_after_the_scan_is_refused_by_git_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let done = worktree(&primary, "done").await;
    let found = scan(&primary, &[], &nowhere()).await.unwrap();
    std::fs::write(done.join("late.txt"), "late").unwrap();

    let swept = sweep(&found, true).await;

    assert!(swept[0].error.is_some());
    assert!(done.join("late.txt").exists());
}

#[tokio::test]
async fn the_answer_lists_what_stays_with_why_and_never_the_main_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let primary = repository(dir.path());
    let done = worktree(&primary, "done").await;
    let dirty = worktree(&primary, "dirty").await;
    std::fs::write(dirty.join("new.txt"), "x").unwrap();
    let found = scan(&primary, &[], &nowhere()).await.unwrap();

    let told = told(&found, None);

    assert_eq!(told["removable"][0]["path"], done.to_str().unwrap());
    assert_eq!(told["kept"].as_array().unwrap().len(), 1);
    assert_eq!(told["kept"][0]["why"][0], "1 changed files");
    assert!(told["removed"].is_null());
}
