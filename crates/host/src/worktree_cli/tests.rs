use std::process::Command;

use super::*;

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn a_merge_names_a_worktree_by_directory_or_branch_and_says_what_to_do_with_main_s_changes() {
    assert_eq!(parse(&words(&["merge"])), Some(Asked::default()));
    let asked = parse(&words(&[
        "merge",
        "--branch",
        "issue-44",
        "--stash",
        "--message",
        "treewide: merge issue-44",
        "--close-issue",
        "--remove",
    ]))
    .unwrap();
    assert_eq!(asked.branch.as_deref(), Some("issue-44"));
    assert_eq!(asked.changes, MainChanges::Stash);
    assert_eq!(asked.message.as_deref(), Some("treewide: merge issue-44"));
    assert!(asked.close_issue && asked.remove && !asked.check);
    assert_eq!(
        parse(&words(&["merge", "--dir", "../wt", "--commit"]))
            .unwrap()
            .changes,
        MainChanges::Commit
    );
}

#[test]
fn main_s_changes_are_left_alone_unless_a_choice_is_given() {
    assert_eq!(
        parse(&words(&["merge"])).unwrap().changes,
        MainChanges::Refuse
    );
}

#[test]
fn what_a_merge_does_not_take_is_refused() {
    for bad in [
        &["merge", "--stash", "--commit"][..],
        &["merge", "--dir", "a", "--branch", "b"],
        &["merge", "--check", "--stash"],
        &["merge", "--check", "--remove"],
        &["merge", "--check", "--resolve"],
        &["merge", "--resolve", "--close-issue"],
        &["merge", "--force"],
        &["merge", "--dir"],
        &["rebase"],
        &[],
    ] {
        assert_eq!(parse(&words(bad)), None, "{bad:?}");
    }
}

fn git(dir: &Path, args: &[&str]) {
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
}

fn commit(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", name]);
}

async fn repository() -> (tempfile::TempDir, PathBuf, PathBuf) {
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
    (dir, main, feature)
}

#[tokio::test]
async fn a_check_reads_how_the_merge_stands_and_changes_nothing() {
    let (dir, main, feature) = repository().await;
    commit(&feature, "new.txt", "new\n");
    let asked = parse(&words(&["merge", "--check", "--branch", "feature"])).unwrap();

    let (told, code) = carry_out(dir.path(), &crate::daemon::daemon(), None, &asked, &main)
        .await
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(told["standing"]["ahead"], 1);
    assert_eq!(told["standing"]["base"], "main");
    assert!(told["refusal"].is_null());
    assert!(!main.join("new.txt").exists());
}

#[tokio::test]
async fn a_merge_that_would_conflict_exits_with_3_and_touches_nothing() {
    let (dir, main, feature) = repository().await;
    commit(&main, "shared.txt", "main\n");
    commit(&feature, "shared.txt", "feature\n");
    let asked = parse(&words(&["merge", "--dir", feature.to_str().unwrap()])).unwrap();

    let (told, code) = carry_out(dir.path(), &crate::daemon::daemon(), None, &asked, &main)
        .await
        .unwrap();

    assert_eq!(code, 3);
    assert_eq!(told["merged"]["files"][0], "shared.txt");
    assert!(told["note"].as_str().unwrap().contains("--resolve"));
    assert_eq!(
        std::fs::read_to_string(main.join("shared.txt")).unwrap(),
        "main\n"
    );
}

#[tokio::test]
async fn a_branch_no_worktree_has_is_told() {
    let (dir, main, _) = repository().await;
    let asked = parse(&words(&["merge", "--branch", "nowhere"])).unwrap();

    let told = carry_out(dir.path(), &crate::daemon::daemon(), None, &asked, &main)
        .await
        .unwrap_err();

    assert!(
        told.contains("no worktree has the branch nowhere"),
        "{told}"
    );
}
