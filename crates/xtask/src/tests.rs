use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::*;

#[test]
fn each_platform_s_gate_runs_its_own_checks() {
    use Task::*;
    let unix = [
        FmtCheck,
        Lint,
        StampCheck,
        NoticesTest,
        BundleTest,
        TestNeovim,
        TestHeadless,
    ];
    // (platform, what its gate runs)
    let cases: [(Os, &[Task]); 3] = [
        (Os::Linux, &unix),
        (
            Os::Macos,
            &[
                FmtCheck,
                Lint,
                StampCheck,
                NoticesTest,
                BundleTest,
                LicensesCheck,
                TestNeovim,
                TestHeadless,
            ],
        ),
        // A work machine runs Unix, so Windows builds no headless executable.
        (Os::Windows, &[FmtCheck, Lint, StampCheck, TestNeovim]),
    ];
    for (os, want) in cases {
        assert_eq!(gate(os), want, "{os:?}");
    }
}

#[test]
fn the_tests_keep_only_the_variables_they_are_asked_to_run_with() {
    let root = Path::new("/tmp/t");
    let given = [
        "TSUMUGI_NVIM",
        "TSUMUGI_TEST_LAN",
        "TSUMUGI_LOG",
        "TSUMUGI_DEBUG_KEYS",
        "CODEX_THREAD_ID",
        "VIM",
        "VIMRUNTIME",
        "PATH",
        "CI",
    ]
    .map(OsString::from);
    let env = test_env(given.iter().map(OsString::as_os_str), root);
    let removed: Vec<_> = env
        .iter()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k.to_str().unwrap())
        .collect();
    assert_eq!(
        removed,
        [
            "TSUMUGI_LOG",
            "TSUMUGI_DEBUG_KEYS",
            "CODEX_THREAD_ID",
            "VIM",
            "VIMRUNTIME"
        ]
    );
    let set: Vec<_> = env
        .iter()
        .filter_map(|(k, v)| Some((k.to_str().unwrap(), v.clone()?)))
        .collect();
    assert_eq!(
        set,
        [
            ("ZDOTDIR", root.join("zsh")),
            ("XDG_STATE_HOME", root.join("state")),
            ("XDG_CONFIG_HOME", root.join("config")),
            ("XDG_DATA_HOME", root.join("data")),
        ]
    );
}

#[test]
fn the_headless_build_never_lands_on_the_desktop_executable() {
    // (CARGO_TARGET_DIR as set, the desktop's target, the headless one)
    let cases = [
        (None, "target", "target/headless"),
        (Some(""), "target", "target/headless"),
        (
            Some("build artifacts"),
            "build artifacts",
            "build artifacts/headless",
        ),
    ];
    for (configured, desktop, headless) in cases {
        let configured = configured.map(OsString::from);
        assert_eq!(
            target_dir(configured.as_deref()),
            PathBuf::from(desktop),
            "{configured:?}"
        );
        assert_eq!(
            headless_target(configured.as_deref()),
            PathBuf::from(headless),
            "{configured:?}"
        );
    }
}

#[test]
fn a_task_is_named_as_it_is_typed() {
    for task in Task::ALL {
        assert_eq!(Task::named(task.name()), Some(*task));
    }
    assert_eq!(Task::named("nothing"), None);
}
