//! A persistent work host can still start helpers after an application update.
#![cfg(target_os = "linux")]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn a_running_host_finds_its_executable_after_replacement() {
    for (name, replace) in [
        ("tsumugi", true),
        ("tsumugi (deleted)", false),
        ("tsumugi (deleted)", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join(name);
        let ready = dir.path().join("ready");
        std::fs::copy(std::env::current_exe().unwrap(), &program).unwrap();
        let mut child = Command::new(&program)
            .args(["--exact", "executable_fixture", "--ignored", "--nocapture"])
            .env("TSUMUGI_TEST_EXECUTABLE", &program)
            .env("TSUMUGI_TEST_EXECUTABLE_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            if child.try_wait().unwrap().is_some() || Instant::now() >= until {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap();
                panic!("the fixture did not start: {output:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if replace {
            let replacement = dir.path().join("replacement");
            std::fs::copy(&program, &replacement).unwrap();
            std::fs::rename(replacement, &program).unwrap();
        } else {
            // A real filename ending in the kernel's marker must stay intact.
            std::fs::copy(&program, dir.path().join("tsumugi")).unwrap();
        }
        child.stdin.take().unwrap().write_all(b"x").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{name}, replaced={replace}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "run by a_running_host_finds_its_executable_after_replacement"]
fn executable_fixture() {
    use std::io::Read;

    std::fs::write(
        std::env::var_os("TSUMUGI_TEST_EXECUTABLE_READY").unwrap(),
        b"",
    )
    .unwrap();
    std::io::stdin().read_exact(&mut [0]).unwrap();
    let expected = std::env::var_os("TSUMUGI_TEST_EXECUTABLE").unwrap();
    let program = tsumugi_host::work_host::tsumugi_program();
    assert_eq!(program, Path::new(&expected));
    let output = Command::new(program).arg("--list").output().unwrap();
    assert!(output.status.success(), "{output:?}");
}
