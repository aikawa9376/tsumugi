use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn limits(text: &str) -> Limits {
    let settings = tsumugi_core::settings::Settings::parse(text).unwrap();
    Limits::from(&settings.governor)
}

#[test]
fn with_no_limit_set_every_start_has_room() {
    let none = limits("");
    assert_eq!(none.admit(0), Admission::Room);
    assert_eq!(none.admit(500), Admission::Room);
    assert!(
        none.environment("/r/api".as_ref(), "/s".as_ref())
            .is_empty()
    );
}

#[test]
fn a_start_beyond_the_most_sessions_has_no_room_and_says_how_many_run() {
    let two = limits("[governor]\nmax_sessions = 2\n");
    assert_eq!(two.admit(1), Admission::Room);
    let full = two.admit(2);
    assert_eq!(full, Admission::Full { running: 2, max: 2 });
    assert!(full.refusal().unwrap().contains("2 of 2"));
    assert!(two.admit(5).refusal().unwrap().contains("5 of 2"));
}

#[test]
fn builds_are_told_how_many_jobs_to_run_and_where_to_build() {
    let primary = Path::new("/r/api");
    let shared = Path::new("/s");
    let jobs = limits("[governor]\nbuild_jobs = 4\n").environment(primary, shared);
    assert_eq!(jobs, [("CARGO_BUILD_JOBS".to_string(), "4".to_string())]);

    let both =
        limits("[governor]\nbuild_jobs = 2\nbuild_cache = 'shared'\n").environment(primary, shared);
    assert_eq!(both[0], ("CARGO_BUILD_JOBS".to_string(), "2".to_string()));
    assert_eq!(
        both[1],
        (
            "CARGO_TARGET_DIR".to_string(),
            folder_of(shared, primary).display().to_string()
        )
    );

    let cached = limits("[governor]\nbuild_cache = 'sccache'\n").environment(primary, shared);
    assert_eq!(
        cached,
        [("RUSTC_WRAPPER".to_string(), "sccache".to_string())]
    );
}

#[test]
fn worktrees_of_one_repository_share_a_folder_and_another_repository_does_not() {
    let shared = Path::new("/s");
    let api = folder_of(shared, "/r/api".as_ref());
    assert_eq!(api, folder_of(shared, "/r/api".as_ref()));
    assert_ne!(api, folder_of(shared, "/other/api".as_ref()));
    assert!(api.starts_with("/s"));
    assert!(
        api.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("api-")
    );
}

#[test]
fn worktrees_over_the_size_allowed_warn_naming_the_largest() {
    let allowed = limits("[governor]\nworktree_warn_gib = 10\n");
    let size = |gib: u64| gib * GIB;
    let under = [("/w/a".into(), size(4)), ("/w/b".into(), size(5))];
    assert_eq!(allowed.warning(&under), None);
    let over = [
        ("/w/a".into(), size(4)),
        ("/w/b".into(), size(9)),
        ("/w/c".into(), size(1)),
    ];
    let warning = allowed.warning(&over).unwrap();
    assert!(warning.contains("14.0 GiB of the 10.0 GiB"), "{warning}");
    assert!(warning.find("/w/b").unwrap() < warning.find("/w/a").unwrap());
    assert_eq!(limits("").warning(&over), None);
}

#[test]
fn a_folder_weighs_the_files_under_it_and_not_what_its_links_lead_to() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a"), [0u8; 100]).unwrap();
    std::fs::write(dir.path().join("sub/b"), [0u8; 50]).unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("big"), [0u8; 4000]).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    assert!(size_of(dir.path()) >= 150);
    assert!(size_of(dir.path()) < 4000);
    assert_eq!(size_of(&dir.path().join("missing")), 0);
}

#[tokio::test]
async fn a_start_waits_for_room_and_goes_once_a_session_has_ended() {
    let two = limits("[governor]\nmax_sessions = 2\n");
    let looked = Arc::new(AtomicUsize::new(0));
    let seen = looked.clone();
    let result = wait_for_room(
        &two,
        Duration::from_secs(5),
        Duration::from_millis(5),
        move || {
            let now = seen.fetch_add(1, Ordering::SeqCst);
            async move { Ok(if now < 3 { 2 } else { 1 }) }
        },
    )
    .await;
    assert_eq!(result, Ok(()));
    assert_eq!(looked.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn a_start_that_waited_in_vain_is_refused_with_how_many_run() {
    let two = limits("[governor]\nmax_sessions = 2\n");
    let result = wait_for_room(
        &two,
        Duration::from_millis(30),
        Duration::from_millis(5),
        || async { Ok(3) },
    )
    .await;
    assert!(result.unwrap_err().contains("3 of 2"));
}

#[tokio::test]
async fn nothing_is_waited_for_where_nothing_is_limited() {
    let looked = Arc::new(AtomicUsize::new(0));
    let seen = looked.clone();
    wait_for_room(
        &limits(""),
        Duration::from_millis(30),
        Duration::from_millis(5),
        move || {
            seen.fetch_add(1, Ordering::SeqCst);
            async { Ok(99) }
        },
    )
    .await
    .unwrap();
    assert_eq!(looked.load(Ordering::SeqCst), 1);
}
