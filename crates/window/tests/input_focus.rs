//! The page must retain dialog controls while following the terminal cursor.
#[test]
fn dialog_controls_keep_the_keys_until_the_dialog_is_gone() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/input_focus.cjs"
        ))
        .output()
        .expect("Node.js runs the page input regression");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
