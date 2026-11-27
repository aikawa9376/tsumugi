//! A diff window's review of one file's line diff: the line the cursor is
//! on, the lines marked with it, the comment being written and the prompt
//! being previewed. The comments themselves are kept by the work host; the
//! window asks it, and this holds what the keys have decided.

use crate::diff_view::Way;
use tsumugi_model::github::LineThread;
use tsumugi_model::repository::{DiffKind, DiffLine, Rev};
use tsumugi_model::review::{self, Anchor, Change, Comment, Freshness, Judged, NewComment, Side};

/// What a pane is reading: the diff of a file, or of every file of a change
/// one after another under their headings, and the comments of the worktree.
#[derive(Clone)]
pub struct Reading<'a> {
    pub lines: &'a [DiffLine],
    /// The file of the lines above the first heading.
    pub path: &'a str,
    pub rev: &'a Rev,
    pub comments: &'a [Judged],
    numbers: Vec<review::Numbers>,
    /// Where each file's heading is.
    heads: Vec<usize>,
}

impl<'a> Reading<'a> {
    pub fn new(
        lines: &'a [DiffLine],
        path: &'a str,
        rev: &'a Rev,
        comments: &'a [Judged],
    ) -> Reading<'a> {
        let heads = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == DiffKind::File)
            .map(|(i, _)| i)
            .collect();
        Reading {
            lines,
            path,
            rev,
            comments,
            numbers: review::numbered(lines),
            heads,
        }
    }

    /// Who wrote each line, of what agents are known to have written of the
    /// worktree's files as they read now: lines of the new side only, and
    /// none where the new side is not the worktree.
    pub fn written_by<'w>(&self, written: &'w [review::Written]) -> Vec<Option<&'w str>> {
        if !matches!(self.rev, Rev::WorkingCopy | Rev::Base(_)) || written.is_empty() {
            return vec![None; self.lines.len()];
        }
        (0..self.lines.len())
            .map(|i| {
                let kind = self.lines[i].kind;
                if !matches!(kind, DiffKind::Added | DiffKind::Context) {
                    return None;
                }
                review::written_by(written, self.path_at(i), self.numbers[i].new?)
            })
            .collect()
    }

    /// What agents are known to have written of the files this diff shows,
    /// as they read now; none where the new side is not the worktree.
    pub fn written(&self, written: &[review::Written]) -> Vec<review::Written> {
        if !matches!(self.rev, Rev::WorkingCopy | Rev::Base(_)) {
            return Vec::new();
        }
        written
            .iter()
            .filter(|w| self.shows(&w.path))
            .cloned()
            .collect()
    }

    /// Where each file of the diff begins: its heading, or the first line
    /// when there is none.
    pub fn file_starts(&self) -> Vec<usize> {
        if self.heads.first() == Some(&0) || self.lines.is_empty() {
            self.heads.clone()
        } else {
            std::iter::once(0)
                .chain(self.heads.iter().copied())
                .collect()
        }
    }

    /// The file the line `line` is of.
    pub fn path_at(&self, line: usize) -> &'a str {
        let before = self.heads.partition_point(|&h| h <= line);
        match before.checked_sub(1) {
            Some(k) => self.lines[self.heads[k]].text.as_str(),
            None => self.path,
        }
    }

    /// Whether the file `path` is one this diff shows.
    fn shows(&self, path: &str) -> bool {
        if self.heads.is_empty() {
            return path == self.path;
        }
        self.heads.iter().any(|&h| self.lines[h].text == path)
            || (self.heads.first() != Some(&0) && path == self.path)
    }

    fn here(&self) -> impl Iterator<Item = &'a Judged> + use<'a, '_> {
        self.comments
            .iter()
            .filter(move |j| j.comment.rev == *self.rev && self.shows(&j.comment.anchor.path))
    }

    /// The comments of these files whose lines no longer read as they did,
    /// listed after the diff, where the cursor can reach them.
    pub fn outdated(&self) -> Vec<&'a Judged> {
        self.here()
            .filter(|j| j.freshness == Freshness::Outdated)
            .collect()
    }

    /// How many places the cursor can be at: each line, then each outdated
    /// comment.
    pub fn places(&self) -> usize {
        self.lines.len() + self.outdated().len()
    }

    /// The number the line `line` has on the side `side`, in its file.
    fn number(&self, line: usize, side: Side) -> Option<u32> {
        let n = self.numbers[line];
        match side {
            Side::Old => n.old,
            Side::New => n.new,
        }
    }

    fn covers(&self, comment: &Comment, line: usize) -> bool {
        comment.anchor.path == self.path_at(line)
            && self
                .number(line, comment.anchor.side)
                .is_some_and(|n| (comment.anchor.start..=comment.anchor.end).contains(&n))
    }

    /// The comments that stand on the line `line`, those ending there
    /// first, which a thread is drawn under.
    pub fn standing_on(&self, line: usize) -> Vec<&'a Judged> {
        self.here()
            .filter(|j| j.freshness == Freshness::Current && self.covers(&j.comment, line))
            .collect()
    }

    /// The comments whose thread is drawn under the line `line`: those whose
    /// last line it is.
    pub fn ending_at(&self, line: usize) -> Vec<&'a Judged> {
        let path = self.path_at(line);
        self.here()
            .filter(|j| {
                let a = &j.comment.anchor;
                j.freshness == Freshness::Current
                    && a.path == path
                    && self.number(line, a.side) == Some(a.end)
            })
            .collect()
    }

    /// The lines an agent's note is drawn under, in order.
    pub fn note_ends(&self) -> Vec<usize> {
        let mut ends: Vec<usize> = (0..self.lines.len())
            .filter(|&i| self.ending_at(i).iter().any(|j| j.comment.is_note()))
            .collect();
        ends.dedup();
        ends
    }

    /// The comment under the cursor place `at`: a line one stands on, or an
    /// outdated one.
    pub fn comment_at(&self, at: usize) -> Option<&'a Judged> {
        if at < self.lines.len() {
            self.standing_on(at).into_iter().next()
        } else {
            self.outdated().get(at - self.lines.len()).copied()
        }
    }
}

/// The review threads of the branch's pull request, as comments of this
/// diff: drawn only in a diff against the base, as the pull request's is,
/// and standing only where the lines GitHub quoted still read so. Their ids
/// count down from the top, where no kept comment's reach.
pub fn github_threads(
    lines: &[DiffLine],
    path: &str,
    rev: &Rev,
    threads: &[LineThread],
) -> Vec<Judged> {
    if !matches!(rev, Rev::Branch(_) | Rev::Base(_)) {
        return Vec::new();
    }
    let heads: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == DiffKind::File)
        .map(|(i, _)| i)
        .collect();
    // The lines of one file: under its heading, or all of a file's own diff.
    let of_file = |file: &str| -> Option<&[DiffLine]> {
        if heads.is_empty() {
            return (file == path).then_some(lines);
        }
        let at = heads.iter().position(|&h| lines[h].text == file)?;
        let end = heads.get(at + 1).copied().unwrap_or(lines.len());
        Some(&lines[heads[at] + 1..end])
    };
    threads
        .iter()
        .enumerate()
        .map(|(n, t)| {
            let stands = !t.outdated
                && of_file(&t.anchor.path)
                    .and_then(|file| review::text_at(file, &t.anchor))
                    .is_some_and(|now| now == t.quoted);
            let said: Vec<String> = t
                .remarks
                .iter()
                .map(|r| format!("@{}: {}", r.author, r.body.trim()))
                .collect();
            Judged {
                comment: Comment {
                    id: u64::MAX - n as u64,
                    rev: rev.clone(),
                    anchor: t.anchor.clone(),
                    quoted: t.quoted.clone(),
                    text: said.join("\n\n"),
                    resolved: t.resolved,
                    sent: false,
                    author: t.remarks.first().map(|r| r.author.clone()),
                    github: Some(t.url.clone()),
                },
                freshness: if stands {
                    Freshness::Current
                } else {
                    Freshness::Outdated
                },
            }
        })
        .collect()
}

/// What a key that changes a comment says of a thread of GitHub's.
const GITHUB_THREAD: &str = "This thread is GitHub's: T takes it into your review";

/// What the comment being written is for.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Add { anchor: Anchor, quoted: Vec<String> },
    Edit { id: u64 },
}

/// The prompt being previewed, with what it carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    pub text: String,
    /// The comments it carries, and what each said when it was read.
    pub carries: Vec<(u64, String)>,
    /// The destination chosen among those the window lists.
    pub choice: usize,
    /// Sending, which waits on the work host.
    pub busy: bool,
    /// Why the last send did not go.
    pub error: Option<String>,
}

impl Preview {
    /// The comments of `now` the prompt carried as they were read, which
    /// are those to mark sent: one changed while the prompt was on its way
    /// is not what was sent.
    pub fn sent_ids(&self, now: &[Judged]) -> Vec<u64> {
        self.carries
            .iter()
            .filter(|(id, text)| {
                now.iter()
                    .any(|j| j.comment.id == *id && j.comment.text == *text)
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Mode {
    #[default]
    Browsing,
    Writing {
        target: Target,
        text: String,
    },
    Previewing(Preview),
}

/// One diff window's review.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pane {
    cursor: usize,
    mark: Option<usize>,
    pub mode: Mode,
    /// What the last key could not do, until the next one.
    pub notice: Option<String>,
}

impl Pane {
    /// Where the cursor is, kept within what `reading` has.
    pub fn cursor(&self, reading: &Reading) -> usize {
        self.cursor.min(reading.places().saturating_sub(1))
    }

    /// The lines the cursor and the mark span, the first and the last.
    pub fn span(&self, reading: &Reading) -> (usize, usize) {
        let cursor = self.cursor(reading);
        let mark = self
            .mark
            .map_or(cursor, |m| m.min(reading.lines.len().saturating_sub(1)));
        (cursor.min(mark), cursor.max(mark))
    }

    pub fn marking(&self) -> bool {
        self.mark.is_some()
    }

    /// Move the cursor `by` places, or to the first or last of them, or to
    /// the heading of the next or previous hunk.
    pub fn go(&mut self, reading: &Reading, to: Go) {
        self.notice = None;
        let last = reading.places().saturating_sub(1);
        let here = self.cursor(reading);
        self.cursor = match to {
            Go::By(n) => here.saturating_add_signed(n).min(last),
            Go::Top => 0,
            Go::Bottom => last,
            Go::Note(way) => {
                let ends = reading.note_ends();
                match crate::diff_view::hunk_from(&ends, Some(here), way) {
                    Some(at) => at,
                    None => {
                        self.notice = Some(
                            match way {
                                Way::Next => "No later note",
                                Way::Prev => "No earlier note",
                            }
                            .into(),
                        );
                        here
                    }
                }
            }
            Go::File(way) => {
                let starts = reading.file_starts();
                match crate::diff_view::hunk_from(&starts, Some(here), way) {
                    Some(at) => at,
                    None => {
                        self.notice = Some(
                            match way {
                                Way::Next => "No later file",
                                Way::Prev => "No earlier file",
                            }
                            .into(),
                        );
                        here
                    }
                }
            }
            Go::Hunk(way) => {
                let starts = crate::diff_view::hunk_starts(reading.lines);
                match crate::diff_view::hunk_from(&starts, Some(here), way) {
                    Some(at) => at,
                    None => {
                        self.notice = Some(
                            match way {
                                Way::Next => "No later change",
                                Way::Prev => "No earlier change",
                            }
                            .into(),
                        );
                        here
                    }
                }
            }
        };
    }

    /// Put the cursor where a click was.
    pub fn place(&mut self, at: usize) {
        self.notice = None;
        self.cursor = at;
    }

    /// Start marking lines from the cursor, or stop.
    pub fn select(&mut self, reading: &Reading) {
        self.notice = None;
        self.mark = match self.mark {
            Some(_) => None,
            None => Some(self.cursor(reading)),
        };
    }

    /// Write a comment on the line the cursor is on, or the lines marked.
    pub fn comment(&mut self, reading: &Reading) {
        self.notice = None;
        let (first, last) = self.span(reading);
        if self.cursor(reading) >= reading.lines.len() {
            self.notice = Some("Comment on lines of the diff".into());
            return;
        }
        match review::anchor_of(reading.lines, reading.path_at(first), first, last) {
            Ok((anchor, quoted)) => {
                self.mark = None;
                self.mode = Mode::Writing {
                    target: Target::Add { anchor, quoted },
                    text: String::new(),
                };
            }
            Err(refusal) => self.notice = Some(refusal.words().into()),
        }
    }

    /// Change the comment under the cursor.
    pub fn edit(&mut self, reading: &Reading) {
        self.notice = None;
        let Some(found) = reading.comment_at(self.cursor(reading)) else {
            self.notice = Some("No comment here".into());
            return;
        };
        if found.comment.is_github() {
            self.notice = Some(GITHUB_THREAD.into());
            return;
        }
        self.mode = Mode::Writing {
            target: Target::Edit {
                id: found.comment.id,
            },
            text: found.comment.text.clone(),
        };
    }

    /// The text written so far is `text`.
    pub fn typed(&mut self, text: String) {
        if let Mode::Writing { text: now, .. } = &mut self.mode {
            *now = text;
        }
    }

    /// Keep the comment written, if it says something; `rev` is where the
    /// diff was read from.
    pub fn finish(&mut self, rev: &Rev) -> Option<Change> {
        let Mode::Writing { target, text } = &self.mode else {
            return None;
        };
        if text.trim().is_empty() {
            self.notice = Some("Write what the comment says, or cancel".into());
            return None;
        }
        let change = match target {
            Target::Add { anchor, quoted } => Change::Add(NewComment {
                rev: rev.clone(),
                anchor: anchor.clone(),
                quoted: quoted.clone(),
                text: text.clone(),
                author: None,
            }),
            Target::Edit { id } => Change::Edit {
                id: *id,
                text: text.clone(),
            },
        };
        self.mode = Mode::Browsing;
        self.notice = None;
        Some(change)
    }

    /// Give up what is being written or previewed.
    pub fn cancel(&mut self) {
        self.mode = Mode::Browsing;
        self.notice = None;
    }

    /// Delete the comment under the cursor.
    pub fn delete(&mut self, reading: &Reading) -> Option<Change> {
        self.notice = None;
        match reading.comment_at(self.cursor(reading)) {
            Some(found) if found.comment.is_github() => {
                self.notice = Some(GITHUB_THREAD.into());
                None
            }
            Some(found) => Some(Change::Remove {
                id: found.comment.id,
            }),
            None => {
                self.notice = Some("No comment here".into());
                None
            }
        }
    }

    /// Resolve the comment under the cursor, or open it again.
    pub fn resolve(&mut self, reading: &Reading) -> Option<Change> {
        self.notice = None;
        match reading.comment_at(self.cursor(reading)) {
            Some(found) if found.comment.is_github() => {
                self.notice = Some(GITHUB_THREAD.into());
                None
            }
            Some(found) => Some(Change::Resolve {
                id: found.comment.id,
                resolved: !found.comment.resolved,
            }),
            None => {
                self.notice = Some("No comment here".into());
                None
            }
        }
    }

    /// Keep the thread of GitHub's under the cursor as a comment of the
    /// user's on its lines, saying who said what and where it is.
    pub fn take(&mut self, reading: &Reading) -> Option<Change> {
        self.notice = None;
        let Some(found) = reading.comment_at(self.cursor(reading)) else {
            self.notice = Some("No thread here".into());
            return None;
        };
        let Some(url) = &found.comment.github else {
            self.notice = Some("Only a thread from GitHub is taken in".into());
            return None;
        };
        let c = &found.comment;
        Some(Change::Add(NewComment {
            rev: c.rev.clone(),
            anchor: c.anchor.clone(),
            quoted: c.quoted.clone(),
            text: format!("From the review thread on GitHub at {url}:\n\n{}", c.text),
            author: None,
        }))
    }

    /// Show the prompt that carries every comment not resolved, of every
    /// file, before it goes anywhere.
    pub fn preview(&mut self, comments: &[Judged]) {
        self.notice = None;
        let text = review::prompt(comments);
        if text.is_empty() {
            self.notice = Some("No comments to send".into());
            return;
        }
        let carries = comments
            .iter()
            .filter(|j| j.comment.is_carried())
            .map(|j| (j.comment.id, j.comment.text.clone()))
            .collect();
        self.mode = Mode::Previewing(Preview {
            text,
            carries,
            choice: 0,
            busy: false,
            error: None,
        });
    }

    /// Choose another destination of the `of` the preview lists.
    pub fn choose(&mut self, by: isize, of: usize) {
        if let Mode::Previewing(preview) = &mut self.mode {
            preview.choice = preview
                .choice
                .min(of.saturating_sub(1))
                .saturating_add_signed(by)
                .min(of.saturating_sub(1));
        }
    }

    /// The preview is sending, or stopped doing so for `error`.
    pub fn sending(&mut self, busy: bool, error: Option<String>) {
        if let Mode::Previewing(preview) = &mut self.mode {
            preview.busy = busy;
            preview.error = error;
        }
    }
}

/// Where the cursor goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Go {
    By(isize),
    Top,
    Bottom,
    Hunk(Way),
    /// To the heading of the next or previous file.
    File(Way),
    /// To the next or previous line an agent's note is under.
    Note(Way),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: DiffKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            text: text.into(),
        }
    }

    fn diff() -> Vec<DiffLine> {
        use DiffKind::*;
        vec![
            line(File, "src/メモ.rs"),
            line(Hunk, "@@ -1,2 +1,2 @@"),
            line(Context, " one"),
            line(Removed, "-two"),
            line(Added, "+TWO"),
        ]
    }

    fn stored(id: u64, side: Side, at: u32, quoted: &str, freshness: Freshness) -> Judged {
        Judged {
            comment: Comment {
                id,
                rev: Rev::WorkingCopy,
                anchor: Anchor {
                    path: "src/メモ.rs".into(),
                    side,
                    start: at,
                    end: at,
                },
                quoted: vec![quoted.into()],
                text: format!("comment {id}"),
                resolved: false,
                sent: false,
                author: None,
                github: None,
            },
            freshness,
        }
    }

    fn reading<'a>(lines: &'a [DiffLine], comments: &'a [Judged]) -> Reading<'a> {
        Reading::new(lines, "src/メモ.rs", &Rev::WorkingCopy, comments)
    }

    fn stream() -> Vec<DiffLine> {
        use DiffKind::*;
        vec![
            line(File, "a.rs"),
            line(Hunk, "@@ -1 +1 @@"),
            line(Added, "+one"),
            line(File, "b.rs"),
            line(Hunk, "@@ -1 +1 @@"),
            line(Added, "+one"),
        ]
    }

    #[test]
    fn lines_of_the_new_side_are_told_by_who_wrote_them_only_where_it_is_the_worktree() {
        use DiffKind::*;
        use tsumugi_model::review::{Written, WrittenRun};
        let lines = vec![
            line(File, "a.rs"),
            line(Hunk, "@@ -1,2 +1,2 @@"),
            line(Context, " one"),
            line(Removed, "-two"),
            line(Added, "+TWO"),
            line(File, "b.rs"),
            line(Hunk, "@@ -0,0 +1 @@"),
            line(Added, "+new"),
        ];
        let run = |path: &str, start, end| Written {
            path: path.into(),
            runs: vec![WrittenRun {
                start,
                end,
                by: "s1 fixer".into(),
            }],
        };
        let written = [run("a.rs", 1, 2), run("b.rs", 1, 1)];
        let w = Some("s1 fixer");
        for rev in [Rev::WorkingCopy, Rev::Base("abc".into())] {
            let r = Reading::new(&lines, "a.rs", &rev, &[]);
            assert_eq!(
                r.written_by(&written),
                [None, None, w, None, w, None, None, w],
                "{rev:?}"
            );
        }
        let commit = Rev::Commit("abc".into());
        let r = Reading::new(&lines, "a.rs", &commit, &[]);
        assert!(r.written_by(&written).iter().all(Option::is_none));
    }

    #[test]
    fn the_lines_written_that_are_kept_aside_are_those_of_the_files_the_diff_shows() {
        use tsumugi_model::review::{Written, WrittenRun};
        let lines = stream();
        let of = |path: &str| Written {
            path: path.into(),
            runs: vec![WrittenRun {
                start: 1,
                end: 1,
                by: "s1 fixer".into(),
            }],
        };
        let written = [of("a.rs"), of("elsewhere.rs"), of("b.rs")];
        let r = Reading::new(&lines, "a.rs", &Rev::WorkingCopy, &[]);
        assert_eq!(r.written(&written), [of("a.rs"), of("b.rs")]);
        let commit = Rev::Commit("abc".into());
        let r = Reading::new(&lines, "a.rs", &commit, &[]);
        assert!(r.written(&written).is_empty());
    }

    #[test]
    fn the_cursor_steps_between_the_lines_agents_notes_are_under() {
        let lines = stream();
        let mut note = stored(1, Side::New, 1, "one", Freshness::Current);
        note.comment.anchor.path = "b.rs".into();
        note.comment.author = Some("abcd fixer".into());
        let mut mine = stored(2, Side::New, 1, "one", Freshness::Current);
        mine.comment.anchor.path = "a.rs".into();
        let comments = [note, mine];
        let r = reading(&lines, &comments);
        assert_eq!(r.note_ends(), [5], "a person's comment is no note");
        let mut pane = Pane::default();
        pane.go(&r, Go::Note(Way::Next));
        assert_eq!(pane.cursor(&r), 5);
        pane.go(&r, Go::Note(Way::Next));
        assert!(pane.notice.is_some());
    }

    #[test]
    fn notes_are_never_carried_to_an_agent() {
        let mut note = stored(1, Side::New, 2, "TWO", Freshness::Current);
        note.comment.author = Some("abcd fixer".into());
        let mut pane = Pane::default();
        pane.preview(&[note.clone()]);
        assert_eq!(pane.mode, Mode::Browsing, "nothing to send");
        let mine = stored(2, Side::New, 2, "TWO", Freshness::Current);
        pane.preview(&[note, mine]);
        let Mode::Previewing(preview) = &pane.mode else {
            panic!("no preview");
        };
        assert_eq!(preview.carries.len(), 1);
        assert!(!preview.text.contains("comment 1"));
    }

    #[test]
    fn each_line_of_a_stream_is_of_the_file_whose_heading_is_above_it() {
        let lines = stream();
        let r = reading(&lines, &[]);
        assert_eq!(r.path_at(0), "a.rs");
        assert_eq!(r.path_at(2), "a.rs");
        assert_eq!(r.path_at(3), "b.rs");
        assert_eq!(r.path_at(5), "b.rs");
        assert_eq!(r.file_starts(), [0, 3]);
    }

    #[test]
    fn a_comment_in_a_stream_stands_only_on_its_own_file_and_is_written_for_it() {
        let lines = stream();
        let mut on_b = stored(1, Side::New, 1, "one", Freshness::Current);
        on_b.comment.anchor.path = "b.rs".into();
        let mut gone = stored(2, Side::New, 7, "x", Freshness::Outdated);
        gone.comment.anchor.path = "a.rs".into();
        let comments = [on_b, gone];
        let r = reading(&lines, &comments);
        assert!(r.standing_on(2).is_empty());
        assert_eq!(r.standing_on(5).len(), 1);
        assert_eq!(r.ending_at(5).len(), 1);
        assert_eq!(
            r.outdated().len(),
            1,
            "outdated ones of every file are listed"
        );
        let mut pane = Pane::default();
        pane.place(5);
        pane.comment(&r);
        let Mode::Writing {
            target: Target::Add { anchor, .. },
            ..
        } = &pane.mode
        else {
            panic!("not writing: {:?}", pane.mode);
        };
        assert_eq!(anchor.path, "b.rs");
    }

    #[test]
    fn the_cursor_moves_to_the_next_and_previous_file_of_a_stream() {
        let lines = stream();
        let r = reading(&lines, &[]);
        let mut pane = Pane::default();
        pane.place(1);
        pane.go(&r, Go::File(Way::Next));
        assert_eq!(pane.cursor(&r), 3);
        pane.go(&r, Go::File(Way::Next));
        assert_eq!(pane.cursor(&r), 3);
        assert!(pane.notice.is_some());
        pane.go(&r, Go::File(Way::Prev));
        assert_eq!(pane.cursor(&r), 0);
        pane.go(&r, Go::Hunk(Way::Next));
        pane.go(&r, Go::Hunk(Way::Next));
        assert_eq!(pane.cursor(&r), 4, "a hunk move crosses into the next file");
    }

    #[test]
    fn the_cursor_jumps_to_the_next_and_previous_hunk_and_stays_at_the_ends_saying_so() {
        use DiffKind::*;
        let lines = vec![
            line(File, "a.rs"),
            line(Hunk, "@@ -1 +1 @@"),
            line(Added, "+one"),
            line(Hunk, "@@ -9 +9 @@"),
            line(Added, "+two"),
        ];
        let r = reading(&lines, &[]);
        let mut pane = Pane::default();
        pane.go(&r, Go::Hunk(Way::Next));
        assert_eq!(pane.cursor(&r), 1);
        pane.go(&r, Go::Hunk(Way::Next));
        assert_eq!(pane.cursor(&r), 3);
        assert_eq!(pane.notice, None);
        pane.go(&r, Go::Hunk(Way::Next));
        assert_eq!(pane.cursor(&r), 3);
        assert!(pane.notice.is_some());
        pane.go(&r, Go::Hunk(Way::Prev));
        assert_eq!(pane.cursor(&r), 1);
        pane.go(&r, Go::Hunk(Way::Prev));
        assert_eq!(pane.cursor(&r), 1);
        assert!(pane.notice.is_some());
    }

    #[test]
    fn a_diff_with_no_hunk_has_no_change_to_jump_to() {
        let lines = vec![
            line(DiffKind::File, "a.png"),
            line(DiffKind::Context, "Binary files differ"),
        ];
        let r = reading(&lines, &[]);
        let mut pane = Pane::default();
        pane.go(&r, Go::Hunk(Way::Next));
        assert_eq!(pane.cursor(&r), 0);
        assert!(pane.notice.is_some());
    }

    #[test]
    fn the_cursor_moves_through_the_lines_and_then_the_outdated_comments_and_stops_at_the_ends() {
        let lines = diff();
        let comments = [stored(1, Side::New, 9, "gone", Freshness::Outdated)];
        let r = reading(&lines, &comments);
        let mut pane = Pane::default();
        pane.go(&r, Go::By(-1));
        assert_eq!(pane.cursor(&r), 0);
        pane.go(&r, Go::Bottom);
        assert_eq!(pane.cursor(&r), 5);
        pane.go(&r, Go::By(3));
        assert_eq!(pane.cursor(&r), 5);
        assert_eq!(r.comment_at(5).unwrap().comment.id, 1);
        pane.go(&r, Go::Top);
        assert_eq!(pane.cursor(&r), 0);
        pane.place(99);
        assert_eq!(pane.cursor(&r), 5);
        pane.place(2);
        assert_eq!(pane.cursor(&r), 2);
    }

    #[test]
    fn a_comment_on_the_line_under_the_cursor_pins_it_to_its_side_and_number() {
        let lines = diff();
        let r = reading(&lines, &[]);
        let mut pane = Pane::default();
        pane.go(&r, Go::By(3));
        pane.comment(&r);
        let Mode::Writing { target, .. } = &pane.mode else {
            panic!("{pane:?}")
        };
        assert_eq!(
            *target,
            Target::Add {
                anchor: Anchor {
                    path: "src/メモ.rs".into(),
                    side: Side::Old,
                    start: 2,
                    end: 2
                },
                quoted: vec!["two".into()]
            }
        );
    }

    #[test]
    fn marked_lines_are_commented_together_and_headings_and_mixed_sides_are_refused_with_a_word() {
        let lines = diff();
        let r = reading(&lines, &[]);
        let mut pane = Pane::default();
        pane.comment(&r);
        assert_eq!(pane.mode, Mode::Browsing);
        assert!(pane.notice.is_some());
        pane.go(&r, Go::By(3));
        pane.select(&r);
        pane.go(&r, Go::By(1));
        assert_eq!(pane.span(&r), (3, 4));
        pane.comment(&r);
        assert_eq!(pane.mode, Mode::Browsing);
        assert!(pane.notice.as_deref().unwrap().contains("both"));
        pane.select(&r);
        assert!(!pane.marking());
        pane.go(&r, Go::By(-2));
        pane.select(&r);
        pane.go(&r, Go::By(1));
        pane.comment(&r);
        let Mode::Writing {
            target: Target::Add { anchor, quoted },
            ..
        } = &pane.mode
        else {
            panic!("{pane:?}")
        };
        assert_eq!((anchor.side, anchor.start, anchor.end), (Side::Old, 1, 2));
        assert_eq!(quoted, &["one", "two"]);
        assert!(!pane.marking());
    }

    #[test]
    fn writing_ends_in_a_change_only_when_it_says_something_and_cancelling_keeps_nothing() {
        let lines = diff();
        let r = reading(&lines, &[]);
        let mut pane = Pane::default();
        pane.go(&r, Go::By(4));
        pane.comment(&r);
        pane.typed("  ".into());
        assert_eq!(pane.finish(&Rev::WorkingCopy), None);
        assert!(matches!(pane.mode, Mode::Writing { .. }));
        assert!(pane.notice.is_some());
        pane.typed("名前を変える\nなぜ?".into());
        let Some(Change::Add(new)) = pane.finish(&Rev::Commit("abc".into())) else {
            panic!("no change")
        };
        assert_eq!(new.text, "名前を変える\nなぜ?");
        assert_eq!(new.rev, Rev::Commit("abc".into()));
        assert_eq!(new.anchor.side, Side::New);
        assert_eq!(pane.mode, Mode::Browsing);
        pane.comment(&r);
        pane.typed("again".into());
        pane.cancel();
        assert_eq!(pane.mode, Mode::Browsing);
        assert_eq!(pane.finish(&Rev::WorkingCopy), None);
    }

    #[test]
    fn a_comment_is_edited_deleted_and_resolved_where_the_cursor_is_on_it() {
        let lines = diff();
        let comments = [
            stored(4, Side::New, 2, "TWO", Freshness::Current),
            stored(5, Side::Old, 7, "gone", Freshness::Outdated),
        ];
        let r = reading(&lines, &comments);
        let mut pane = Pane::default();
        assert_eq!(pane.delete(&r), None);
        assert!(pane.notice.is_some());
        pane.go(&r, Go::By(4));
        pane.edit(&r);
        assert_eq!(
            pane.mode,
            Mode::Writing {
                target: Target::Edit { id: 4 },
                text: "comment 4".into()
            }
        );
        pane.typed("better".into());
        assert_eq!(
            pane.finish(&Rev::WorkingCopy),
            Some(Change::Edit {
                id: 4,
                text: "better".into()
            })
        );
        assert_eq!(
            pane.resolve(&r),
            Some(Change::Resolve {
                id: 4,
                resolved: true
            })
        );
        assert_eq!(pane.delete(&r), Some(Change::Remove { id: 4 }));
        pane.go(&r, Go::Bottom);
        assert_eq!(pane.delete(&r), Some(Change::Remove { id: 5 }));
        pane.comment(&r);
        assert_eq!(pane.mode, Mode::Browsing, "no commenting on a comment");
    }

    #[test]
    fn comments_of_other_files_and_revisions_are_not_under_this_diff() {
        let lines = diff();
        let mut elsewhere = stored(1, Side::New, 2, "TWO", Freshness::Current);
        elsewhere.comment.anchor.path = "other.rs".into();
        let mut older = stored(2, Side::New, 2, "TWO", Freshness::Current);
        older.comment.rev = Rev::Commit("abc".into());
        let comments = [elsewhere, older];
        let r = reading(&lines, &comments);
        assert!(r.standing_on(4).is_empty());
        assert_eq!(r.places(), lines.len());
    }

    #[test]
    fn a_thread_is_drawn_under_the_last_line_of_its_range() {
        let lines = diff();
        let mut range = stored(1, Side::New, 1, "one", Freshness::Current);
        range.comment.anchor.end = 2;
        range.comment.quoted.push("TWO".into());
        let comments = [range];
        let r = reading(&lines, &comments);
        assert_eq!(r.standing_on(2).len(), 1);
        assert_eq!(r.standing_on(4).len(), 1);
        assert!(r.ending_at(2).is_empty());
        assert_eq!(r.ending_at(4).len(), 1);
    }

    #[test]
    fn the_preview_carries_every_comment_not_resolved_and_needs_one() {
        let mut done = stored(1, Side::New, 2, "TWO", Freshness::Current);
        done.comment.resolved = true;
        let open = stored(2, Side::Old, 2, "two", Freshness::Current);
        let mut pane = Pane::default();
        pane.preview(&[done.clone()]);
        assert_eq!(pane.mode, Mode::Browsing);
        assert_eq!(pane.notice.as_deref(), Some("No comments to send"));
        pane.preview(&[done, open.clone()]);
        let Mode::Previewing(p) = &pane.mode else {
            panic!("{pane:?}")
        };
        assert_eq!(p.carries, [(2, "comment 2".to_string())]);
        assert!(p.text.contains("comment 2") && !p.text.contains("comment 1"));
    }

    #[test]
    fn what_was_edited_while_sending_is_not_marked_sent() {
        let (a, b) = (
            stored(1, Side::New, 2, "TWO", Freshness::Current),
            stored(2, Side::Old, 2, "two", Freshness::Current),
        );
        let mut pane = Pane::default();
        pane.preview(&[a.clone(), b.clone()]);
        pane.sending(true, None);
        let mut edited = b;
        edited.comment.text = "changed meanwhile".into();
        let Mode::Previewing(p) = &pane.mode else {
            panic!()
        };
        assert!(p.busy);
        assert_eq!(p.sent_ids(&[a, edited]), [1]);
        // A comment deleted meanwhile is not marked either.
        assert!(p.sent_ids(&[]).is_empty());
    }

    #[test]
    fn the_destination_is_chosen_within_those_listed() {
        let mut pane = Pane::default();
        pane.preview(&[stored(1, Side::New, 2, "TWO", Freshness::Current)]);
        pane.choose(5, 3);
        let Mode::Previewing(p) = &pane.mode else {
            panic!()
        };
        assert_eq!(p.choice, 2);
        pane.choose(-1, 3);
        pane.choose(-9, 3);
        let Mode::Previewing(p) = &pane.mode else {
            panic!()
        };
        assert_eq!(p.choice, 0);
        pane.choose(1, 0);
    }

    fn github(side: Side, at: u32, quoted: &str, outdated: bool) -> LineThread {
        use tsumugi_model::github::LineRemark;
        LineThread {
            url: "https://github.com/o/r/pull/1#discussion_r9".into(),
            anchor: Anchor {
                path: "src/メモ.rs".into(),
                side,
                start: at,
                end: at,
            },
            quoted: vec![quoted.into()],
            resolved: false,
            outdated,
            remarks: vec![
                LineRemark {
                    author: "alice".into(),
                    body: "Why upper case?".into(),
                },
                LineRemark {
                    author: "bob".into(),
                    body: "Agreed".into(),
                },
            ],
        }
    }

    #[test]
    fn a_github_thread_is_drawn_only_in_a_diff_against_the_base_and_stands_only_where_its_lines_read_so()
     {
        let lines = diff();
        let threads = [
            github(Side::New, 2, "TWO", false),
            github(Side::New, 2, "two", false),
            github(Side::Old, 2, "two", true),
        ];
        for rev in [
            Rev::WorkingCopy,
            Rev::Commit("c".into()),
            Rev::Range("a".into(), "b".into()),
        ] {
            assert!(
                github_threads(&lines, "", &rev, &threads).is_empty(),
                "{rev:?}"
            );
        }
        let base = Rev::Base("abc".into());
        let judged = github_threads(&lines, "", &base, &threads);
        assert_eq!(
            judged.iter().map(|j| j.freshness).collect::<Vec<_>>(),
            [Freshness::Current, Freshness::Outdated, Freshness::Outdated],
            "GitHub calling it outdated makes it so"
        );
        let r = Reading::new(&lines, "", &base, &judged);
        assert_eq!(r.ending_at(4).len(), 1);
        assert_eq!(r.outdated().len(), 2);
        assert!(
            judged
                .iter()
                .all(|j| j.comment.is_github() && !j.comment.is_note())
        );
        assert_eq!(
            judged[0].comment.text,
            "@alice: Why upper case?\n\n@bob: Agreed"
        );
        let branch = Rev::Branch("abc".into());
        assert_eq!(github_threads(&lines, "", &branch, &threads).len(), 3);
    }

    #[test]
    fn a_github_thread_is_never_carried_by_the_prompt_nor_edited_deleted_or_resolved_from_the_diff()
    {
        let lines = diff();
        let base = Rev::Base("abc".into());
        let judged = github_threads(&lines, "", &base, &[github(Side::New, 2, "TWO", false)]);
        assert_eq!(review::prompt(&judged), "");
        let r = Reading::new(&lines, "", &base, &judged);
        let mut pane = Pane::default();
        pane.go(&r, Go::By(4));
        pane.preview(&judged);
        assert_eq!(pane.mode, Mode::Browsing);
        pane.edit(&r);
        assert_eq!(pane.mode, Mode::Browsing);
        assert_eq!(pane.delete(&r), None);
        assert_eq!(pane.resolve(&r), None);
        assert!(pane.notice.as_deref().is_some_and(|n| n.contains("GitHub")));
    }

    #[test]
    fn taking_a_github_thread_keeps_a_comment_of_the_user_s_on_its_lines_naming_each_author_and_linking_it()
     {
        let lines = diff();
        let base = Rev::Base("abc".into());
        let judged = github_threads(&lines, "", &base, &[github(Side::New, 2, "TWO", false)]);
        let r = Reading::new(&lines, "", &base, &judged);
        let mut pane = Pane::default();
        assert_eq!(pane.take(&r), None);
        pane.go(&r, Go::By(4));
        let Some(Change::Add(new)) = pane.take(&r) else {
            panic!("nothing taken");
        };
        assert_eq!(new.author, None);
        assert_eq!(new.rev, base);
        assert_eq!(new.anchor, judged[0].comment.anchor);
        assert_eq!(new.quoted, vec!["TWO".to_string()]);
        assert!(
            new.text
                .contains("https://github.com/o/r/pull/1#discussion_r9")
        );
        assert!(new.text.contains("@alice: Why upper case?") && new.text.contains("@bob: Agreed"));
        let own = [stored(4, Side::New, 2, "TWO", Freshness::Current)];
        let r = reading(&lines, &own);
        assert_eq!(pane.take(&r), None, "a comment of the user's is not taken");
    }
}
