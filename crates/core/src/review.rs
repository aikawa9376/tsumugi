//! A diff window's review of one file's line diff: the line the cursor is
//! on, the lines marked with it, the comment being written and the prompt
//! being previewed. The comments themselves are kept by the work host; the
//! window asks it, and this holds what the keys have decided.

use tsumugi_model::repository::{DiffLine, Rev};
use tsumugi_model::review::{self, Anchor, Change, Comment, Freshness, Judged, NewComment, Side};

/// What a pane is reading: the file's diff, and the comments of the worktree.
#[derive(Clone, Copy)]
pub struct Reading<'a> {
    pub lines: &'a [DiffLine],
    pub path: &'a str,
    pub rev: &'a Rev,
    pub comments: &'a [Judged],
}

impl<'a> Reading<'a> {
    fn here(&self) -> impl Iterator<Item = &'a Judged> + use<'a> {
        let (path, rev) = (self.path.to_owned(), self.rev.clone());
        self.comments
            .iter()
            .filter(move |j| j.comment.anchor.path == path && j.comment.rev == rev)
    }

    /// The comments of this file whose lines no longer read as they did,
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

    fn covers(&self, comment: &Comment, line: usize) -> bool {
        let n = review::numbered(self.lines)[line];
        let number = match comment.anchor.side {
            Side::Old => n.old,
            Side::New => n.new,
        };
        number.is_some_and(|n| (comment.anchor.start..=comment.anchor.end).contains(&n))
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
        let n = review::numbered(self.lines)[line];
        self.here()
            .filter(|j| {
                let a = &j.comment.anchor;
                j.freshness == Freshness::Current
                    && match a.side {
                        Side::Old => n.old,
                        Side::New => n.new,
                    } == Some(a.end)
            })
            .collect()
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

    /// Move the cursor `by` places, or to the first or last of them.
    pub fn go(&mut self, reading: &Reading, to: Go) {
        self.notice = None;
        let last = reading.places().saturating_sub(1);
        self.cursor = match to {
            Go::By(n) => self.cursor(reading).saturating_add_signed(n).min(last),
            Go::Top => 0,
            Go::Bottom => last,
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
        match review::anchor_of(reading.lines, reading.path, first, last) {
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
            .filter(|j| !j.comment.resolved)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::repository::DiffKind;

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
            },
            freshness,
        }
    }

    fn reading<'a>(lines: &'a [DiffLine], comments: &'a [Judged]) -> Reading<'a> {
        Reading {
            lines,
            path: "src/メモ.rs",
            rev: &Rev::WorkingCopy,
            comments,
        }
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
}
