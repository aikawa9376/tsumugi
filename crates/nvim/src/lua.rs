//! The Lua tsumugi gives its Neovims, as modules under `tsumugi.` that
//! `require` finds before the user's configuration runs, and the Neovim
//! they run in.

use std::path::{Path, PathBuf};

/// Every module of tsumugi's, by the name `require` takes, with its source.
const MODULES: &[(&str, &str)] = &[
    ("tsumugi.hold", include_str!("../lua/tsumugi/hold.lua")),
    ("tsumugi.config", include_str!("../lua/tsumugi/config.lua")),
    ("tsumugi.offer", include_str!("../lua/tsumugi/offer.lua")),
    ("tsumugi.server", include_str!("../lua/tsumugi/server.lua")),
    ("tsumugi.keys", include_str!("../lua/tsumugi/keys.lua")),
    (
        "tsumugi.complete",
        include_str!("../lua/tsumugi/complete.lua"),
    ),
    ("tsumugi.prompt", include_str!("../lua/tsumugi/prompt.lua")),
    (
        "tsumugi.prompt_defaults",
        include_str!("../lua/tsumugi/prompt_defaults.lua"),
    ),
    ("tsumugi.url", include_str!("../lua/tsumugi/url.lua")),
    ("tsumugi.reader", include_str!("../lua/tsumugi/reader.lua")),
    (
        "tsumugi.reader_defaults",
        include_str!("../lua/tsumugi/reader_defaults.lua"),
    ),
];

/// The Neovim tsumugi runs for the Neovims it embeds: the program
/// `TSUMUGI_NVIM` names, else the one the app carries, else `nvim` found on
/// `PATH`. Tests run it too, so that a Neovim to be shipped is tested by
/// naming it.
pub fn program() -> String {
    chosen(
        std::env::var("TSUMUGI_NVIM").ok(),
        std::env::current_exe().ok().as_deref(),
    )
}

/// The Neovim `named` names, else the one the app whose executable is `exe`
/// carries, else `nvim` found on `PATH`.
fn chosen(named: Option<String>, exe: Option<&Path>) -> String {
    if let Some(named) = named.filter(|n| !n.is_empty()) {
        return named;
    }
    exe.and_then(carried)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "nvim".into())
}

/// The Neovim an app carries beside its executable `exe`, as
/// `Contents/Resources/neovim/bin/nvim` beside `Contents/MacOS/tsumugi`.
fn carried(exe: &Path) -> Option<PathBuf> {
    let nvim = exe.parent()?.parent()?.join("Resources/neovim/bin/nvim");
    nvim.is_file().then_some(nvim)
}

/// The version of the Neovim tsumugi runs, as it names itself, such as
/// `0.12.6`; none when it cannot be run.
pub async fn version() -> Option<String> {
    let out = tokio::process::Command::new(program())
        .arg("--version")
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    named(&String::from_utf8_lossy(&out.stdout))
}

/// The version `nvim --version` printed as `out` names.
fn named(out: &str) -> Option<String> {
    let first = out.lines().next()?;
    first.strip_prefix("NVIM v").map(str::to_string)
}

/// Lua that lets `require` find every module of tsumugi's, one `:lua`
/// command long, as it has to be given before the user's configuration.
pub(crate) fn preload() -> String {
    MODULES
        .iter()
        .map(|(name, source)| {
            let level = "=".repeat(level(source));
            format!(
                "package.preload[{name:?}] = assert(loadstring([{level}[\n{source}]{level}], \"@{name}\"))\n"
            )
        })
        .collect()
}

/// Lua that requires `module` once every module is found, as a `:lua`
/// command takes it.
pub(crate) fn require(module: &str) -> String {
    format!("{}require({module:?})", preload())
}

/// The fewest `=` a long bracket needs to hold `source` whole.
fn level(source: &str) -> usize {
    (0..)
        .find(|n| !source.contains(&format!("]{}]", "=".repeat(*n))))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// An app at `dir` whose executable is `Contents/MacOS/tsumugi`, carrying
    /// a Neovim when `carries`; the executable's path.
    fn app(dir: &Path, carries: bool) -> PathBuf {
        let contents = dir.join("Tsumugi.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        if carries {
            let bin = contents.join("Resources/neovim/bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("nvim"), "").unwrap();
        }
        contents.join("MacOS/tsumugi")
    }

    #[test]
    #[cfg_attr(windows, ignore = "written with Unix paths")]
    fn the_neovim_named_comes_first_then_the_one_the_app_carries_then_the_one_on_path() {
        let dir = tempfile::tempdir().unwrap();
        let carrying = app(&dir.path().join("a"), true);
        let bare = app(&dir.path().join("b"), false);
        let carried = dir
            .path()
            .join("a/Tsumugi.app/Contents/Resources/neovim/bin/nvim");
        assert_eq!(
            chosen(Some("/opt/nvim".into()), Some(&carrying)),
            "/opt/nvim"
        );
        assert_eq!(chosen(None, Some(&carrying)), carried.to_string_lossy());
        assert_eq!(
            chosen(Some(String::new()), Some(&carrying)),
            carried.to_string_lossy()
        );
        assert_eq!(chosen(None, Some(&bare)), "nvim");
        assert_eq!(chosen(None, None), "nvim");
    }

    #[test]
    fn the_version_is_what_neovim_names_itself_by() {
        assert_eq!(
            named("NVIM v0.12.6\nBuild type: Release\n"),
            Some("0.12.6".into())
        );
        assert_eq!(
            named("NVIM v0.13.0-dev-412+g1a2b3c4\n"),
            Some("0.13.0-dev-412+g1a2b3c4".into())
        );
        assert_eq!(named("vim 9.1\n"), None);
    }

    #[tokio::test]
    async fn the_neovim_tsumugi_runs_tells_its_version() {
        let version = version().await.expect("the Neovim tsumugi runs");
        assert!(version.starts_with("0."), "{version}");
    }

    #[test]
    fn a_long_bracket_holds_a_source_that_closes_shorter_ones() {
        assert_eq!(level("x"), 0);
        assert_eq!(level("t[1]] ]=]"), 2);
    }

    fn specs() -> Vec<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lua/spec");
        let mut found: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with("_spec.lua"))
            .collect();
        found.sort();
        found
    }

    /// Every Lua spec, run in the Neovim tsumugi runs with tsumugi's modules
    /// found, passes: its rules hold for that Neovim.
    #[tokio::test]
    async fn every_lua_spec_passes_in_the_neovim_tsumugi_runs() {
        let runner = Path::new(env!("CARGO_MANIFEST_DIR")).join("lua/spec/runner.lua");
        let specs = specs();
        assert!(!specs.is_empty());
        let mut failed = Vec::new();
        for spec in specs {
            let run = tokio::process::Command::new(program())
                .args(["--clean", "--headless", "--cmd"])
                .arg(format!("lua {}", preload()))
                .arg("-l")
                .arg(&runner)
                .arg(&spec)
                .kill_on_drop(true)
                .output();
            let out = tokio::time::timeout(Duration::from_secs(30), run)
                .await
                .unwrap_or_else(|_| panic!("{} timed out", spec.display()))
                .expect("the Neovim tsumugi runs");
            if !out.status.success() {
                failed.push(format!(
                    "{}:\n{}{}",
                    spec.display(),
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
