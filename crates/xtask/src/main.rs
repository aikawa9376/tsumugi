//! The project's tasks, run as `cargo xtask <task>`: the quality gate, the
//! tests in isolation, the references and the bundles. Developing Tsumugi
//! needs only rustup and the platform's packages; a task runs Cargo, the
//! scripts under `assets/` and the platform's own tools.

use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

/// A task, as `cargo xtask` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    Ci,
    Fmt,
    FmtCheck,
    Lint,
    Test,
    TestNeovim,
    TestHeadless,
    References,
    StampCheck,
    Licenses,
    LicensesCheck,
    NoticesTest,
    BundleTest,
    Run,
    BuildMacos,
    InstallMacos,
    Bundle,
    MacosMakeIdentity,
    MacosImportIdentity,
    InstallLinux,
    BundleLinux,
    SourceArchive,
}

impl Task {
    const ALL: &[Task] = &[
        Task::Ci,
        Task::Fmt,
        Task::FmtCheck,
        Task::Lint,
        Task::Test,
        Task::TestNeovim,
        Task::TestHeadless,
        Task::References,
        Task::StampCheck,
        Task::Licenses,
        Task::LicensesCheck,
        Task::NoticesTest,
        Task::BundleTest,
        Task::Run,
        Task::BuildMacos,
        Task::InstallMacos,
        Task::Bundle,
        Task::MacosMakeIdentity,
        Task::MacosImportIdentity,
        Task::InstallLinux,
        Task::BundleLinux,
        Task::SourceArchive,
    ];

    fn name(self) -> &'static str {
        match self {
            Task::Ci => "ci",
            Task::Fmt => "fmt",
            Task::FmtCheck => "fmt-check",
            Task::Lint => "lint",
            Task::Test => "test",
            Task::TestNeovim => "test-neovim",
            Task::TestHeadless => "test-headless",
            Task::References => "references",
            Task::StampCheck => "stamp-check",
            Task::Licenses => "licenses",
            Task::LicensesCheck => "licenses-check",
            Task::NoticesTest => "notices-test",
            Task::BundleTest => "bundle-test",
            Task::Run => "run",
            Task::BuildMacos => "build-macos",
            Task::InstallMacos => "install-macos",
            Task::Bundle => "bundle",
            Task::MacosMakeIdentity => "macos-make-identity",
            Task::MacosImportIdentity => "macos-import-identity",
            Task::InstallLinux => "install-linux",
            Task::BundleLinux => "bundle-linux",
            Task::SourceArchive => "source-archive",
        }
    }

    fn named(name: &str) -> Option<Task> {
        Task::ALL.iter().copied().find(|t| t.name() == name)
    }

    /// What it does, and what it takes, as `cargo xtask` lists it.
    fn about(self) -> &'static str {
        match self {
            Task::Ci => {
                "everything CI runs on this platform, the tests in the Neovim the app carries"
            }
            Task::Fmt => "format every source",
            Task::FmtCheck => "check formatting without changing files",
            Task::Lint => "lint with every warning as an error",
            Task::Test => {
                "[cargo test args] run the tests, in the Neovim TSUMUGI_NVIM names when it is set"
            }
            Task::TestNeovim => {
                "[nvim] run the tests in the Neovim at nvim, the one the app carries by default"
            }
            Task::TestHeadless => {
                "check the executable for a work machine, with no desktop runtime linked"
            }
            Task::References => "write the references in docs/spec/reference/ from the code",
            Task::StampCheck => {
                "fail when the release could not stamp its version over the placeholder"
            }
            Task::Licenses => "write THIRD-PARTY-LICENSES.txt from Cargo.lock",
            Task::LicensesCheck => "fail when THIRD-PARTY-LICENSES.txt is stale (checked on macOS)",
            Task::NoticesTest => "test the normalization of the licence notices",
            Task::BundleTest => {
                "test that app startup verification rejects broken bundled programs"
            }
            Task::Run => "[args] run Tsumugi with every desktop feature enabled",
            Task::BuildMacos => "build target/macos/Tsumugi.app (macOS)",
            Task::InstallMacos => "put the app build-macos makes in /Applications (macOS)",
            Task::Bundle => {
                "[version] build Tsumugi.app for release and its disk image in dist/ (macOS)"
            }
            Task::MacosMakeIdentity => {
                "<folder> make a new signing identity, \"Tsumugi Release\" (macOS)"
            }
            Task::MacosImportIdentity => "import MACOS_SIGNING_P12 into the login keychain (macOS)",
            Task::InstallLinux => {
                "build Tsumugi and install its desktop launcher for this user (Linux)"
            }
            Task::BundleLinux => {
                "[version] build a Linux archive with its installer and notices (Linux)"
            }
            Task::SourceArchive => {
                "[version] package the source and vendored dependencies of a release"
            }
        }
    }
}

/// The platform a gate is run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Os {
    Linux,
    Macos,
    Windows,
}

impl Os {
    fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Linux
        }
    }
}

/// What the gate runs on `os`, in order.
fn gate(os: Os) -> Vec<Task> {
    use Task::*;
    match os {
        // A work machine runs Unix, so Windows builds no headless executable;
        // the Python checks are about the bundles, which Unix makes.
        Os::Windows => vec![FmtCheck, Lint, StampCheck, TestNeovim],
        // The licences are checked where the app that ships them is built.
        Os::Macos => vec![
            FmtCheck,
            Lint,
            StampCheck,
            NoticesTest,
            BundleTest,
            LicensesCheck,
            TestNeovim,
            TestHeadless,
        ],
        Os::Linux => vec![
            FmtCheck,
            Lint,
            StampCheck,
            NoticesTest,
            BundleTest,
            TestNeovim,
            TestHeadless,
        ],
    }
}

/// The variables the tests run with, beside those they inherit: `Some` sets
/// one, `None` removes it. What Tsumugi or an editor running this set would
/// steer the tests by is removed, but for the Neovim they are asked to run in
/// and whether they may listen on the local network; the shell's and the XDG
/// directories are the run's own, under `root`.
fn test_env<'a>(
    names: impl Iterator<Item = &'a OsStr>,
    root: &Path,
) -> Vec<(OsString, Option<PathBuf>)> {
    let kept = ["TSUMUGI_NVIM", "TSUMUGI_TEST_LAN"];
    let editor = ["CODEX_THREAD_ID", "VIM", "VIMRUNTIME"];
    let mut env: Vec<(OsString, Option<PathBuf>)> = names
        .filter(|name| {
            let name = name.to_string_lossy();
            (name.starts_with("TSUMUGI_") && !kept.contains(&&*name)) || editor.contains(&&*name)
        })
        .map(|name| (name.to_owned(), None))
        .collect();
    for (name, dir) in [
        ("ZDOTDIR", "zsh"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
    ] {
        env.push((name.into(), Some(root.join(dir))));
    }
    env
}

/// Cargo's target directory, as `CARGO_TARGET_DIR` sets it: empty is unset.
fn target_dir(configured: Option<&OsStr>) -> PathBuf {
    configured
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("target"), PathBuf::from)
}

/// Where the headless executable is built: apart from the desktop one, which
/// a build without its features would otherwise replace.
fn headless_target(configured: Option<&OsStr>) -> PathBuf {
    target_dir(configured).join("headless")
}

/// A directory of this run's own, taken away when it is dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Result<Scratch, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("tsumugi-{label}.{}.{nanos}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The Cargo that runs this task, so its tasks build with the same one.
fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// A script under `assets/`, run by bash, which Git Bash gives Windows too.
fn script(path: &str) -> Command {
    let mut command = Command::new("bash");
    command.arg(path);
    command
}

fn run(command: &mut Command) -> Result<(), String> {
    eprintln!("+ {command:?}");
    let status = command
        .status()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{:?} failed: {status}", command.get_program()))
    }
}

/// What `command` writes on its standard output, which must succeed.
fn output(command: &mut Command) -> Result<String, String> {
    eprintln!("+ {command:?}");
    let out = command
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if !out.status.success() {
        return Err(format!(
            "{:?} failed: {}",
            command.get_program(),
            out.status
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

fn only_on(os: Os, task: Task) -> Result<(), String> {
    if Os::current() == os {
        Ok(())
    } else {
        Err(format!("{} runs on {os:?} only", task.name()))
    }
}

fn version(args: &[String]) -> &str {
    args.first().map_or("0.0.0-ci", String::as_str)
}

fn python_tests(dir: &str) -> Result<(), String> {
    run(Command::new("python3").args(["-m", "unittest", "discover", "-s", dir, "-p", "test_*.py"]))
}

/// The tests, with `nvim` as the Neovim they run in when it is given.
fn test(args: &[String], nvim: Option<&Path>) -> Result<(), String> {
    let root = Scratch::new("tests")?;
    let mut command = cargo();
    command.arg("test");
    if args.is_empty() {
        command.args(["--workspace", "--no-fail-fast"]);
    } else {
        command.args(args);
    }
    let names: Vec<OsString> = std::env::vars_os().map(|(k, _)| k).collect();
    for (name, value) in test_env(names.iter().map(OsString::as_os_str), &root.0) {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    if let Some(nvim) = nvim {
        command.env("TSUMUGI_NVIM", nvim);
    }
    run(&mut command)
}

/// The Neovim the app carries, fetched and checked against its pin.
fn carried_neovim() -> Result<PathBuf, String> {
    let dir = output(&mut script("assets/neovim/fetch.sh"))?;
    let bin = Path::new(dir.trim()).join("bin");
    Ok(if cfg!(windows) {
        bin.join("nvim.exe")
    } else {
        bin.join("nvim")
    })
}

fn licenses_sorted() -> Result<String, String> {
    let scratch = Scratch::new("licenses")?;
    let generated = scratch.0.join("licenses.txt");
    run(cargo()
        // The package a release ships, so the tasks' own crate is not listed.
        .args(["about", "generate", "--locked"])
        .args([
            "--manifest-path",
            "crates/tsumugi/Cargo.toml",
            "about.hbs",
            "-o",
        ])
        .arg(&generated))?;
    output(
        Command::new("python3")
            .arg("assets/release/normalize_licenses.py")
            .arg(&generated),
    )
}

fn macos_app() -> PathBuf {
    target_dir(std::env::var_os("CARGO_TARGET_DIR").as_deref()).join("macos/Tsumugi.app")
}

fn build_macos() -> Result<(), String> {
    run(cargo().args(["build", "--release", "--locked", "-p", "tsumugi"]))?;
    run(&mut script("assets/macos/bundle.sh"))?;
    run(Command::new("python3")
        .arg("assets/macos/verify_bundle.py")
        .arg(macos_app()))
}

fn perform(task: Task, args: &[String]) -> Result<(), String> {
    match task {
        Task::Ci => {
            for task in gate(Os::current()) {
                eprintln!("== {}", task.name());
                perform(task, &[])?;
            }
            Ok(())
        }
        Task::Fmt => run(cargo().args(["fmt", "--all"])),
        Task::FmtCheck => run(cargo().args(["fmt", "--all", "--check"])),
        Task::Lint => run(cargo().args([
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ])),
        Task::Test => test(args, None),
        Task::TestNeovim => {
            let nvim = match args.first() {
                Some(nvim) => PathBuf::from(nvim),
                None => carried_neovim()?,
            };
            test(&[], Some(&nvim))
        }
        Task::TestHeadless => {
            let target = headless_target(std::env::var_os("CARGO_TARGET_DIR").as_deref());
            run(cargo()
                .env("CARGO_TARGET_DIR", &target)
                .args([
                    "clippy",
                    "-p",
                    "tsumugi",
                    "--all-targets",
                    "--no-default-features",
                ])
                .args(["--", "-D", "warnings"]))?;
            run(cargo()
                .env("CARGO_TARGET_DIR", &target)
                .args(["test", "-p", "tsumugi", "--no-default-features"])
                .args(["--test", "headless", "--test", "hosts"])
                .args(["--test", "remote", "--test", "gateway"]))
        }
        Task::References => {
            run(cargo().args(["run", "-q", "-p", "tsumugi-core", "--example", "references"]))
        }
        Task::StampCheck => {
            let copy = Scratch::new("stamp")?;
            for file in ["Cargo.toml", "Cargo.lock"] {
                std::fs::copy(file, copy.0.join(file)).map_err(|e| format!("{file}: {e}"))?;
            }
            run(Command::new("perl")
                .args(["assets/release/stamp-version.pl", "0.0.0-ci"])
                .arg(&copy.0))
        }
        Task::Licenses => std::fs::write("THIRD-PARTY-LICENSES.txt", licenses_sorted()?)
            .map_err(|e| e.to_string()),
        Task::LicensesCheck => {
            // Checked where the release builds the app that ships them;
            // elsewhere cargo-about need not be installed.
            if Os::current() != Os::Macos {
                return Ok(());
            }
            let kept =
                std::fs::read_to_string("THIRD-PARTY-LICENSES.txt").map_err(|e| e.to_string())?;
            if kept == licenses_sorted()? {
                Ok(())
            } else {
                Err("THIRD-PARTY-LICENSES.txt is stale; run cargo xtask licenses".into())
            }
        }
        Task::NoticesTest => python_tests("assets/release"),
        Task::BundleTest => python_tests("assets/macos"),
        Task::Run => run(cargo()
            .args(["run", "--locked", "-p", "tsumugi", "--bin", "tsumugi", "--"])
            .args(args)),
        Task::BuildMacos => {
            only_on(Os::Macos, task)?;
            build_macos()
        }
        Task::InstallMacos => {
            only_on(Os::Macos, task)?;
            build_macos()?;
            eprint!("Replace /Applications/Tsumugi.app? [y/N] ");
            let _ = std::io::stderr().flush();
            let mut answer = String::new();
            std::io::stdin()
                .read_line(&mut answer)
                .map_err(|e| e.to_string())?;
            if !matches!(answer.trim(), "y" | "Y" | "yes") {
                return Err("left /Applications/Tsumugi.app as it was".into());
            }
            match std::fs::remove_dir_all("/Applications/Tsumugi.app") {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e.to_string());
                }
                _ => {}
            }
            run(Command::new("cp")
                .arg("-R")
                .arg(macos_app())
                .arg("/Applications/"))
        }
        // Run outside any Nix shell, as the release does: a build made inside
        // one refers into /nix/store, which the disk image refuses.
        Task::Bundle => {
            only_on(Os::Macos, task)?;
            build_macos()?;
            run(script("assets/macos/dmg.sh").args([version(args), "dist"]))
        }
        Task::MacosMakeIdentity => {
            only_on(Os::Macos, task)?;
            let folder = args.first().ok_or("macos-make-identity takes a folder")?;
            run(script("assets/macos/make-identity.sh").arg(folder))
        }
        Task::MacosImportIdentity => {
            only_on(Os::Macos, task)?;
            run(&mut script("assets/macos/import-identity.sh"))
        }
        Task::InstallLinux => {
            only_on(Os::Linux, task)?;
            run(cargo().args([
                "build",
                "--release",
                "--locked",
                "-p",
                "tsumugi",
                "--bin",
                "tsumugi",
            ]))?;
            run(&mut script("assets/linux/install.sh"))
        }
        Task::BundleLinux => {
            only_on(Os::Linux, task)?;
            run(cargo().args(["build", "--release", "--locked", "-p", "tsumugi"]))?;
            run(script("assets/linux/bundle.sh").args([version(args), "dist"]))
        }
        Task::SourceArchive => {
            run(script("assets/release/source.sh").args([version(args), "dist"]))
        }
    }
}

fn list() {
    eprintln!("cargo xtask <task> [args]\n");
    for task in Task::ALL {
        eprintln!("  {:<22} {}", task.name(), task.about());
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(name) = args.next() else {
        list();
        return ExitCode::SUCCESS;
    };
    let Some(task) = Task::named(&name) else {
        eprintln!("no task {name:?}\n");
        list();
        return ExitCode::FAILURE;
    };
    // The tasks name their files from the workspace's root, wherever Cargo
    // was run in it.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if let Err(e) = std::env::set_current_dir(&root) {
        eprintln!("xtask: {}: {e}", root.display());
        return ExitCode::FAILURE;
    }
    let args: Vec<String> = args.collect();
    match perform(task, &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
