use super::*;
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn structural_output_is_kept_verbatim_and_missing_or_failed_difft_uses_git() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    std::fs::write(dir.path().join("a.rs"), "fn old() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "initial"]);
    std::fs::write(dir.path().join("a.rs"), "fn new() {}\n").unwrap();
    let change = Change {
        status: " M".into(),
        path: "a.rs".into(),
        from: None,
    };
    let fake = dir.path().join("fake difft");
    std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' 'a.rs --- Rust' '@@ structural, not a unified hunk' '1 1 fn new() {}'\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let lines = review::file_diff_with(dir.path(), None, &change, 2, fake.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| line.kind == DiffKind::Difft));
    assert_eq!(lines[1].text, "@@ structural, not a unified hunk");
    for program in ["/no/such/difft", "/usr/bin/false"] {
        let lines = review::file_diff_with(dir.path(), None, &change, 100, program)
            .await
            .unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.kind == DiffKind::Added && line.text.contains("new"))
        );
    }
    let untracked = Change {
        status: "??".into(),
        path: "new file.rs".into(),
        from: None,
    };
    std::fs::write(dir.path().join(&untracked.path), "new\n").unwrap();
    let lines = review::file_diff_with(dir.path(), None, &untracked, 100, "/no/such/difft")
        .await
        .unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.kind == DiffKind::Added && line.text == "+new")
    );
}

#[tokio::test]
async fn difft_receives_head_and_worktree_or_commit_blobs_without_running_configured_drivers() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
                .output()
                .unwrap()
                .status
                .success()
        );
    };
    git(&["init", "-q"]);
    std::fs::write(dir.path().join("a.rs"), "old\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "initial"]);
    std::fs::write(dir.path().join("a.rs"), "staged\n").unwrap();
    git(&["add", "a.rs"]);
    std::fs::write(dir.path().join("a.rs"), "worktree\n").unwrap();
    std::fs::write(dir.path().join(".gitattributes"), "a.rs diff=custom\n").unwrap();
    git(&["config", "diff.custom.command", "touch SHOULD_NOT_RUN"]);
    git(&["config", "diff.custom.textconv", "touch SHOULD_NOT_CONVERT"]);
    let fake = dir.path().join("fake-difft");
    std::fs::write(&fake, "#!/bin/sh\nshift 6\ncat \"$2\"\ncat \"$5\"\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let change = Change {
        status: "MM".into(),
        path: "a.rs".into(),
        from: None,
    };
    let lines = review::file_diff_with(dir.path(), None, &change, 100, fake.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
        ["old", "worktree"]
    );
    git(&["commit", "-qm", "staged version"]);
    let lines = review::file_diff_with(
        dir.path(),
        Some("HEAD"),
        &change,
        100,
        fake.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
        ["old", "staged"]
    );
    let _ = review::file_diff_with(dir.path(), None, &change, 100, "/no/such/difft").await;
    assert!(!dir.path().join("SHOULD_NOT_RUN").exists());
    assert!(!dir.path().join("SHOULD_NOT_CONVERT").exists());
}

#[tokio::test]
async fn installed_difft_displays_syntax_colors_and_untracked_files_without_exposing_snapshots() {
    if std::process::Command::new("difft")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let change = Change {
        status: "??".into(),
        path: "new file.rs".into(),
        from: None,
    };
    std::fs::write(dir.path().join(&change.path), "fn hello() {}\n").unwrap();
    let lines = review::file_diff(dir.path(), None, &change, 100)
        .await
        .unwrap();
    assert!(
        lines
            .iter()
            .any(|line| line.kind == DiffKind::Difft && line.text.contains("hello"))
    );
    assert!(lines.iter().any(|line| line.text.contains("\x1b[")));
    assert!(!lines.iter().any(|line| line.text.contains(".tmp")));
}

#[tokio::test]
async fn merge_commits_keep_gits_combined_comparison_instead_of_selecting_one_parent() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    git(&["init", "-q"]);
    for value in ["first", "second"] {
        std::fs::write(dir.path().join("a.rs"), format!("{value}\n")).unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", value]);
    }
    std::fs::write(dir.path().join("a.rs"), "merged\n").unwrap();
    git(&["add", "."]);
    let tree = git(&["write-tree"]);
    let merge = git(&[
        "commit-tree",
        &tree,
        "-p",
        "HEAD",
        "-p",
        "HEAD^",
        "-m",
        "merge",
    ]);
    let fake = dir.path().join("fake");
    std::fs::write(&fake, "#!/bin/sh\nprintf 'wrong comparison\\n'\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let change = Change {
        status: "M".into(),
        path: "a.rs".into(),
        from: None,
    };
    let lines = review::file_diff_with(
        dir.path(),
        Some(&merge),
        &change,
        100,
        fake.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert!(!lines.iter().any(|line| line.kind == DiffKind::Difft));
    assert!(lines.iter().any(|line| line.text.contains("merged")));
}
