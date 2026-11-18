//! Linux desktop commands take literal arguments, never shell snippets.
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn clipboard_command(wayland: bool) -> Command {
    if wayland {
        Command::new("wl-copy")
    } else {
        let mut command = Command::new("xclip");
        command.args(["-selection", "clipboard", "-in"]);
        command
    }
}

fn copy_using(text: &str, command: &mut Command) -> io::Result<()> {
    let mut child = command.stdin(Stdio::piped()).spawn()?;
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(text.as_bytes());
    let status = child.wait()?;
    written?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "clipboard helper exited with {status}"
        )))
    }
}

/// Copy UTF-8 text through the current session's clipboard helper.
pub fn copy_to_clipboard(text: &str) -> io::Result<()> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|s| !s.is_empty());
    copy_using(text, &mut clipboard_command(wayland))
}

/// Translate the window's open/reveal/edit requests to a freedesktop opener.
pub fn open_command(args: &[OsString]) -> Option<Command> {
    let target = match args {
        [how, path] if how == "-R" => Path::new(path).parent()?.as_os_str().to_owned(),
        [how, path] if how == "-t" => path.clone(),
        [target] => target.clone(),
        _ => return None,
    };
    let target = if target.to_string_lossy().starts_with('-') {
        Path::new(".").join(target).into_os_string()
    } else {
        target
    };
    let mut command = Command::new("xdg-open");
    command.arg(target);
    Some(command)
}

/// A desktop notification with literal summary and escaped body markup.
pub fn notification_command(name: &str, why: &str) -> Command {
    let body = why
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let mut command = Command::new("notify-send");
    command.args(["--app-name=Tsumugi", "--icon=tsumugi", "--", name, &body]);
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::process::Command;

    fn args(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn copying_uses_the_active_display_protocol() {
        let wayland = clipboard_command(true);
        assert_eq!(wayland.get_program(), "wl-copy");
        let x11 = clipboard_command(false);
        assert_eq!(x11.get_program(), "xclip");
        assert_eq!(args(&x11), ["-selection", "clipboard", "-in"]);
    }

    #[test]
    fn revealing_a_file_opens_its_parent_and_editing_opens_the_file() {
        let path = OsString::from("/tmp/日本語 folder/config.toml");
        let reveal = open_command(&["-R".into(), path.clone()]).unwrap();
        assert_eq!(reveal.get_program(), "xdg-open");
        assert_eq!(args(&reveal), ["/tmp/日本語 folder"]);
        assert_eq!(
            args(&open_command(&["-t".into(), path]).unwrap()),
            ["/tmp/日本語 folder/config.toml"]
        );
        assert!(open_command(&[]).is_none());
    }

    #[test]
    fn open_targets_are_literal_and_cannot_become_options() {
        assert_eq!(
            args(&open_command(&["-strange.pdf".into()]).unwrap()),
            ["./-strange.pdf"]
        );
        let url = "https://example.com/?q=$(touch%20/tmp/unwanted)";
        assert_eq!(args(&open_command(&[url.into()]).unwrap()), [url]);
    }

    #[test]
    fn notification_text_stays_literal_and_markup_is_escaped() {
        let command = notification_command("-dangerous", "<b>日本語</b> & waiting");
        assert_eq!(command.get_program(), "notify-send");
        assert_eq!(
            args(&command),
            [
                "--app-name=Tsumugi",
                "--icon=tsumugi",
                "--",
                "-dangerous",
                "&lt;b&gt;日本語&lt;/b&gt; &amp; waiting"
            ]
        );
    }

    #[test]
    fn a_failed_clipboard_helper_is_reported() {
        assert!(copy_using("hello", &mut Command::new("false")).is_err());
    }
}
