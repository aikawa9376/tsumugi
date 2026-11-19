//! The executable installed on a work machine, built without the window.
//! A work machine runs Unix: its hosts run as the user's services there.
#![cfg(unix)]

use std::process::Command;
use std::time::Duration;

mod support {
    pub mod isolated;
}

use support::isolated::isolated;

#[test]
fn headless_checks_leave_the_desktop_executable_untouched() {
    for configured_target in [None, Some(""), Some("build artifacts")] {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let cargo = bin.join("cargo");
        std::fs::write(
            &cargo,
            "#!/bin/sh\nset -eu\nout=\"${CARGO_TARGET_DIR:-target}/debug\"\nmkdir -p \"$out\"\nprintf '%s\\n' \"$1\" >> \"$out/tsumugi\"\n",
        )
        .unwrap();
        tsumugi_sys::fs::set_mode(&cargo, 0o755).unwrap();
        std::fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../justfile"),
            dir.path().join("justfile"),
        )
        .unwrap();
        let target = dir.path().join(
            configured_target
                .filter(|path| !path.is_empty())
                .unwrap_or("target"),
        );
        let desktop = target.join("debug/tsumugi");
        std::fs::create_dir_all(desktop.parent().unwrap()).unwrap();
        std::fs::write(&desktop, "desktop executable").unwrap();
        let path = std::env::join_paths(
            std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let mut command = Command::new("just");
        command
            .current_dir(dir.path())
            .env("PATH", path)
            .env_remove("CARGO_TARGET_DIR")
            .arg("test-headless");
        if let Some(target) = configured_target {
            command.env("CARGO_TARGET_DIR", target);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(&desktop).unwrap(),
            "desktop executable",
            "CARGO_TARGET_DIR={configured_target:?}"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("headless/debug/tsumugi")).unwrap(),
            "clippy\ntest\n"
        );
    }
}

#[test]
fn a_headless_build_does_not_link_the_window_or_its_desktop_runtime() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
            "--no-default-features",
            "--edges",
            "normal",
            "--prefix",
            "none",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    assert!(!tree.contains("tsumugi-window"), "{tree}");
    assert!(!tree.contains("dioxus"), "{tree}");
    assert!(!tree.contains("wry "), "{tree}");
}

#[test]
fn the_headless_executable_reports_its_version_without_starting_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let output = isolated(dir.path()).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("tsumugi ")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn an_isolated_process_reaches_main_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = isolated(dir.path());
    let started = std::time::Instant::now();
    let output = command.arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[cfg(not(feature = "desktop"))]
#[test]
fn a_headless_build_refuses_window_and_editor_requests_without_starting_hosts() {
    for args in [
        vec![],
        vec!["--new-window"],
        vec!["editor", "/tmp/prompt.md"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = isolated(dir.path()).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("no window")
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
