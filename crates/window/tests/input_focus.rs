//! The page must retain native controls and input-method keys.
#[test]
fn dialog_controls_and_input_method_selectors_keep_their_keys() {
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
