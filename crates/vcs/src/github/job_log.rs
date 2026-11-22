//! The log of one check's Actions job, read to be looked at rather than put
//! in a prompt: the same end of it a fix request quotes, and the line the
//! first error shows at. Only reads.

use std::path::Path;

use tsumugi_model::github::{CheckLog, Log};

use super::fix::{EARLIER_SEPARATOR, read_log};
use super::{reason, remote};
use crate::{Error, run};

/// How many lines after an error are looked through for the stack that
/// belongs to it.
const STACK_REACH: usize = 12;

/// The log of the Actions job `job` of the repository at `root`, as far as it
/// can be read. A job still running has no final log, so it is told so
/// rather than read.
pub async fn job_log(root: &Path, job: u64) -> CheckLog {
    let Some(remote) = remote(root).await else {
        return unread("the repository has no remote on GitHub".into());
    };
    let slug = remote.slug;
    let path = format!("repos/{slug}/actions/jobs/{job}");
    match run("gh", &["api", &path, "--jq", ".status"], Some(root)).await {
        Ok(status) => {
            if let Some(why) = unfinished(status.trim()) {
                return unread(why);
            }
        }
        Err(Error::Failed { stderr, .. }) => return unread(reason(&stderr)),
        Err(e) => return unread(e.to_string()),
    }
    match read_log(root, &slug, job).await {
        log @ Log::Excerpt { .. } => {
            let first_error = match &log {
                Log::Excerpt { text, .. } => first_error(text),
                Log::Unread { .. } => None,
            };
            CheckLog { log, first_error }
        }
        log => CheckLog {
            log,
            first_error: None,
        },
    }
}

fn unread(why: String) -> CheckLog {
    CheckLog {
        log: Log::Unread { why },
        first_error: None,
    }
}

/// Why a job in this `status` has no final log, or none when it has one.
fn unfinished(status: &str) -> Option<String> {
    match status {
        "queued" | "waiting" | "pending" | "requested" => {
            Some("the check has not started yet, so it has no log".into())
        }
        "in_progress" => Some("the check is still running, so it has no final log".into()),
        _ => None,
    }
}

/// The number of the line of `text`, the end of a log, at which its first
/// error shows: in the lines after the marker of the earlier errors left out
/// before it, as they are the log's own end, else before it. Errors that say
/// only that a process exited, or that something failed, are passed over for
/// one that says what.
pub(super) fn first_error(text: &str) -> Option<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let ends = match lines.iter().position(|l| *l == EARLIER_SEPARATOR) {
        Some(separator) => vec![(separator + 1, lines.len()), (0, separator)],
        None => vec![(0, lines.len())],
    };
    for (from, to) in ends {
        let mut inline = None;
        for at in from..to {
            let line = lines[at];
            if line.trim_start().starts_with("at ") {
                continue;
            }
            let Some(start) = diagnostic(line) else {
                continue;
            };
            let candidate = &line[start..];
            let body = candidate
                .strip_prefix("##[error]")
                .or_else(|| strip_annotation(candidate))
                .unwrap_or(candidate)
                .trim();
            if body.is_empty()
                || generic_exit(body)
                || (generic_heading(body) && diagnostic(body).is_none())
            {
                continue;
            }
            let stacked = lines[at + 1..to.min(at + 1 + STACK_REACH)]
                .iter()
                .take_while(|l| diagnostic(l).is_none())
                .any(|l| frame(l));
            // An error a program logs in the middle of a line is weaker
            // evidence than one that opens it, unless a stack follows.
            let opens = timestamp_only(&line[..start])
                || candidate.starts_with("##[error]")
                || candidate.starts_with("::error");
            if opens || stacked {
                return Some(at);
            }
            inline.get_or_insert(at);
        }
        if inline.is_some() {
            return inline;
        }
    }
    None
}

/// What follows the `::error ...::` of a workflow command, when `text` opens
/// with one.
fn strip_annotation(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("::error")?;
    let end = rest.find("::")?;
    Some(&rest[end + 2..])
}

/// Where in `line` a diagnostic starts, as a byte offset.
fn diagnostic(line: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut keep = |at: usize| best = Some(best.map_or(at, |b| b.min(at)));
    if let Some(at) = line.find("##[error]") {
        keep(at);
    }
    if let Some(at) = line.find("::error")
        && line[at + 7..].starts_with([':', ' ', '\t'])
    {
        keep(at);
    }
    // `Error: x`, `TypeError: x`, `error: x`, `panic: x`.
    for (colon, _) in line.match_indices(':') {
        let before = &line[..colon];
        let start = before
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .map_or(0, |i| {
                i + before[i..].chars().next().map_or(1, char::len_utf8)
            });
        let word = &before[start..];
        let named = matches!(word, "Error" | "error" | "panic")
            || (word.starts_with(|c: char| c.is_ascii_uppercase()) && word.ends_with("Error"));
        if named && line[colon + 1..].trim_start().chars().next().is_some() {
            keep(start);
        }
    }
    // `error[E0425]: x`, as rustc says it.
    for (at, _) in line.match_indices("error[") {
        let word_start = !line[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_');
        if word_start
            && line[at..]
                .find("]:")
                .is_some_and(|i| !line[at + i + 2..].trim().is_empty())
        {
            keep(at);
        }
    }
    // `error TS2304: x`.
    for (at, _) in line.match_indices("error ") {
        let rest = &line[at + 6..];
        let letters = rest.chars().take_while(char::is_ascii_uppercase).count();
        let digits = rest[letters..]
            .chars()
            .take_while(char::is_ascii_digit)
            .count();
        if letters > 0
            && digits > 0
            && rest[letters + digits..].starts_with(':')
            && rest[letters + digits + 1..]
                .trim_start()
                .chars()
                .next()
                .is_some()
        {
            keep(at);
        }
    }
    for code in ["ENOENT", "EACCES"] {
        if let Some(at) = line.find(code) {
            let rest = line[at + code.len()..].trim_start_matches([':', ' ', ',']);
            if rest.len() < line.len() - at - code.len() && !rest.is_empty() {
                keep(at);
            }
        }
    }
    best
}

/// A line that says only that a process exited with a code.
fn generic_exit(body: &str) -> bool {
    let body = body.trim_end_matches(['.', '!']).to_lowercase();
    let words: Vec<&str> = body.split_whitespace().collect();
    words.len() <= 7
        && words.contains(&"code")
        && words
            .last()
            .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
        && !words.is_empty()
}

/// A line that only heads what follows: `FAILED`, `3 tests failed`, `stderr`.
fn generic_heading(body: &str) -> bool {
    let body = body.trim_end_matches(['.', '!']);
    let lower = body.to_lowercase();
    body == "FAILED"
        || lower == "stderr"
        || lower.starts_with("stderr |")
        || lower.starts_with("test stderr")
        || {
            let mut words = lower.split_whitespace();
            words
                .next()
                .is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
                && matches!(words.next(), Some("test" | "tests"))
                && words.next() == Some("failed")
                && words.next().is_none()
        }
}

/// Whether `prefix` is empty or a timestamp, as every line of a job's log
/// begins.
fn timestamp_only(prefix: &str) -> bool {
    let prefix = prefix.trim();
    prefix.is_empty()
        || (prefix.len() >= 19
            && prefix.as_bytes()[4] == b'-'
            && prefix.as_bytes()[10] == b'T'
            && prefix
                .chars()
                .all(|c| c.is_ascii_digit() || "-:T.Z+".contains(c)))
}

/// Whether `line` is a stack frame: `at f` or a position `file.ext:12:3`.
fn frame(line: &str) -> bool {
    line.trim_start().starts_with("at ")
        || line.split_whitespace().any(|w| {
            let mut parts = w.rsplitn(3, ':');
            let (col, row, file) = (parts.next(), parts.next(), parts.next());
            matches!((col, row, file), (Some(c), Some(r), Some(f))
                if !c.is_empty() && c.trim_end_matches(')').chars().all(|d| d.is_ascii_digit())
                    && !r.is_empty() && r.chars().all(|d| d.is_ascii_digit())
                    && f.contains('.'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(log: &[&str]) -> Option<usize> {
        first_error(&log.join("\n"))
    }

    #[test]
    fn a_log_with_no_error_has_no_first_error() {
        assert_eq!(at(&["Run cargo test", "all passed"]), None);
    }

    #[test]
    fn the_first_error_is_the_first_line_that_says_what_went_wrong() {
        let log = [
            "Run cargo test",
            "2026-01-02T03:04:05.0000000Z error[E0425]: cannot find value",
            "2026-01-02T03:04:06.0000000Z error: could not compile",
        ];
        assert_eq!(at(&log), Some(1));
        let log = [
            "Run npm test",
            "2026-01-02T03:04:05.0000000Z ##[error]TypeError: x is not a function",
            "2026-01-02T03:04:06.0000000Z ##[error]Process completed with exit code 1.",
        ];
        assert_eq!(at(&log), Some(1));
    }

    #[test]
    fn a_line_that_only_says_a_process_exited_is_passed_over() {
        let log = [
            "##[error]Process completed with exit code 1.",
            "Error: expected 2, got 3",
        ];
        assert_eq!(at(&log), Some(1));
        // With nothing better, there is no first error to scroll to.
        assert_eq!(at(&["##[error]Process completed with exit code 1."]), None);
    }

    #[test]
    fn a_workflow_command_error_names_its_message() {
        assert_eq!(
            at(&["ok", "::error file=a.rs,line=1::unused variable"]),
            Some(1)
        );
        assert_eq!(at(&["::error::"]), None);
    }

    #[test]
    fn a_stack_frame_is_not_an_error_and_a_heading_alone_is_passed_over() {
        let log = [
            "##[error]FAILED",
            "    at Error: foo (a.js:1:2)",
            "panic: boom",
        ];
        assert_eq!(at(&log), Some(2));
    }

    #[test]
    fn a_program_that_logs_an_error_mid_line_counts_only_when_nothing_better_comes() {
        let log = [
            "worker said hello Error: cache miss",
            "2026-01-02T03:04:05.0000000Z ENOENT: no such file",
        ];
        assert_eq!(at(&log), Some(1));
        assert_eq!(at(&["worker said hello Error: cache miss"]), Some(0));
        // A stack after it makes it the one.
        assert_eq!(
            at(&[
                "worker said hello Error: cache miss",
                "  at run (a.js:3:4)",
                "ENOENT: x"
            ]),
            Some(0)
        );
    }

    #[test]
    fn the_lines_after_the_earlier_errors_marker_are_looked_through_first() {
        let log = [
            "error: could not install",
            "noise",
            EARLIER_SEPARATOR,
            "later",
            "panic: the real failure",
        ];
        assert_eq!(at(&log), Some(4));
        assert_eq!(at(&[log[0], log[1], log[2], "later"]), Some(0));
    }

    #[test]
    fn a_job_that_has_not_finished_has_no_log_and_says_why() {
        assert!(unfinished("in_progress").unwrap().contains("still running"));
        assert!(unfinished("queued").unwrap().contains("not started"));
        assert_eq!(unfinished("completed"), None);
    }
}
