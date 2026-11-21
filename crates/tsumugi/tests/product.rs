//! The application's executable is usable without opening a GUI or creating hosts.
use std::process::Command;

fn isolated(dir: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("TSUMUGI_") {
            command.env_remove(name);
        }
    }
    command
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("XDG_DATA_HOME", dir.join("data"));
    command
}

#[test]
fn tsumugi_reports_its_own_version_without_creating_hosts() {
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
fn an_unknown_option_reports_tsumugi_usage_without_starting_a_window() {
    let dir = tempfile::tempdir().unwrap();
    let output = isolated(dir.path())
        .arg("--not-a-real-option")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("usage: tsumugi ")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn the_tsumugi_namespace_selects_the_cli_instance_before_any_host_starts() {
    let dir = tempfile::tempdir().unwrap();
    let output = isolated(dir.path())
        .env("TSUMUGI_INSTANCE", "invalid/instance")
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("tsumugi:")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn a_desktop_install_carries_the_notices_of_every_bundled_component() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("bash")
        .arg(root.join("assets/linux/install.sh"))
        .arg("--prefix")
        .arg(dir.path())
        .args(["--binary", "/bin/true"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let notices = dir.path().join("share/licenses/tsumugi");
    for name in [
        "LICENSE",
        "ACKNOWLEDGEMENTS.md",
        "THIRD-PARTY-LICENSES.txt",
        "fonts/OFL.txt",
        "fonts/LICENSE",
        "file-icons/vim-nerdfont/LICENSE",
        "file-icons/vim-nerdfont/LICENSE.vim-devicon",
        "file-icons/vim-glyph-palette/LICENSE",
        "tabler/LICENSE",
        "neovim/LICENSE.txt",
        "arto-frontend/katex.LICENSE.txt",
        "themes/nightfox.nvim",
    ] {
        assert!(notices.join(name).is_file(), "missing notice: {name}");
    }
    assert!(
        std::fs::read_to_string(notices.join("LICENSE"))
            .unwrap()
            .contains("Copyright (c) 2026 Alisue <lambdalisue@gmail.com>")
    );
}
