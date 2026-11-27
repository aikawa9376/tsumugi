//! Review comments on a file's line diff: what they are pinned to, whether
//! what they are pinned to still reads as it did, and the prompt that
//! carries them to an agent.

use serde::{Deserialize, Serialize};

use crate::repository::{DiffKind, DiffLine, Rev};

/// Which side of a change the lines are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Side {
    /// The file before the change.
    Old,
    /// The file after the change.
    New,
}

/// The lines of one side of a file a comment is about, counted from 1.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Anchor {
    pub path: String,
    pub side: Side,
    pub start: u32,
    /// The last line, never before `start`.
    pub end: u32,
}

/// A remark on lines of a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    /// The change the diff was read from.
    pub rev: Rev,
    pub anchor: Anchor,
    /// The text of the lines as they were when commented, one entry each.
    pub quoted: Vec<String>,
    pub text: String,
    /// Set by a person alone.
    pub resolved: bool,
    /// Whether a prompt carrying it went to an agent.
    pub sent: bool,
    /// The session whose agent left it as a note, by its code and name; a
    /// person's comment has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Where it is seen on GitHub, for a review thread of the branch's pull
    /// request drawn in the diff; Tsumugi never keeps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
}

impl Comment {
    /// Whether an agent left it, as a note, rather than a person.
    pub fn is_note(&self) -> bool {
        self.author.is_some() && self.github.is_none()
    }

    /// Whether it is a review thread of GitHub's, drawn but not kept.
    pub fn is_github(&self) -> bool {
        self.github.is_some()
    }

    /// Whether a prompt to an agent carries it: the user's own, open.
    pub fn is_carried(&self) -> bool {
        !self.resolved && self.author.is_none() && self.github.is_none()
    }
}

/// A comment to keep, before the work host gives it an id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewComment {
    pub rev: Rev,
    pub anchor: Anchor,
    pub quoted: Vec<String>,
    pub text: String,
    /// The session whose agent leaves it as a note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// Whether the lines a comment was made on still read as they did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Freshness {
    Current,
    /// The lines changed or are gone, or the diff cannot be read.
    Outdated,
}

/// A comment, and how it stands against the diff as it is now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Judged {
    pub comment: Comment,
    pub freshness: Freshness,
}

/// What a person changes of the comments kept for a worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    Add(NewComment),
    Edit {
        id: u64,
        text: String,
    },
    Remove {
        id: u64,
    },
    Resolve {
        id: u64,
        resolved: bool,
    },
    /// A prompt carrying these went to an agent.
    Sent {
        ids: Vec<u64>,
    },
}

/// The line numbers a diff line has on each side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Numbers {
    pub old: Option<u32>,
    pub new: Option<u32>,
}

/// Why lines cannot be commented on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// A line that is no line of the file: a file or hunk heading, or a
    /// diff read as structural text.
    NoLine,
    AcrossHunks,
    /// Removed and added lines together are lines of both sides.
    BothSides,
}

impl Refusal {
    pub fn words(self) -> &'static str {
        match self {
            Refusal::NoLine => "Comment on lines of the file, not on headings",
            Refusal::AcrossHunks => "Comment on lines of one hunk at a time",
            Refusal::BothSides => "Comment on removed or on added lines, not on both",
        }
    }
}

/// The numbers of each line of a unified diff, read from its hunk headings.
pub fn numbered(lines: &[DiffLine]) -> Vec<Numbers> {
    let mut next: Option<(u32, u32)> = None;
    lines
        .iter()
        .map(|line| match line.kind {
            DiffKind::File => {
                next = None;
                Numbers::default()
            }
            DiffKind::Hunk => {
                next = hunk_start(&line.text);
                Numbers::default()
            }
            DiffKind::Difft => Numbers::default(),
            // `\ No newline at end of file` belongs to no line.
            DiffKind::Context if line.text.starts_with('\\') => Numbers::default(),
            kind => {
                let Some((old, new)) = next.as_mut() else {
                    return Numbers::default();
                };
                let at = Numbers {
                    old: (kind != DiffKind::Added).then_some(*old),
                    new: (kind != DiffKind::Removed).then_some(*new),
                };
                if at.old.is_some() {
                    *old += 1;
                }
                if at.new.is_some() {
                    *new += 1;
                }
                at
            }
        })
        .collect()
}

/// Where a hunk heading, `@@ -a,b +c,d @@`, says its lines start.
fn hunk_start(heading: &str) -> Option<(u32, u32)> {
    let mut words = heading.strip_prefix("@@ ")?.split_whitespace();
    let first = |word: Option<&str>, sign: char| {
        word?
            .strip_prefix(sign)?
            .split(',')
            .next()?
            .parse::<u32>()
            .ok()
    };
    let old = first(words.next(), '-')?;
    let new = first(words.next(), '+')?;
    // A hunk with no lines on a side names the line before it, 0 at the top.
    Some((old.max(1), new.max(1)))
}

/// The text of a line of a diff without the mark git puts before it.
fn shown(line: &DiffLine) -> String {
    line.text.chars().skip(1).collect()
}

/// The lines selected from `from` to `to` of `lines`, in either order, as a
/// comment's anchor and the lines' text.
pub fn anchor_of(
    lines: &[DiffLine],
    path: &str,
    from: usize,
    to: usize,
) -> Result<(Anchor, Vec<String>), Refusal> {
    let (first, last) = (from.min(to), from.max(to));
    let numbers = numbered(lines);
    let commentable = |at: usize| {
        numbers
            .get(at)
            .is_some_and(|n| n.old.is_some() || n.new.is_some())
    };
    if !commentable(first) || !commentable(last) {
        return Err(Refusal::NoLine);
    }
    let range = first..=last;
    let kind = |at: usize| lines[at].kind;
    if range
        .clone()
        .any(|at| matches!(kind(at), DiffKind::File | DiffKind::Hunk))
    {
        return Err(Refusal::AcrossHunks);
    }
    let removed = range.clone().any(|at| kind(at) == DiffKind::Removed);
    let added = range.clone().any(|at| kind(at) == DiffKind::Added);
    if removed && added {
        return Err(Refusal::BothSides);
    }
    let side = if removed { Side::Old } else { Side::New };
    // A no-newline marker inside the selection has no number to count.
    let picked: Vec<(u32, String)> = range
        .filter_map(|at| {
            let n = numbers[at];
            Some((
                if side == Side::Old { n.old? } else { n.new? },
                shown(&lines[at]),
            ))
        })
        .collect();
    let anchor = Anchor {
        path: path.to_owned(),
        side,
        start: picked.first().map(|p| p.0).ok_or(Refusal::NoLine)?,
        end: picked.last().map(|p| p.0).ok_or(Refusal::NoLine)?,
    };
    Ok((anchor, picked.into_iter().map(|p| p.1).collect()))
}

/// The text of `anchor`'s lines in `lines` if every one is there.
pub fn text_at(lines: &[DiffLine], anchor: &Anchor) -> Option<Vec<String>> {
    let numbers = numbered(lines);
    (anchor.start..=anchor.end)
        .map(|wanted| {
            let at = numbers.iter().position(|n| {
                (if anchor.side == Side::Old {
                    n.old
                } else {
                    n.new
                }) == Some(wanted)
            })?;
            Some(shown(&lines[at]))
        })
        .collect()
}

/// Whether `comment`'s lines read in `lines`, the file's diff now, as they
/// did when it was made.
pub fn judge(comment: &Comment, lines: &[DiffLine]) -> Freshness {
    match text_at(lines, &comment.anchor) {
        Some(now) if now == comment.quoted => Freshness::Current,
        _ => Freshness::Outdated,
    }
}

/// The prompt that carries `comments` to an agent, those resolved left out.
pub fn prompt(comments: &[Judged]) -> String {
    // An agent's notes are for the person reviewing, never sent back, and
    // a thread of GitHub's goes only once the user has taken it in.
    let mut open: Vec<&Judged> = comments.iter().filter(|j| j.comment.is_carried()).collect();
    if open.is_empty() {
        return String::new();
    }
    open.sort_by(|a, b| {
        let key = |j: &Judged| {
            let a = &j.comment.anchor;
            (a.path.clone(), a.side, a.start, a.end, j.comment.id)
        };
        key(a).cmp(&key(b))
    });
    let mut text = String::from(
        "Review comments on your changes. Each comment names a file, the side of the change and the lines it is about, and quotes them.\n",
    );
    for (n, j) in open.iter().enumerate() {
        let Comment {
            rev,
            anchor,
            quoted,
            text: said,
            ..
        } = &j.comment;
        let side = match anchor.side {
            Side::Old => "before the change",
            Side::New => "after the change",
        };
        let lines = if anchor.start == anchor.end {
            format!("line {}", anchor.start)
        } else {
            format!("lines {}-{}", anchor.start, anchor.end)
        };
        let mut from = match rev {
            Rev::WorkingCopy => "uncommitted changes".to_owned(),
            Rev::Commit(id) => format!("commit {}", id.chars().take(7).collect::<String>()),
            Rev::Branch(id) => format!(
                "the branch since {}",
                id.chars().take(7).collect::<String>()
            ),
            Rev::Base(id) => format!(
                "the worktree against {}",
                id.chars().take(7).collect::<String>()
            ),
            Rev::Range(from, to) => format!("{to} against {from}"),
        };
        if j.freshness == Freshness::Outdated {
            from.push_str(", outdated: the code changed since this comment was made");
        }
        text.push_str(&format!(
            "\n{}. {}, {side}, {lines} ({from})\n",
            n + 1,
            anchor.path
        ));
        for line in quoted {
            text.push_str(&format!("   > {line}\n"));
        }
        for line in said.lines() {
            text.push_str(&format!("   {line}\n"));
        }
    }
    text
}

/// A file of a change marked viewed, with what its diff was when it was:
/// it stands viewed only while its diff still reads so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewed {
    pub rev: Rev,
    pub path: String,
    /// The [`digest`] of the file's lines when it was marked.
    pub digest: String,
}

/// What a file's lines in a diff come to, short enough to keep and the same
/// whichever process reads them.
pub fn digest(lines: &[DiffLine]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for line in lines {
        feed(format!("{:?}", line.kind).as_bytes());
        feed(b"\0");
        feed(line.text.as_bytes());
        feed(b"\n");
    }
    format!("{hash:016x}")
}

/// Lines of a worktree's file a session's agent wrote, as the file reads now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    /// From the top of the worktree.
    pub path: String,
    /// The lines, in order and apart from each other.
    pub runs: Vec<WrittenRun>,
}

/// Lines one after another the same session wrote, numbered from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenRun {
    pub start: u32,
    /// The last line, never before `start`.
    pub end: u32,
    /// The session: its code, then its name.
    pub by: String,
}

/// The lines of `worktree`'s files agents wrote, as JSON to keep beside a
/// change or hand to another tool: each file with its runs of lines and the
/// session that wrote each, as the files read now.
pub fn exported(worktree: &str, written: &[Written]) -> String {
    let files: Vec<_> = written
        .iter()
        .map(|w| serde_json::json!({"path": w.path, "lines": w.runs}))
        .collect();
    let json = serde_json::json!({"worktree": worktree, "files": files});
    let mut text = serde_json::to_string_pretty(&json).expect("the lines are JSON");
    text.push('\n');
    text
}

/// Who wrote line `line` of the file at `path`, numbered as it reads now.
pub fn written_by<'a>(written: &'a [Written], path: &str, line: u32) -> Option<&'a str> {
    written
        .iter()
        .find(|w| w.path == path)?
        .runs
        .iter()
        .find(|r| (r.start..=r.end).contains(&line))
        .map(|r| r.by.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_digest_changes_with_any_of_its_lines_and_only_with_them() {
        let line = |kind, text: &str| DiffLine {
            kind,
            text: text.into(),
        };
        let a = [line(DiffKind::File, "a"), line(DiffKind::Added, "+x")];
        assert_eq!(digest(&a), digest(&a.clone()));
        assert_eq!(digest(&a).len(), 16);
        let edited = [line(DiffKind::File, "a"), line(DiffKind::Added, "+y")];
        let other_kind = [line(DiffKind::File, "a"), line(DiffKind::Context, "+x")];
        assert_ne!(digest(&a), digest(&edited));
        assert_ne!(digest(&a), digest(&other_kind));
    }

    fn line(kind: DiffKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            text: text.into(),
        }
    }

    /// A diff of one file: line 2 of three is rewritten, and one is added.
    fn diff() -> Vec<DiffLine> {
        use DiffKind::*;
        vec![
            line(File, "src/lib.rs"),
            line(Hunk, "@@ -1,3 +1,4 @@"),
            line(Context, " one"),
            line(Removed, "-two"),
            line(Added, "+TWO"),
            line(Added, "+TWO AND A HALF"),
            line(Context, " three"),
        ]
    }

    fn comment(anchor: Anchor, quoted: &[&str]) -> Comment {
        Comment {
            id: 1,
            rev: Rev::WorkingCopy,
            anchor,
            quoted: quoted.iter().map(|s| s.to_string()).collect(),
            text: "why?".into(),
            resolved: false,
            sent: false,
            author: None,
            github: None,
        }
    }

    fn anchor(side: Side, start: u32, end: u32) -> Anchor {
        Anchor {
            path: "src/lib.rs".into(),
            side,
            start,
            end,
        }
    }

    #[test]
    fn hunk_headings_number_each_side_of_the_lines_under_them() {
        let n = |old, new| Numbers { old, new };
        assert_eq!(
            numbered(&diff()),
            [
                n(None, None),
                n(None, None),
                n(Some(1), Some(1)),
                n(Some(2), None),
                n(None, Some(2)),
                n(None, Some(3)),
                n(Some(3), Some(4)),
            ]
        );
    }

    #[test]
    fn a_hunk_that_names_one_line_and_a_second_hunk_restart_the_count() {
        use DiffKind::*;
        let lines = vec![
            line(Hunk, "@@ -4 +4 @@ fn f() {"),
            line(Removed, "-a"),
            line(Added, "+b"),
            line(Hunk, "@@ -10,2 +10,2 @@"),
            line(Context, " c"),
            line(Context, " d"),
        ];
        let numbers = numbered(&lines);
        assert_eq!(numbers[1].old, Some(4));
        assert_eq!(numbers[2].new, Some(4));
        assert_eq!(
            numbers[4],
            Numbers {
                old: Some(10),
                new: Some(10)
            }
        );
        assert_eq!(
            numbers[5],
            Numbers {
                old: Some(11),
                new: Some(11)
            }
        );
    }

    #[test]
    fn a_line_before_any_hunk_and_a_no_newline_marker_have_no_numbers() {
        use DiffKind::*;
        let lines = vec![
            line(Context, "Binary file"),
            line(Hunk, "@@ -1 +1 @@"),
            line(Removed, "-a"),
            line(Context, "\\ No newline at end of file"),
            line(Added, "+b"),
        ];
        let numbers = numbered(&lines);
        assert_eq!(numbers[0], Numbers::default());
        assert_eq!(numbers[3], Numbers::default());
        assert_eq!(numbers[4].new, Some(1));
    }

    #[test]
    fn a_structural_diff_has_no_numbers() {
        let lines = vec![line(DiffKind::Difft, "1 let a = 1;")];
        assert_eq!(numbered(&lines), [Numbers::default()]);
        assert_eq!(anchor_of(&lines, "a.rs", 0, 0), Err(Refusal::NoLine));
    }

    #[test]
    fn a_removed_line_is_on_the_old_side_and_an_added_one_on_the_new() {
        let (a, quoted) = anchor_of(&diff(), "src/lib.rs", 3, 3).unwrap();
        assert_eq!(a, anchor(Side::Old, 2, 2));
        assert_eq!(quoted, ["two"]);
        let (a, quoted) = anchor_of(&diff(), "src/lib.rs", 5, 4).unwrap();
        assert_eq!(a, anchor(Side::New, 2, 3));
        assert_eq!(quoted, ["TWO", "TWO AND A HALF"]);
    }

    #[test]
    fn an_unchanged_line_is_commented_on_the_new_side_and_extends_either() {
        let (a, _) = anchor_of(&diff(), "src/lib.rs", 2, 2).unwrap();
        assert_eq!(a, anchor(Side::New, 1, 1));
        let (a, quoted) = anchor_of(&diff(), "src/lib.rs", 2, 3).unwrap();
        assert_eq!(a, anchor(Side::Old, 1, 2));
        assert_eq!(quoted, ["one", "two"]);
        let (a, _) = anchor_of(&diff(), "src/lib.rs", 5, 6).unwrap();
        assert_eq!(a, anchor(Side::New, 3, 4));
    }

    #[test]
    fn lines_are_refused_that_are_headings_span_hunks_or_both_sides() {
        use DiffKind::*;
        assert_eq!(anchor_of(&diff(), "p", 0, 0), Err(Refusal::NoLine));
        assert_eq!(anchor_of(&diff(), "p", 1, 3), Err(Refusal::NoLine));
        assert_eq!(anchor_of(&diff(), "p", 3, 4), Err(Refusal::BothSides));
        assert_eq!(anchor_of(&diff(), "p", 3, 99), Err(Refusal::NoLine));
        let mut two = diff();
        two.push(line(Hunk, "@@ -20 +21 @@"));
        two.push(line(Context, " z"));
        assert_eq!(anchor_of(&two, "p", 6, 8), Err(Refusal::AcrossHunks));
    }

    #[test]
    fn the_text_of_lines_is_read_back_from_a_diff_by_side_and_number() {
        let d = diff();
        assert_eq!(
            text_at(&d, &anchor(Side::Old, 1, 2)).unwrap(),
            ["one", "two"]
        );
        assert_eq!(
            text_at(&d, &anchor(Side::New, 2, 3)).unwrap(),
            ["TWO", "TWO AND A HALF"]
        );
        assert_eq!(text_at(&d, &anchor(Side::New, 4, 5)), None);
        assert_eq!(text_at(&d, &anchor(Side::Old, 9, 9)), None);
    }

    #[test]
    fn a_comment_is_current_while_its_lines_read_as_they_did() {
        let c = comment(anchor(Side::New, 2, 3), &["TWO", "TWO AND A HALF"]);
        assert_eq!(judge(&c, &diff()), Freshness::Current);
    }

    #[test]
    fn a_comment_is_outdated_when_its_lines_changed_moved_or_went() {
        let c = comment(anchor(Side::New, 2, 2), &["TWO"]);
        let mut changed = diff();
        changed[4].text = "+Two".into();
        assert_eq!(judge(&c, &changed), Freshness::Outdated);
        // The same text one line down is another line: it is not followed.
        let mut moved = diff();
        moved.insert(2, line(DiffKind::Added, "+new first line"));
        assert_eq!(judge(&c, &moved), Freshness::Outdated);
        assert_eq!(judge(&c, &[]), Freshness::Outdated);
    }

    #[test]
    fn unicode_text_and_paths_are_quoted_whole() {
        use DiffKind::*;
        let lines = vec![
            line(Hunk, "@@ -0,0 +1 @@"),
            line(Added, "+let 名前 = \"日本語\";"),
        ];
        let (a, quoted) = anchor_of(&lines, "日本語/メモ 1.md", 1, 1).unwrap();
        assert_eq!(a.path, "日本語/メモ 1.md");
        assert_eq!(quoted, ["let 名前 = \"日本語\";"]);
        let c = Comment {
            quoted,
            anchor: a,
            ..comment(anchor(Side::New, 1, 1), &[])
        };
        assert_eq!(judge(&c, &lines), Freshness::Current);
    }

    fn judged(c: Comment, freshness: Freshness) -> Judged {
        Judged {
            comment: c,
            freshness,
        }
    }

    #[test]
    fn the_prompt_names_each_comment_s_file_side_lines_and_quotes_them() {
        let mut a = comment(anchor(Side::New, 2, 3), &["TWO", "TWO AND A HALF"]);
        a.text = "Say why.\nAnd test it.".into();
        let mut b = comment(
            Anchor {
                path: "日本語/メモ.md".into(),
                side: Side::Old,
                start: 7,
                end: 7,
            },
            &["gone"],
        );
        b.id = 2;
        b.rev = Rev::Commit("0123456789abcdef".into());
        b.text = "Why was this removed?".into();
        let text = prompt(&[judged(b, Freshness::Current), judged(a, Freshness::Current)]);
        assert_eq!(
            text,
            "\
Review comments on your changes. Each comment names a file, the side of the change and the lines it is about, and quotes them.

1. src/lib.rs, after the change, lines 2-3 (uncommitted changes)
   > TWO
   > TWO AND A HALF
   Say why.
   And test it.

2. 日本語/メモ.md, before the change, line 7 (commit 0123456)
   > gone
   Why was this removed?
"
        );
    }

    #[test]
    fn the_prompt_leaves_out_resolved_comments_and_marks_outdated_ones() {
        let mut done = comment(anchor(Side::New, 1, 1), &["x"]);
        done.resolved = true;
        let stale = comment(anchor(Side::New, 2, 2), &["TWO"]);
        let text = prompt(&[
            judged(done, Freshness::Current),
            judged(stale, Freshness::Outdated),
        ]);
        assert!(!text.contains("line 1"), "{text}");
        assert!(text.contains("line 2 (uncommitted changes, outdated: the code changed since this comment was made)"), "{text}");
        assert!(prompt(&[]).is_empty());
    }

    #[test]
    fn the_lines_written_are_kept_aside_as_json_of_each_file_s_runs_and_who_wrote_them() {
        let written = [Written {
            path: "src/メモ.rs".into(),
            runs: vec![WrittenRun {
                start: 2,
                end: 4,
                by: "s1 \"fixer\"".into(),
            }],
        }];
        let text = exported("/w/tree", &written);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "worktree": "/w/tree",
                "files": [{"path": "src/メモ.rs", "lines": [
                    {"start": 2, "end": 4, "by": "s1 \"fixer\""},
                ]}],
            })
        );
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn a_line_is_told_by_the_run_it_falls_in_of_its_own_file() {
        let run = |start, end, by: &str| WrittenRun {
            start,
            end,
            by: by.into(),
        };
        let written = [Written {
            path: "a".into(),
            runs: vec![run(2, 3, "s1 one"), run(5, 5, "s2 two")],
        }];
        assert_eq!(written_by(&written, "a", 1), None);
        assert_eq!(written_by(&written, "a", 2), Some("s1 one"));
        assert_eq!(written_by(&written, "a", 3), Some("s1 one"));
        assert_eq!(written_by(&written, "a", 4), None);
        assert_eq!(written_by(&written, "a", 5), Some("s2 two"));
        assert_eq!(written_by(&written, "b", 2), None);
    }
}
