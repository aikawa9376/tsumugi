//! The Claude Code plugin every session tsumugi starts loads, which tells
//! Claude of the `tsumugi` command it can run from there.

use std::path::Path;

use crate::integration::start_session;

const MANIFEST: &str = include_str!("../../plugin/.claude-plugin/plugin.json");

/// Write the plugin into `dir`, naming `program`, over what an earlier tsumugi
/// wrote there.
pub fn install(program: &Path, dir: &Path) -> std::io::Result<()> {
    let manifest = dir.join(".claude-plugin");
    std::fs::create_dir_all(&manifest)?;
    std::fs::write(manifest.join("plugin.json"), MANIFEST)?;
    let skill = dir.join("skills").join("start-session");
    std::fs::create_dir_all(&skill)?;
    std::fs::write(skill.join("SKILL.md"), start_session(program))?;
    let hooks = dir.join("hooks");
    std::fs::create_dir_all(&hooks)?;
    std::fs::write(hooks.join("hooks.json"), self::hooks(program))
}

/// The tools Claude Code writes a file with, each naming it `file_path`.
pub const WRITING_TOOLS: [&str; 3] = ["Edit", "MultiEdit", "Write"];

/// The hooks that ask `program` what to tell Claude: once a conversation is
/// cleared and as each prompt arrives; and that tell it of each file Claude
/// is about to write and has written.
fn hooks(program: &Path) -> String {
    let asked = serde_json::json!([{
        "type": "command",
        "command": format!("{} mcp hook", crate::shell_word(program)),
        "timeout": 5,
    }]);
    let hooks = serde_json::json!({"hooks": {
        "SessionStart": [{"matcher": "clear", "hooks": asked}],
        "UserPromptSubmit": [{"hooks": asked}],
        "PreToolUse": [{"matcher": WRITING_TOOLS.join("|"), "hooks": asked}],
        "PostToolUse": [{"matcher": WRITING_TOOLS.join("|"), "hooks": asked}],
    }});
    serde_json::to_string_pretty(&hooks).expect("the hooks are JSON")
}

/// The arguments that load the plugin, written afresh into `dir` for the
/// session about to start; none when it cannot be written, as the session
/// runs without it.
pub fn args(program: &Path, dir: &Path) -> Vec<String> {
    match install(program, dir) {
        Ok(()) => super::plugin_args(dir),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plugin_is_written_as_claude_code_loads_one() {
        let dir = tempfile::tempdir().unwrap();
        install(Path::new("/bin/tsumugi"), dir.path()).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "tsumugi");
        let skill =
            std::fs::read_to_string(dir.path().join("skills/start-session/SKILL.md")).unwrap();
        assert!(skill.starts_with("---\nname: start-session\n"));
    }

    #[test]
    fn the_plugin_asks_tsumugi_about_the_session_s_name_after_a_clear_and_at_each_prompt() {
        let dir = tempfile::tempdir().unwrap();
        install(Path::new("/Applications/My Tsumugi/tsumugi"), dir.path()).unwrap();
        let hooks: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("hooks/hooks.json")).unwrap(),
        )
        .unwrap();
        let asked = "'/Applications/My Tsumugi/tsumugi' mcp hook";
        let start = &hooks["hooks"]["SessionStart"][0];
        assert_eq!(start["matcher"], "clear");
        assert_eq!(start["hooks"][0]["type"], "command");
        assert_eq!(start["hooks"][0]["command"], asked);
        let prompt = &hooks["hooks"]["UserPromptSubmit"][0];
        assert_eq!(prompt["hooks"][0]["command"], asked);
    }

    #[test]
    fn the_plugin_tells_tsumugi_of_each_write_before_and_after_it() {
        let dir = tempfile::tempdir().unwrap();
        install(Path::new("/bin/tsumugi"), dir.path()).unwrap();
        let hooks: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("hooks/hooks.json")).unwrap(),
        )
        .unwrap();
        for event in ["PreToolUse", "PostToolUse"] {
            let around = &hooks["hooks"][event][0];
            assert_eq!(around["matcher"], "Edit|MultiEdit|Write", "{event}");
            assert_eq!(
                around["hooks"][0]["command"], "/bin/tsumugi mcp hook",
                "{event}"
            );
        }
    }

    #[test]
    fn writing_it_again_replaces_what_an_earlier_tsumugi_wrote() {
        let dir = tempfile::tempdir().unwrap();
        install(Path::new("/old/tsumugi"), dir.path()).unwrap();
        install(Path::new("/new/tsumugi"), dir.path()).unwrap();
        let skill =
            std::fs::read_to_string(dir.path().join("skills/start-session/SKILL.md")).unwrap();
        assert!(skill.contains("/new/tsumugi session start"));
        assert!(!skill.contains("/old/tsumugi"));
    }
}
