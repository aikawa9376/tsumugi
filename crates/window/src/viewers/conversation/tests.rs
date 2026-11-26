use super::*;

fn said(role: Role, text: &str, thinking: bool) -> Message {
    Message {
        role,
        text: text.into(),
        at_ms: None,
        thinking,
        tool: None,
    }
}

fn called(name: &str, input: &str, output: Option<&str>, failed: bool) -> Message {
    Message {
        tool: Some(Tool {
            id: name.into(),
            name: name.into(),
            summary: tools_summary(input),
            input: input.into(),
            output: output.map(Into::into),
            failed,
        }),
        ..said(Role::Assistant, "", false)
    }
}

/// The summary the agents' readers give, near enough: the first line of
/// the input's first string.
fn tools_summary(input: &str) -> String {
    serde_json::from_str::<serde_json::Value>(input)
        .ok()
        .and_then(|v| {
            v.as_object()?
                .values()
                .find_map(|v| v.as_str().map(String::from))
        })
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_default()
}

#[test]
fn every_reply_addressed_to_the_user_stays_visible_and_the_user_s_words_are_bubbles() {
    let html = conversation_fragment(&[
        said(Role::User, "Fix it", false),
        said(Role::Assistant, "Checking", false),
        said(Role::Assistant, "Done **now**", false),
        said(Role::User, "Next", false),
        said(Role::Assistant, "Still working", false),
    ]);
    assert!(!html.contains("<details"));
    assert!(html.find("Checking").unwrap() < html.find("Done <strong>now</strong>").unwrap());
    assert!(html.contains("Still working"));
    assert!(html.find("Fix it").unwrap() < html.find("Next").unwrap());
    assert_eq!(html.matches("conversation-bubble").count(), 2);
}

#[test]
fn a_message_tells_when_it_was_said_for_the_page_to_put_in_the_reader_s_time() {
    let html = conversation_fragment(&[Message {
        at_ms: Some(1_700_000_000_000),
        ..said(Role::User, "Fix it", false)
    }]);
    assert!(html.contains("<time data-at=\"1700000000000\"></time>"));
}

#[test]
fn a_lone_call_is_one_line_saying_what_it_did_to_what() {
    let html = conversation_fragment(&[
        said(Role::User, "Look", false),
        called(
            "Read",
            "{\"file_path\":\"/r/a/src/main.rs\"}",
            Some("fn main"),
            false,
        ),
        said(Role::Assistant, "Seen", false),
    ]);
    assert!(!html.contains("conversation-run"));
    assert!(html.contains("<span class=\"conversation-tool-verb\">Read</span>"));
    assert!(html.contains("<span class=\"conversation-tool-target\">main.rs</span>"));
    assert!(html.contains("title=\"/r/a/src/main.rs\""));
}

#[test]
fn the_work_between_replies_folds_into_one_line_telling_what_it_did() {
    let html = conversation_fragment(&[
        said(Role::User, "Fix it", false),
        said(Role::Assistant, "Let me think", true),
        called("Read", "{\"file_path\":\"/a.rs\"}", Some("x"), false),
        called("Read", "{\"file_path\":\"/b.rs\"}", Some("x"), false),
        called("Bash", "{\"command\":\"cargo test\"}", Some("ok"), false),
        said(Role::Assistant, "Fixed", false),
    ]);
    assert_eq!(html.matches("class=\"conversation-run ").count(), 1);
    assert!(html.contains("Thought, read 2 files and ran 1 command"));
    assert!(html.contains("data-progress=\"run-1\""));
    assert!(html.contains("data-progress=\"thought-1\""));
    assert!(html.contains("data-progress=\"tool-4\""));
    assert!(!html.contains(" open"));
    assert!(html.rfind("</details>").unwrap() < html.find("Fixed").unwrap());
}

#[test]
fn the_latest_run_says_what_it_is_doing_while_a_call_runs_and_an_old_one_does_not() {
    let running = [
        said(Role::User, "Test it", false),
        called("Read", "{\"file_path\":\"/a.rs\"}", Some("x"), false),
        called("Bash", "{\"command\":\"cargo test\"}", None, false),
    ];
    let html = conversation_fragment(&running);
    assert!(html.contains("conversation-run running"));
    assert!(html.contains("Running cargo test"));
    let mut settled = running.to_vec();
    settled.push(said(Role::Assistant, "Interrupted", false));
    let html = conversation_fragment(&settled);
    assert!(!html.contains("running"));
    assert!(html.contains("Read 1 file and ran 1 command"));
}

#[test]
fn a_failed_call_is_marked_and_counted_on_its_run() {
    let html = conversation_fragment(&[
        called("Bash", "{\"command\":\"false\"}", Some("exit 1"), true),
        called("Grep", "{\"pattern\":\"x\"}", Some(""), false),
        said(Role::Assistant, "Hm", false),
    ]);
    assert!(html.contains("conversation-run failed"));
    assert!(html.contains("1 failed"));
    assert!(html.contains("conversation-tool failed"));
    assert!(html.contains("Tried to run"));
    assert!(html.contains("<b>Error</b>"));
}

#[test]
fn an_edit_opens_to_its_diff_and_counts_its_lines() {
    let input = serde_json::json!({
        "file_path": "/r/a/lib.rs",
        "old_string": "fn a() {\n    1\n}",
        "new_string": "fn a() {\n    2\n    3\n}",
    })
    .to_string();
    let html = conversation_fragment(&[called("Edit", &input, Some("ok"), false)]);
    assert!(html.contains("<span class=\"added\">+2</span><span class=\"removed\">−1</span>"));
    assert!(
        html.contains(
            "<div class=\"removed\"><span class=\"mark\">−</span><span>    1</span></div>"
        )
    );
    assert!(
        html.contains("<div class=\"added\"><span class=\"mark\">+</span><span>    3</span></div>")
    );
    assert!(
        html.contains(
            "<div class=\"same\"><span class=\"mark\"> </span><span>fn a() {</span></div>"
        )
    );
    assert!(html.contains(">lib.rs<"));
}

#[test]
fn a_codex_patch_is_read_as_a_diff_of_the_file_it_names() {
    let patch = "*** Begin Patch\n*** Update File: src/x.rs\n@@\n-old\n+new\n*** End Patch";
    let html = conversation_fragment(&[called("apply_patch", patch, Some("Done"), false)]);
    assert!(html.contains(">x.rs<"));
    assert!(html.contains("<span class=\"added\">+1</span><span class=\"removed\">−1</span>"));
    assert!(
        html.contains("<div class=\"gap\"><span class=\"mark\"></span><span>src/x.rs</span></div>")
    );
}

#[test]
fn a_command_opens_to_the_whole_command_and_what_it_printed() {
    let html = conversation_fragment(&[called(
        "Bash",
        "{\"command\":\"cargo test \\\\\\n  -p core\"}",
        Some("ok"),
        false,
    )]);
    assert!(html.contains(
        "<pre class=\"conversation-command\"><code>cargo test \\\n  -p core</code></pre>"
    ));
    assert!(html.contains("<b>Output</b><pre>ok</pre>"));
}

#[test]
fn a_plan_written_is_a_checklist_between_the_runs() {
    let input = serde_json::json!({"todos": [
        {"content": "Read", "status": "completed"},
        {"content": "Fix", "status": "in_progress"},
        {"content": "Test <it>", "status": "pending"},
    ]})
    .to_string();
    let html = conversation_fragment(&[
        called("Read", "{\"file_path\":\"/a\"}", Some(""), false),
        called("TodoWrite", &input, Some("ok"), false),
        called("Read", "{\"file_path\":\"/b\"}", Some(""), false),
    ]);
    assert!(html.contains("conversation-plan"));
    assert!(html.contains("1 of 3"));
    assert!(html.contains("<li class=\"done\">"));
    assert!(html.contains("<li class=\"doing\">"));
    assert!(html.contains("Test &lt;it&gt;"));
    assert!(html.find("title=\"/a\"").unwrap() < html.find("conversation-plan").unwrap());
    assert!(html.find("conversation-plan").unwrap() < html.find("title=\"/b\"").unwrap());
}

#[test]
fn what_a_call_was_given_and_returned_is_escaped_and_starts_folded() {
    let html = conversation_fragment(&[
        said(Role::Assistant, "Running", false),
        called(
            "Bash",
            "{\"command\":\"echo <b>\"}",
            Some("<script>x</script>"),
            true,
        ),
        called("mcp__github__get_issue", "{\"number\":\"1\"}", None, false),
    ]);
    assert!(html.contains("echo &lt;b&gt;"));
    assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains(" open"));
    assert!(html.contains("github · get_issue"));
    assert_eq!(html.matches("conversation-message").count(), 1);
}

#[test]
fn copy_preserves_markdown_and_cannot_escape_the_attribute() {
    let html =
        conversation_fragment(&[said(Role::User, "\" onmouseover=\"alert(1)\n`code`", false)]);
    assert!(html.contains("data-conversation-copy=\"&quot; onmouseover=&quot;alert(1)\n`code`\""));
    assert!(!html.contains(" onmouseover=\"alert"));
    assert!(conversation_fragment(&[]).contains("No conversation yet"));
}

#[test]
fn a_line_that_says_all_its_call_was_given_opens_to_what_it_returned_alone() {
    let html = conversation_fragment(&[
        called("Bash", "{\"command\":\"cargo test\"}", Some("ok"), false),
        called("Read", "{\"file_path\":\"/a.rs\"}", Some("fn a"), false),
        said(Role::Assistant, "Done", false),
    ]);
    assert!(!html.contains("conversation-command"));
    assert!(!html.contains("<b>Input</b>"));
    assert!(html.contains("<b>Output</b><pre>fn a</pre>"));
}

#[test]
fn a_run_totals_the_lines_its_edits_changed_and_marks_a_failure() {
    let edit = |old: &str, new: &str| {
        serde_json::json!({"file_path": "/a.rs", "old_string": old, "new_string": new}).to_string()
    };
    let html = conversation_fragment(&[
        called("Edit", &edit("a", "b\nc"), Some("ok"), false),
        called("Edit", &edit("d", "e"), Some("ok"), false),
        called("Bash", "{\"command\":\"false\"}", Some("1"), true),
        said(Role::Assistant, "Hm", false),
    ]);
    let heading = &html[..html.find("conversation-run-lines").unwrap()];
    assert!(heading.contains("<span class=\"added\">+3</span><span class=\"removed\">−2</span>"));
    assert!(heading.contains(&icon(Icon::AlertTriangle)));
}
