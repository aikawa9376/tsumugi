//! Telling the user, outside the window, that a session waits on them.

/// The AppleScript that shows a notification, its words quoted so none can
/// end the string they are in.
#[cfg(any(test, target_os = "macos"))]
fn script(title: &str, subtitle: &str, body: &str) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    format!(
        "display notification {} with title {} subtitle {}",
        q(body),
        q(title),
        q(subtitle)
    )
}

/// Show a notification that the session `name` waits, for `why`, once the
/// future it gives is run.
#[cfg(target_os = "macos")]
pub fn notify_waiting(name: &str, why: &str) -> impl std::future::Future<Output = ()> + use<> {
    let mut cmd = tokio::process::Command::new("osascript");
    cmd.args(["-e", &script("Tsumugi", name, why)])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    async move {
        let _ = cmd.status().await;
    }
}

/// Nothing shown: Windows has no notification a program can show without
/// registering as an app, and the window still marks the session waiting.
#[cfg(windows)]
pub fn notify_waiting(_: &str, _: &str) -> impl std::future::Future<Output = ()> + use<> {
    std::future::ready(())
}

/// Use the notification daemon of the Linux desktop session.
#[cfg(target_os = "linux")]
pub fn notify_waiting(name: &str, why: &str) -> impl std::future::Future<Output = ()> + use<> {
    let mut command =
        tokio::process::Command::from(tsumugi_sys::desktop::notification_command(name, why));
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    async move {
        match command.status().await {
            Ok(status) if status.success() => {}
            result => tracing::warn!(?result, "desktop notification could not be delivered"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_s_words_cannot_break_out_of_the_script() {
        assert_eq!(
            script("Tsumugi", "say \"hi\"", "a\\b"),
            r#"display notification "a\\b" with title "Tsumugi" subtitle "say \"hi\"""#
        );
    }
}
