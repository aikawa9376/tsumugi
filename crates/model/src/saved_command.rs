//! A shell command the user keeps for a project to run again. Saving one
//! runs nothing.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The longest a label, and the text of a command, may be.
pub const MAX_COMMAND_LABEL: usize = 256;
pub const MAX_COMMAND: usize = 16 * 1024;

/// A command to keep, or to keep anew under the same id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSavedCommand {
    pub id: String,
    /// A directory of the project, which keeps it under its workspace's root.
    pub dir: PathBuf,
    pub label: String,
    pub command: String,
}

/// A command the project keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedCommand {
    pub id: String,
    /// The root of the workspace that keeps it.
    pub project: PathBuf,
    pub label: String,
    pub command: String,
}

impl SavedCommand {
    /// Whether every word of `query` appears, in any case, in the label or
    /// the command. An empty query matches every command.
    pub fn matches(&self, query: &str) -> bool {
        let haystack = format!("{}\n{}", self.label, self.command).to_lowercase();
        query
            .split_whitespace()
            .all(|word| haystack.contains(&word.to_lowercase()))
    }
}

/// What is wrong with a command to keep, if anything.
pub fn refuse_command(new: &NewSavedCommand) -> Option<&'static str> {
    if new.label.trim().is_empty() {
        return Some("A saved command needs a label");
    }
    if new.label.len() > MAX_COMMAND_LABEL {
        return Some("A label is at most 256 bytes");
    }
    if new.label.contains(['\n', '\r']) {
        return Some("A label is one line");
    }
    if new.command.trim().is_empty() {
        return Some("A saved command needs a command");
    }
    if new.command.len() > MAX_COMMAND {
        return Some("A command is at most 16384 bytes");
    }
    if new.command.contains('\0') {
        return Some("A command holds no NUL character");
    }
    if !new.dir.is_absolute() {
        return Some("A project is an absolute directory");
    }
    None
}

/// The program and arguments that run `script` in `shell`, as one argument
/// the shell reads whole.
pub fn shell_line(shell: &str, script: &str) -> Vec<String> {
    // Split by hand: the shell may be a Windows path read on another system.
    let file = shell.rsplit(['/', '\\']).next().unwrap_or_default();
    let name = file
        .rsplit_once('.')
        .map_or(file, |(stem, _)| stem)
        .to_lowercase();
    let flag = match name.as_str() {
        "cmd" => "/C",
        "powershell" | "pwsh" => "-Command",
        _ => "-c",
    };
    vec![shell.to_owned(), flag.to_owned(), script.to_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(label: &str, command: &str) -> NewSavedCommand {
        NewSavedCommand {
            id: "c".into(),
            dir: "/p".into(),
            label: label.into(),
            command: command.into(),
        }
    }

    fn saved(label: &str, command: &str) -> SavedCommand {
        SavedCommand {
            id: "c".into(),
            project: "/p".into(),
            label: label.into(),
            command: command.into(),
        }
    }

    #[test]
    fn a_query_matches_when_each_of_its_words_is_in_the_label_or_the_command() {
        let c = saved("Checks", "just ci\ncargo test");
        assert!(c.matches(""));
        assert!(c.matches("CHECKS"));
        assert!(c.matches("cargo ci"));
        assert!(c.matches("checks test"));
        assert!(!c.matches("checks npm"));
    }

    #[test]
    fn a_query_in_unicode_matches_in_any_case() {
        let c = saved("ビルド ÄÖ", "make 全部");
        assert!(c.matches("ビルド äö"));
        assert!(c.matches("全部"));
    }

    #[test]
    fn a_command_is_kept_only_with_a_label_and_a_command() {
        assert_eq!(refuse_command(&new("Checks", "just ci")), None);
        assert!(refuse_command(&new("", "just ci")).is_some());
        assert!(refuse_command(&new("  ", "just ci")).is_some());
        assert!(refuse_command(&new("Checks", "")).is_some());
        assert!(refuse_command(&new("Checks", " \n ")).is_some());
    }

    #[test]
    fn a_command_may_span_lines_and_hold_any_text_but_a_nul() {
        assert_eq!(refuse_command(&new("Setup", "cd 'a b'\nls 日本語\n")), None);
        assert!(refuse_command(&new("Setup", "a\0b")).is_some());
        assert!(refuse_command(&new("two\nlines", "ls")).is_some());
    }

    #[test]
    fn a_label_and_a_command_are_kept_only_short() {
        assert_eq!(
            refuse_command(&new(&"l".repeat(MAX_COMMAND_LABEL), "x")),
            None
        );
        assert!(refuse_command(&new(&"l".repeat(MAX_COMMAND_LABEL + 1), "x")).is_some());
        assert_eq!(refuse_command(&new("l", &"c".repeat(MAX_COMMAND))), None);
        assert!(refuse_command(&new("l", &"c".repeat(MAX_COMMAND + 1))).is_some());
    }

    #[test]
    fn a_command_is_kept_only_in_an_absolute_directory() {
        let mut kept = new("l", "x");
        kept.dir = "p".into();
        assert!(refuse_command(&kept).is_some());
    }

    #[test]
    fn the_text_reaches_the_shell_as_one_argument_whatever_it_holds() {
        let script = "echo 'a b' \"$HOME\"\nls 日本語";
        assert_eq!(
            shell_line("/bin/zsh", script),
            ["/bin/zsh", "-c", script].map(String::from)
        );
        assert_eq!(shell_line("C:\\Windows\\System32\\cmd.exe", "dir")[1], "/C");
        assert_eq!(shell_line("pwsh", "ls")[1], "-Command");
    }
}
