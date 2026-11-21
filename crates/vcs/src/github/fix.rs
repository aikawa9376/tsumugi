//! What a request to fix a pull request's checks needs that is read afresh:
//! the commit the pull request ends at now, and, of each check that failed,
//! the end of its job's log. Only reads; nothing is written to GitHub.

use std::path::Path;

use tsumugi_model::github::{Check, FixCheck, FixContext, Log, Outcome};

use super::thread::{api, checks, head_of};
use super::{reason, remote};
use crate::{Error, run};

/// How many failed checks have their logs read: each is a request to GitHub
/// and a block of the prompt.
const LOGGED: usize = 5;
/// The lines kept from the end of a log, and of them the most recent ones
/// that earlier errors make room beside.
const TAIL_LINES: usize = 200;
const RECENT_LINES: usize = 100;
/// The earlier lines kept for the errors that come before the recent ones,
/// and the lines around each error kept with it.
const EARLIER_LINES: usize = 30;
const ERROR_CONTEXT: usize = 2;
/// The most bytes of an excerpt.
const EXCERPT_BYTES: usize = 16 * 1024;
const EARLIER_SEPARATOR: &str = "… earlier errors …";

/// The pull request `number` of the repository at `root`, as it ends now;
/// with `with_checks`, the checks on that commit that failed or still run,
/// each failed one with the end of its log where it can be read.
pub async fn fix_context(
    root: &Path,
    number: u64,
    with_checks: bool,
) -> Result<FixContext, String> {
    let slug = remote(root)
        .await
        .ok_or("the repository has no remote on GitHub")?
        .slug;
    let pull = api(root, &format!("repos/{slug}/pulls/{number}"), None).await?;
    let head = head_of(&pull).ok_or("GitHub answered a pull request with no head")?;
    if !with_checks {
        return Ok(FixContext {
            head,
            checks: Vec::new(),
        });
    }
    let runs_path = format!("repos/{slug}/commits/{head}/check-runs?per_page=100");
    let statuses_path = format!("repos/{slug}/commits/{head}/status");
    let (runs, statuses) = tokio::join!(
        api(root, &runs_path, Some(".check_runs[]")),
        api(root, &statuses_path, Some(".statuses[]")),
    );
    // Neither can be read: say so, rather than a pull request with no
    // failures.
    let (runs, statuses) = match (runs, statuses) {
        (Err(wrong), Err(_)) => return Err(wrong),
        (runs, statuses) => (runs.unwrap_or_default(), statuses.unwrap_or_default()),
    };
    let mut fixes = Vec::new();
    for (check, plan) in plan(checks(&runs, &statuses)) {
        let log = match plan {
            Plan::Read(job) => read_log(root, &slug, job).await,
            Plan::Skip(why) => Log::Unread { why },
        };
        fixes.push(FixCheck { check, log });
    }
    Ok(FixContext {
        head,
        checks: fixes,
    })
}

/// How the log of a check is come by.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    /// Read the log of the Actions job with this id.
    Read(u64),
    /// Leave it unread, for this reason.
    Skip(String),
}

/// The checks of `all` that did not pass, those that failed first, and for
/// each how its log is come by.
fn plan(all: Vec<Check>) -> Vec<(Check, Plan)> {
    let mut read = 0;
    all.into_iter()
        .filter(|c| matches!(c.outcome, Outcome::Failed | Outcome::Pending))
        .map(|check| {
            let plan = if check.outcome == Outcome::Pending {
                Plan::Skip("the check is still running, so it has no final log".into())
            } else if let Some(job) = job_of(&check) {
                if read < LOGGED {
                    read += 1;
                    Plan::Read(job)
                } else {
                    Plan::Skip(format!(
                        "only the logs of the first {LOGGED} failed checks are read"
                    ))
                }
            } else {
                Plan::Skip(
                    "the check was not run by GitHub Actions, so Tsumugi cannot read its log; its link tells more"
                        .into(),
                )
            };
            (check, plan)
        })
        .collect()
}

/// The id of the Actions job a check run is, from the address it tells
/// more at.
fn job_of(check: &Check) -> Option<u64> {
    if check.by != "GitHub Actions" {
        return None;
    }
    let after = |marker: &str| -> Option<u64> {
        let rest = check.link.split_once(marker)?.1;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    // `/actions/runs/<run>/job/<job>` names the job; a check run's own page,
    // `/runs/<id>`, is the job's id too; `/actions/runs/<run>` alone is the
    // whole run, whose jobs are not known.
    after("/job/").or_else(|| {
        let before = check.link.split_once("/runs/")?.0;
        (!before.ends_with("/actions")).then(|| after("/runs/"))?
    })
}

/// The end of the log of the Actions job `job`, or why it could not be read.
async fn read_log(root: &Path, slug: &str, job: u64) -> Log {
    let path = format!("repos/{slug}/actions/jobs/{job}/logs");
    match run("gh", &["api", &path], Some(root)).await {
        Ok(out) => {
            let (text, cut) = excerpt(&out);
            Log::Excerpt { text, cut }
        }
        Err(Error::Failed { stderr, .. }) => Log::Unread {
            why: reason(&stderr),
        },
        Err(e) => Log::Unread { why: e.to_string() },
    }
}

/// `text` without terminal escape sequences and control characters, which
/// would be typed into an agent's prompt.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                // CSI: up to a final byte of `@` to `~`.
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: up to a bell or a string terminator.
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            },
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            '\u{feff}' | '\u{200b}' | '\u{2060}' => {}
            c => out.push(c),
        }
    }
    out
}

fn tail(text: &str) -> &str {
    tail_within(text, EXCERPT_BYTES)
}

fn is_error(line: &str) -> bool {
    const MARKERS: [&str; 8] = [
        "##[error]",
        "::error",
        "error:",
        "failed",
        "exit code",
        "enoent",
        "eacces",
        "panic:",
    ];
    let lower = line.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m)) || line.contains("AssertionError")
}

/// What of `log` goes to the prompt: its last lines, with the lines around
/// the earlier errors in front of them where installing and caching output
/// buried those, within `EXCERPT_BYTES`; and whether earlier lines were left
/// out.
pub(super) fn excerpt(log: &str) -> (String, bool) {
    let log = plain(log);
    let lines: Vec<&str> = log.trim_end_matches('\n').split('\n').collect();
    let recent_from = lines.len().saturating_sub(RECENT_LINES);
    if recent_from == 0 {
        return (tail(&lines.join("\n")).to_string(), false);
    }
    // The earlier error lines, newest first, each with its context, until
    // there are enough.
    let mut earlier = std::collections::BTreeSet::new();
    'found: for at in (0..recent_from).rev() {
        if !is_error(lines[at]) {
            continue;
        }
        let from = at.saturating_sub(ERROR_CONTEXT);
        let to = (at + ERROR_CONTEXT).min(recent_from - 1);
        for kept in (from..=to).rev() {
            earlier.insert(kept);
            if earlier.len() == EARLIER_LINES {
                break 'found;
            }
        }
    }
    if earlier.is_empty() {
        let from = lines.len().saturating_sub(TAIL_LINES);
        return (tail(&lines[from..].join("\n")).to_string(), from > 0);
    }
    let mut head: Vec<&str> = earlier.iter().map(|&i| lines[i]).collect();
    head.push(EARLIER_SEPARATOR);
    let head = head.join("\n");
    if head.len() >= EXCERPT_BYTES {
        return (tail(&head).to_string(), true);
    }
    let room = EXCERPT_BYTES - head.len() - 1;
    let recent = lines[recent_from..].join("\n");
    (format!("{head}\n{}", tail_within(&recent, room)), true)
}

/// The tail of `text` that fits `room` bytes, from a character boundary.
fn tail_within(text: &str, room: usize) -> &str {
    if text.len() <= room {
        return text;
    }
    let mut start = text.len() - room;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(count: usize) -> String {
        (1..=count)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn check(name: &str, outcome: Outcome, by: &str, link: &str) -> Check {
        Check {
            name: name.into(),
            by: by.into(),
            outcome,
            link: link.into(),
            said: String::new(),
        }
    }

    fn job(name: &str, id: u64) -> Check {
        check(
            name,
            Outcome::Failed,
            "GitHub Actions",
            &format!("https://github.com/o/r/actions/runs/9/job/{id}"),
        )
    }

    #[test]
    fn a_short_log_is_kept_whole() {
        let (text, cut) = excerpt("one\ntwo\nthree\n");
        assert_eq!(text, "one\ntwo\nthree");
        assert!(!cut);
    }

    #[test]
    fn a_long_log_keeps_its_end_and_says_that_lines_were_left_out() {
        let (text, cut) = excerpt(&numbered(1000));
        assert!(cut);
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines.last(), Some(&"line 1000"));
        assert_eq!(lines.first(), Some(&"line 801"));
    }

    #[test]
    fn errors_before_the_recent_lines_are_kept_with_their_neighbours() {
        let mut lines: Vec<String> = (1..=500).map(|n| format!("noise {n}")).collect();
        lines[9] = "error: could not install".into();
        let (text, cut) = excerpt(&lines.join("\n"));
        assert!(cut);
        let kept: Vec<_> = text.lines().collect();
        assert_eq!(
            &kept[..6],
            [
                "noise 8",
                "noise 9",
                "error: could not install",
                "noise 11",
                "noise 12",
                EARLIER_SEPARATOR
            ]
        );
        assert_eq!(kept.last(), Some(&"noise 500"));
        assert_eq!(kept.len(), 6 + RECENT_LINES);
    }

    #[test]
    fn an_excerpt_never_passes_its_bytes_and_ends_on_a_character() {
        let long = "あ".repeat(EXCERPT_BYTES);
        let (text, _) = excerpt(&long);
        assert!(text.len() <= EXCERPT_BYTES);
        assert!(text.chars().all(|c| c == 'あ'));
    }

    #[test]
    fn escape_sequences_and_control_characters_do_not_reach_the_prompt() {
        let (text, _) = excerpt("\x1b[31mred\x1b[0m\r\nbell\x07\x1b]0;title\x07 ok\rnext\tx\n");
        assert_eq!(text, "red\nbell ok\nnext\tx");
    }

    #[test]
    fn the_job_of_an_actions_check_comes_from_its_address() {
        assert_eq!(job_of(&job("a", 42)), Some(42));
        let run_page = check(
            "a",
            Outcome::Failed,
            "GitHub Actions",
            "https://github.com/o/r/runs/77?check_suite_focus=true",
        );
        assert_eq!(job_of(&run_page), Some(77));
    }

    #[test]
    fn no_job_is_named_by_a_whole_run_a_status_or_another_app() {
        let whole = check(
            "a",
            Outcome::Failed,
            "GitHub Actions",
            "https://github.com/o/r/actions/runs/9",
        );
        let other = check("b", Outcome::Failed, "Buildkite", "https://x/runs/3");
        let status = check("c", Outcome::Failed, "", "https://y/job/4");
        assert_eq!(job_of(&whole), None);
        assert_eq!(job_of(&other), None);
        assert_eq!(job_of(&status), None);
    }

    #[test]
    fn those_that_passed_or_were_skipped_are_left_out_and_failed_ones_come_first() {
        let planned = plan(vec![
            job("lint", 1),
            check("test", Outcome::Pending, "GitHub Actions", ""),
            check("build", Outcome::Passed, "GitHub Actions", ""),
            check("docs", Outcome::Skipped, "GitHub Actions", ""),
        ]);
        let names: Vec<_> = planned.iter().map(|(c, _)| c.name.as_str()).collect();
        assert_eq!(names, ["lint", "test"]);
        assert_eq!(planned[0].1, Plan::Read(1));
        assert!(matches!(&planned[1].1, Plan::Skip(why) if why.contains("still running")));
    }

    #[test]
    fn only_the_first_failed_checks_have_their_logs_read_and_the_rest_say_so() {
        let planned = plan((1..=7).map(|n| job(&format!("job {n}"), n)).collect());
        let read = planned
            .iter()
            .filter(|(_, p)| matches!(p, Plan::Read(_)))
            .count();
        assert_eq!(read, LOGGED);
        assert!(matches!(&planned[5].1, Plan::Skip(why) if why.contains("first 5")));
    }

    #[test]
    fn a_check_gh_cannot_read_the_log_of_keeps_its_name_and_link_with_a_reason() {
        let planned = plan(vec![check(
            "ci/old",
            Outcome::Failed,
            "",
            "https://y/build/1",
        )]);
        assert_eq!(planned[0].0.link, "https://y/build/1");
        assert!(
            matches!(&planned[0].1, Plan::Skip(why) if why.contains("not run by GitHub Actions"))
        );
    }
}
