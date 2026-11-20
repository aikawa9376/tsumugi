#[test]
fn conversation_updates_keep_the_reading_position_and_expanded_progress() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/conversation.cjs"
        ))
        .output()
        .expect("Node.js runs the conversation regression");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
