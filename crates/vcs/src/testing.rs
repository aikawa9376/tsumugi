//! What the tests of this crate start from.

use std::path::Path;

/// Write the shell script `text` to `path` and make it executable. It is
/// written from a child of its own: a program written by this process cannot
/// be run while a child another test forked still holds the file open for
/// writing, which Linux refuses as busy; the child that writes it has ended
/// before anyone runs it.
pub(crate) fn program(path: &Path, text: &str) {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    let mut writing = std::process::Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\"", "sh"])
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    writing
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    assert!(writing.wait().unwrap().success());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
