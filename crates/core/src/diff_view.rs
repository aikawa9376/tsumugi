//! What a diff tab shows beside the diff itself: the changes to move over,
//! the files to move between, how long lines and pictures are drawn, and
//! what a worktree's tabs keep of those choices.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use tsumugi_model::EnvironmentId;
use tsumugi_model::repository::{Change, DiffKind, DiffLine, Rev};

/// Which way a key moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Way {
    Next,
    Prev,
}

/// `text` without the colour and style sequences difftastic writes.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        if chars.next() == Some('[') {
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
    }
    out
}

/// Where each change of a diff begins: a unified diff's hunk headings, and
/// for difftastic's text each run of lines that follows a blank one.
pub fn hunk_starts(lines: &[DiffLine]) -> Vec<usize> {
    if lines.iter().any(|l| l.kind == DiffKind::Difft) {
        let blank: Vec<bool> = lines
            .iter()
            .map(|l| plain(&l.text).trim().is_empty())
            .collect();
        return (0..lines.len())
            .filter(|&i| !blank[i] && (i == 0 || blank[i - 1]))
            .collect();
    }
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == DiffKind::Hunk)
        .map(|(i, _)| i)
        .collect()
}

/// The start in `starts`, which is in order, that `way` reaches from line
/// `at`; `None` before the first line asks for the first start.
pub fn hunk_from(starts: &[usize], at: Option<usize>, way: Way) -> Option<usize> {
    match (way, at) {
        (Way::Next, None) => starts.first().copied(),
        (Way::Next, Some(at)) => starts.iter().copied().find(|&s| s > at),
        (Way::Prev, None) => None,
        (Way::Prev, Some(at)) => starts.iter().rev().copied().find(|&s| s < at),
    }
}

/// The changes a diff tab on a worktree switches between: the worktree
/// against its base first, where the branch can be compared, then what is
/// not committed, then what the branch committed.
pub fn changes(merge_base: Option<&str>) -> Vec<Rev> {
    match merge_base {
        Some(base) => vec![
            Rev::Base(base.to_string()),
            Rev::WorkingCopy,
            Rev::Branch(base.to_string()),
        ],
        None => vec![Rev::WorkingCopy],
    }
}

fn short(id: &str) -> String {
    id.chars().take(7).collect()
}

/// What a diff tab calls the change `rev`.
pub fn called(rev: &Rev) -> String {
    match rev {
        Rev::Base(_) => "Branch".into(),
        Rev::WorkingCopy => "Uncommitted".into(),
        Rev::Branch(_) => "Committed".into(),
        Rev::Commit(id) => short(id),
        Rev::Range(from, to) => format!("{}..{}", short(from), short(to)),
    }
}

/// The two revisions written to compare, `from..to` or `from to`; one
/// alone, or more than two, compares nothing.
pub fn compared(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    let (from, to) = match text.split_once("..") {
        Some((from, to)) => (from.trim(), to.trim().trim_start_matches('.')),
        None => {
            let mut words = text.split_whitespace();
            let pair = (words.next()?, words.next()?);
            if words.next().is_some() {
                return None;
            }
            pair
        }
    };
    (!from.is_empty() && !to.is_empty() && !to.contains(char::is_whitespace))
        .then(|| (from.to_string(), to.to_string()))
}

/// How the two versions of a picture are drawn against each other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Pictures {
    /// One beside the other.
    #[default]
    SideBySide,
    /// One over the other, a divider moved across to uncover the old.
    Swipe,
    /// The old faded over the new.
    Onion,
}

impl Pictures {
    pub fn next(self) -> Pictures {
        match self {
            Pictures::SideBySide => Pictures::Swipe,
            Pictures::Swipe => Pictures::Onion,
            Pictures::Onion => Pictures::SideBySide,
        }
    }
}

/// Where a version of a picture is read from on the work machine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    /// The Git revision it is read in, or the worktree's own file.
    pub rev: Option<String>,
    pub path: String,
}

/// Where the old and the new version of the file `change` makes are read
/// from when the diff was read from `rev`; a side the change leaves empty,
/// as the old of a new file, is `None`.
pub fn versions(rev: &Rev, change: &Change) -> (Option<Source>, Option<Source>) {
    let status = change.status.as_str();
    let added = status == "??" || status.trim_start().starts_with('A');
    let deleted = status.contains('D');
    let source = |rev: Option<String>, path: &str| Source {
        rev,
        path: path.to_string(),
    };
    let old_path = change.from.as_deref().unwrap_or(&change.path);
    let old = (!added).then(|| match rev {
        Rev::WorkingCopy => source(Some("HEAD".into()), old_path),
        Rev::Commit(id) => source(Some(format!("{id}^")), old_path),
        Rev::Branch(since) | Rev::Base(since) => source(Some(since.clone()), old_path),
        Rev::Range(from, _) => source(Some(from.clone()), old_path),
    });
    let new = (!deleted).then(|| match rev {
        Rev::WorkingCopy => source(None, &change.path),
        Rev::Commit(id) => source(Some(id.clone()), &change.path),
        Rev::Branch(_) => source(Some("HEAD".into()), &change.path),
        Rev::Base(_) => source(None, &change.path),
        Rev::Range(_, to) => source(Some(to.clone()), &change.path),
    });
    (old, new)
}

/// How many columns a file's lines are drawn in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Columns {
    /// Two where the tab is wide enough for them, one where it is not.
    #[default]
    Auto,
    /// Removed lines above added ones.
    One,
    /// The old beside the new.
    Two,
}

impl Columns {
    pub fn next(self) -> Columns {
        match self {
            Columns::Auto => Columns::One,
            Columns::One => Columns::Two,
            Columns::Two => Columns::Auto,
        }
    }

    /// The narrowest tab, in pixels, that draws two columns by itself.
    pub const TWO_FROM: f64 = 1100.0;

    /// Whether a tab `width` pixels wide draws two columns.
    pub fn two_at(self, width: f64) -> bool {
        match self {
            Columns::Auto => width >= Self::TWO_FROM,
            Columns::One => false,
            Columns::Two => true,
        }
    }
}

/// What a worktree's diff tabs keep between one and the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffView {
    /// Long lines are broken to the width instead of scrolled across.
    pub wrap: bool,
    /// The list of the files changed is beside the diff.
    pub files: bool,
    pub pictures: Pictures,
    pub columns: Columns,
    /// The file opened is shown as difftastic reads it, not as lines.
    pub structural: bool,
}

impl Default for DiffView {
    fn default() -> Self {
        DiffView {
            wrap: false,
            files: true,
            pictures: Pictures::default(),
            columns: Columns::default(),
            structural: false,
        }
    }
}

/// One row of a file's lines drawn in two columns: a line both sides share
/// or that belongs to neither, or a removed line beside the added one that
/// took its place, either missing where the other side has more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Whole(usize),
    Pair(Option<usize>, Option<usize>),
}

/// The rows `lines` are drawn in two columns: each run of removed lines is
/// set beside the run of added lines right after it, line by line.
pub fn rows(lines: &[DiffLine]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let removed = lines[i..]
            .iter()
            .take_while(|l| l.kind == DiffKind::Removed)
            .count();
        let added = lines[i + removed..]
            .iter()
            .take_while(|l| l.kind == DiffKind::Added)
            .count();
        if removed + added == 0 {
            rows.push(Row::Whole(i));
            i += 1;
            continue;
        }
        for k in 0..removed.max(added) {
            rows.push(Row::Pair(
                (k < removed).then_some(i + k),
                (k < added).then_some(i + removed + k),
            ));
        }
        i += removed + added;
    }
    rows
}

/// The line each removed or added line of `lines` is compared with to mark
/// the words that changed: the one beside it in two columns.
pub fn partners(lines: &[DiffLine]) -> Vec<Option<usize>> {
    let mut partner = vec![None; lines.len()];
    for row in rows(lines) {
        if let Row::Pair(Some(old), Some(new)) = row {
            partner[old] = Some(new);
            partner[new] = Some(old);
        }
    }
    partner
}

/// The words to mark of each line of `lines`, as byte ranges of its text:
/// those a removed or added line does not share with the line beside it in
/// two columns, its sign left out.
pub fn word_marks(lines: &[DiffLine]) -> Vec<Vec<(usize, usize)>> {
    let mut marks = vec![Vec::new(); lines.len()];
    for row in rows(lines) {
        let Row::Pair(Some(old), Some(new)) = row else {
            continue;
        };
        let body = |i: usize| lines[i].text.get(1..).unwrap_or_default();
        let (a, b) = changed_words(body(old), body(new));
        let shift =
            |ranges: Vec<(usize, usize)>| ranges.into_iter().map(|(s, e)| (s + 1, e + 1)).collect();
        marks[old] = shift(a);
        marks[new] = shift(b);
    }
    marks
}

/// The line `way` of `from` whose text holds `text`, going round at the
/// ends; the case is ignored unless `text` has a capital.
pub fn find(lines: &[DiffLine], from: usize, text: &str, way: Way) -> Option<usize> {
    if text.is_empty() || lines.is_empty() {
        return None;
    }
    let careful = text.chars().any(char::is_uppercase);
    let wanted = if careful {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    let holds = |i: usize| {
        let line = &lines[i].text;
        if careful {
            line.contains(&wanted)
        } else {
            line.to_lowercase().contains(&wanted)
        }
    };
    let n = lines.len();
    (1..=n)
        .map(|k| match way {
            Way::Next => (from + k) % n,
            Way::Prev => (from + n * 2 - k) % n,
        })
        .find(|&i| holds(i))
}

/// The unchanged lines folded away before each hunk: by the index of its
/// heading, how many lines of the file come between it and the hunk before,
/// or the file's start.
pub fn folds(lines: &[DiffLine]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut after = 0u32;
    for (i, line) in lines.iter().enumerate() {
        match line.kind {
            DiffKind::File => after = 0,
            DiffKind::Hunk => {
                let Some((start, count)) = old_side(&line.text) else {
                    continue;
                };
                let hidden = start.saturating_sub(after + 1);
                if hidden > 0 && start > 0 {
                    out.push((i, hidden));
                }
                after = start + count.saturating_sub(1);
                if count == 0 {
                    after = start;
                }
            }
            _ => {}
        }
    }
    out
}

/// Where a hunk heading's old side starts, and how many lines it holds.
fn old_side(heading: &str) -> Option<(u32, u32)> {
    let word = heading.strip_prefix("@@ ")?.split_whitespace().next()?;
    let mut parts = word.strip_prefix('-')?.split(',');
    let start = parts.next()?.parse().ok()?;
    let count = parts.next().map_or(Some(1), |c| c.parse().ok())?;
    Some((start, count))
}

/// Why a patch cannot be made of the lines chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unpatchable {
    /// The lines are not in one hunk.
    NotOneHunk,
    /// A file added, deleted, renamed or binary is staged whole.
    WholeFile,
    /// Nothing chosen changes anything.
    NoChange,
}

impl Unpatchable {
    pub fn words(self) -> &'static str {
        match self {
            Unpatchable::NotOneHunk => "Choose lines of one hunk",
            Unpatchable::WholeFile => {
                "A file added, deleted, renamed or binary is staged whole, from Changes"
            }
            Unpatchable::NoChange => "No change here",
        }
    }
}

/// The patch that makes the change of the lines `first..=last` of `lines`,
/// or of the whole hunk when they are its heading; `reverse` makes the one
/// that takes the change back out, as unstaging and discarding apply. The
/// lines not chosen are left as they are on the side the patch applies to.
pub fn patch(
    lines: &[DiffLine],
    first: usize,
    last: usize,
    reverse: bool,
) -> Result<String, Unpatchable> {
    let (first, last) = (first.min(last), first.max(last));
    let heading = lines[..=first.min(lines.len().saturating_sub(1))]
        .iter()
        .rposition(|l| l.kind == DiffKind::Hunk)
        .ok_or(Unpatchable::NotOneHunk)?;
    let file = lines[..heading]
        .iter()
        .rposition(|l| l.kind == DiffKind::File)
        .ok_or(Unpatchable::NotOneHunk)?;
    let end = lines[heading + 1..]
        .iter()
        .position(|l| matches!(l.kind, DiffKind::Hunk | DiffKind::File))
        .map_or(lines.len(), |k| heading + 1 + k);
    if last >= end {
        return Err(Unpatchable::NotOneHunk);
    }
    // A note between the file's heading and its hunk says it is renamed or
    // binary; a hunk from or to nothing adds or deletes the file.
    let (old_start, new_start) =
        review_starts(&lines[heading].text).ok_or(Unpatchable::NotOneHunk)?;
    if heading != file + 1 || old_start == 0 || new_start == 0 {
        return Err(Unpatchable::WholeFile);
    }
    let whole = first == heading;
    let chosen = |i: usize| whole || (first..=last).contains(&i);
    let path = &lines[file].text;
    let (mut body, mut old, mut new, mut changes) = (String::new(), 0, 0, 0);
    for (i, line) in lines.iter().enumerate().take(end).skip(heading + 1) {
        let rest = line.text.get(1..).unwrap_or_default();
        // Read forwards the change goes from old to new; backwards a removed
        // line is what the patch adds back.
        let (adds, takes) = match (line.kind, reverse) {
            (DiffKind::Added, false) | (DiffKind::Removed, true) => (true, false),
            (DiffKind::Removed, false) | (DiffKind::Added, true) => (false, true),
            _ => (false, false),
        };
        if line.text.starts_with('\\') {
            body.push_str(&line.text);
            body.push('\n');
        } else if adds && chosen(i) {
            body.push_str(&format!("+{rest}\n"));
            new += 1;
            changes += 1;
        } else if takes && chosen(i) {
            body.push_str(&format!("-{rest}\n"));
            old += 1;
            changes += 1;
        } else if adds {
            // Not chosen: not there on the side the patch applies to.
        } else {
            body.push_str(&format!(" {rest}\n"));
            old += 1;
            new += 1;
        }
    }
    if changes == 0 {
        return Err(Unpatchable::NoChange);
    }
    let start = if reverse { new_start } else { old_start };
    Ok(format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{start},{old} +{start},{new} @@\n{body}"
    ))
}

/// Where a hunk heading, `@@ -a,b +c,d @@`, says its two sides start.
fn review_starts(heading: &str) -> Option<(u32, u32)> {
    let mut words = heading.strip_prefix("@@ ")?.split_whitespace();
    let start = |word: Option<&str>, sign: char| -> Option<u32> {
        word?.strip_prefix(sign)?.split(',').next()?.parse().ok()
    };
    Some((start(words.next(), '-')?, start(words.next(), '+')?))
}

/// `text` cut where `ranges`, in order, begin and end, each piece with
/// whether it is one of them.
pub fn pieces<'a>(text: &'a str, ranges: &[(usize, usize)]) -> Vec<(&'a str, bool)> {
    let mut out = Vec::new();
    let mut at = 0;
    for &(start, end) in ranges {
        let (start, end) = (start.min(text.len()), end.min(text.len()));
        if start < at || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        if start > at {
            out.push((&text[at..start], false));
        }
        if end > start {
            out.push((&text[start..end], true));
        }
        at = end;
    }
    if at < text.len() {
        out.push((&text[at..], false));
    }
    out
}

/// Byte ranges of a line's text, in order.
pub type Spans = Vec<(usize, usize)>;

/// Words, the spaces between them and each other character alone.
fn tokens(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let class = |c: char| {
            if c.is_alphanumeric() || c == '_' {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        };
        let kind = class(c);
        let mut end = start + c.len_utf8();
        if kind != 2 {
            while let Some(&(at, next)) = chars.peek() {
                if class(next) != kind {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
        }
        out.push((start, end));
    }
    out
}

/// The byte ranges of `old` and of `new` that the other does not share,
/// word by word; none where the two have nothing in common, as a line
/// rewritten whole is marked whole by its colour already.
pub fn changed_words(old: &str, new: &str) -> (Spans, Spans) {
    const MOST: usize = 400;
    let (a, b) = (tokens(old), tokens(new));
    if a.len() > MOST || b.len() > MOST {
        return (Vec::new(), Vec::new());
    }
    let word = |text: &str, (s, e): (usize, usize)| text[s..e].to_string();
    let (aw, bw): (Vec<String>, Vec<String>) = (
        a.iter().map(|t| word(old, *t)).collect(),
        b.iter().map(|t| word(new, *t)).collect(),
    );
    // The longest run they share, in order.
    let mut table = vec![vec![0u16; bw.len() + 1]; aw.len() + 1];
    for i in (0..aw.len()).rev() {
        for j in (0..bw.len()).rev() {
            table[i][j] = if aw[i] == bw[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    if table[0][0] == 0 {
        return (Vec::new(), Vec::new());
    }
    let (mut kept_a, mut kept_b) = (vec![false; aw.len()], vec![false; bw.len()]);
    let (mut i, mut j) = (0, 0);
    while i < aw.len() && j < bw.len() {
        if aw[i] == bw[j] {
            kept_a[i] = true;
            kept_b[j] = true;
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let ranges = |tokens: &[(usize, usize)], kept: &[bool]| {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for (t, keep) in tokens.iter().zip(kept) {
            if *keep {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.1 == t.0 => last.1 = t.1,
                _ => out.push(*t),
            }
        }
        out
    };
    (ranges(&a, &kept_a), ranges(&b, &kept_b))
}

/// A choice a key makes about how a diff is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Choice {
    Wrap,
    Files,
    Pictures,
    Columns,
    Structural,
}

/// The choices of each worktree, for as long as the window runs, and the
/// files of it shown whole rather than as their hunks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffViews(
    HashMap<Worktree, DiffView>,
    HashMap<Worktree, BTreeSet<String>>,
);

/// A worktree, by its environment and its path.
type Worktree = (EnvironmentId, PathBuf);

impl DiffViews {
    /// The files of the worktree shown whole.
    pub fn unfolded(&self, environment: &EnvironmentId, worktree: &Path) -> Vec<String> {
        self.1
            .get(&(environment.clone(), worktree.to_path_buf()))
            .map(|files| files.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Show the file `path` of the worktree whole, or as its hunks again.
    pub fn unfold(&mut self, environment: &EnvironmentId, worktree: &Path, path: &str) {
        let files = self
            .1
            .entry((environment.clone(), worktree.to_path_buf()))
            .or_default();
        if !files.remove(path) {
            files.insert(path.to_string());
        }
    }

    pub fn of(&self, environment: &EnvironmentId, worktree: &Path) -> DiffView {
        self.0
            .get(&(environment.clone(), worktree.to_path_buf()))
            .copied()
            .unwrap_or_default()
    }

    pub fn choose(&mut self, environment: &EnvironmentId, worktree: &Path, choice: Choice) {
        let view = self
            .0
            .entry((environment.clone(), worktree.to_path_buf()))
            .or_default();
        match choice {
            Choice::Wrap => view.wrap = !view.wrap,
            Choice::Files => view.files = !view.files,
            Choice::Pictures => view.pictures = view.pictures.next(),
            Choice::Columns => view.columns = view.columns.next(),
            Choice::Structural => view.structural = !view.structural,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_columns_are_drawn_where_the_tab_is_wide_enough_unless_one_or_two_is_chosen() {
        assert!(!Columns::Auto.two_at(Columns::TWO_FROM - 1.0));
        assert!(Columns::Auto.two_at(Columns::TWO_FROM));
        assert!(!Columns::One.two_at(4000.0));
        assert!(Columns::Two.two_at(200.0));
        assert_eq!(Columns::Auto.next().next().next(), Columns::Auto);
    }

    #[test]
    fn removed_lines_are_set_beside_the_added_lines_that_follow_them() {
        use DiffKind::*;
        let lines: Vec<_> = [
            (File, "a"),
            (Hunk, "@@"),
            (Context, " x"),
            (Removed, "-a"),
            (Removed, "-b"),
            (Added, "+A"),
            (Context, " y"),
            (Added, "+new"),
        ]
        .into_iter()
        .map(|(k, t)| line(k, t))
        .collect();
        assert_eq!(
            rows(&lines),
            [
                Row::Whole(0),
                Row::Whole(1),
                Row::Whole(2),
                Row::Pair(Some(3), Some(5)),
                Row::Pair(Some(4), None),
                Row::Whole(6),
                Row::Pair(None, Some(7)),
            ]
        );
        let partner = partners(&lines);
        assert_eq!(
            (partner[3], partner[5], partner[4]),
            (Some(5), Some(3), None)
        );
    }

    #[test]
    fn the_words_of_a_line_are_marked_against_the_line_beside_it_past_its_sign() {
        use DiffKind::*;
        let lines = [
            line(Removed, "-a = 1"),
            line(Added, "+a = 2"),
            line(Added, "+b"),
        ];
        let marks = word_marks(&lines);
        assert_eq!(marks[0], [(5, 6)]);
        assert_eq!(&lines[1].text[5..6], "2");
        assert_eq!(marks[1], [(5, 6)]);
        assert!(marks[2].is_empty(), "an added line beside nothing");
    }

    fn hunk() -> Vec<DiffLine> {
        use DiffKind::*;
        [
            (File, "a.txt"),
            (Hunk, "@@ -1,3 +1,3 @@"),
            (Context, " one"),
            (Removed, "-two"),
            (Removed, "-three"),
            (Added, "+TWO"),
            (Added, "+THREE"),
        ]
        .into_iter()
        .map(|(k, t)| line(k, t))
        .collect()
    }

    #[test]
    fn a_search_finds_the_next_or_previous_line_holding_the_text_going_round() {
        use DiffKind::*;
        let lines: Vec<_> = [(Context, " Alpha"), (Added, "+beta"), (Removed, "-alpha")]
            .into_iter()
            .map(|(k, t)| line(k, t))
            .collect();
        assert_eq!(find(&lines, 0, "alpha", Way::Next), Some(2));
        assert_eq!(find(&lines, 2, "alpha", Way::Next), Some(0));
        assert_eq!(find(&lines, 0, "alpha", Way::Prev), Some(2));
        assert_eq!(
            find(&lines, 2, "Alpha", Way::Next),
            Some(0),
            "a capital is kept"
        );
        assert_eq!(
            find(&lines, 0, "Alpha", Way::Next),
            Some(0),
            "the line itself last"
        );
        assert_eq!(find(&lines, 0, "gamma", Way::Next), None);
        assert_eq!(find(&lines, 0, "", Way::Next), None);
    }

    #[test]
    fn the_unchanged_lines_between_hunks_are_counted_for_their_folds() {
        use DiffKind::*;
        let lines: Vec<_> = [
            (File, "a"),
            (Hunk, "@@ -1,3 +1,3 @@"),
            (Hunk, "@@ -10,2 +10,3 @@"),
            (File, "b"),
            (Hunk, "@@ -5 +5 @@"),
            (File, "new"),
            (Hunk, "@@ -0,0 +1 @@"),
        ]
        .into_iter()
        .map(|(k, t)| line(k, t))
        .collect();
        assert_eq!(folds(&lines), [(2, 6), (4, 4)]);
    }

    #[test]
    fn a_file_is_shown_whole_and_then_as_its_hunks_again() {
        let mut views = DiffViews::default();
        let here = EnvironmentId::default();
        views.unfold(&here, Path::new("/w"), "a.rs");
        assert_eq!(views.unfolded(&here, Path::new("/w")), ["a.rs"]);
        assert!(views.unfolded(&here, Path::new("/other")).is_empty());
        views.unfold(&here, Path::new("/w"), "a.rs");
        assert!(views.unfolded(&here, Path::new("/w")).is_empty());
    }

    #[test]
    fn a_whole_hunk_is_patched_from_its_heading() {
        assert_eq!(
            patch(&hunk(), 1, 1, false).unwrap(),
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n-three\n+TWO\n+THREE\n"
        );
    }

    #[test]
    fn lines_not_chosen_stay_as_they_are_on_the_side_the_patch_applies_to() {
        // Staging "-two" and "+TWO" leaves "three" in place and "+THREE" out.
        assert_eq!(
            patch(&hunk(), 3, 3, false).unwrap(),
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,2 @@\n one\n-two\n three\n"
        );
        // Taken back out, an added line not chosen is there to keep, and a
        // removed one not chosen is not.
        assert_eq!(
            patch(&hunk(), 5, 5, true).unwrap(),
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,2 @@\n one\n-TWO\n THREE\n"
        );
    }

    #[test]
    fn a_patch_is_refused_across_hunks_for_whole_files_and_where_nothing_changes() {
        let mut lines = hunk();
        lines.push(line(DiffKind::Hunk, "@@ -9 +9 @@"));
        lines.push(line(DiffKind::Added, "+x"));
        assert_eq!(patch(&lines, 3, 8, false), Err(Unpatchable::NotOneHunk));
        assert_eq!(patch(&lines, 0, 0, false), Err(Unpatchable::NotOneHunk));
        assert_eq!(patch(&lines, 2, 2, false), Err(Unpatchable::NoChange));
        let added = [
            line(DiffKind::File, "new.txt"),
            line(DiffKind::Hunk, "@@ -0,0 +1 @@"),
            line(DiffKind::Added, "+fresh"),
        ];
        assert_eq!(patch(&added, 2, 2, false), Err(Unpatchable::WholeFile));
        let renamed = [
            line(DiffKind::File, "b.txt"),
            line(DiffKind::Context, "Renamed from a.txt"),
            line(DiffKind::Hunk, "@@ -1 +1 @@"),
            line(DiffKind::Added, "+x"),
        ];
        assert_eq!(patch(&renamed, 3, 3, false), Err(Unpatchable::WholeFile));
    }

    #[test]
    fn a_line_is_cut_into_the_words_marked_and_those_between_them() {
        assert_eq!(
            pieces("let sum = a", &[(4, 7), (10, 11)]),
            [("let ", false), ("sum", true), (" = ", false), ("a", true)]
        );
        assert_eq!(pieces("ab", &[]), [("ab", false)]);
        assert_eq!(pieces("", &[]), [] as [(&str, bool); 0]);
    }

    #[test]
    fn the_words_that_changed_inside_a_line_are_marked_on_each_side() {
        let (old, new) = changed_words("let total = a + b;", "let sum = a + b + c;");
        let said = |text: &str, ranges: &[(usize, usize)]| {
            ranges
                .iter()
                .map(|&(s, e)| text[s..e].to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(said("let total = a + b;", &old), ["total"]);
        assert_eq!(said("let sum = a + b + c;", &new), ["sum", " + c"]);
        let (old, new) = changed_words("名前 = 古い", "名前 = 新しい");
        assert_eq!(said("名前 = 新しい", &new), ["新しい"]);
        assert_eq!(said("名前 = 古い", &old), ["古い"]);
        assert_eq!(
            changed_words("abc", "xyz"),
            (vec![], vec![]),
            "nothing shared"
        );
    }

    fn line(kind: DiffKind, text: &str) -> DiffLine {
        DiffLine {
            kind,
            text: text.into(),
        }
    }

    fn change(status: &str, path: &str, from: Option<&str>) -> Change {
        Change {
            status: status.into(),
            path: path.into(),
            from: from.map(Into::into),
        }
    }

    fn unified() -> Vec<DiffLine> {
        vec![
            line(DiffKind::File, "a.rs"),
            line(DiffKind::Hunk, "@@ -1 +1 @@"),
            line(DiffKind::Removed, "-x"),
            line(DiffKind::Added, "+y"),
            line(DiffKind::Hunk, "@@ -9 +9 @@"),
            line(DiffKind::Context, " z"),
            line(DiffKind::Hunk, "@@ -20 +20 @@"),
        ]
    }

    #[test]
    fn a_line_diffs_changes_begin_at_its_hunk_headings() {
        assert_eq!(hunk_starts(&unified()), [1, 4, 6]);
        assert_eq!(
            hunk_starts(&[line(DiffKind::File, "a.rs")]),
            [] as [usize; 0]
        );
        assert_eq!(hunk_starts(&[]), [] as [usize; 0]);
    }

    #[test]
    fn a_structural_diffs_changes_begin_after_each_blank_line() {
        let lines: Vec<_> = [
            "a.rs --- 1/2 --- Rust",
            "1 \x1b[31mold\x1b[0m",
            "",
            "a.rs --- 2/2 --- Rust",
            "9 \x1b[32mnew\x1b[0m",
            "\x1b[0m",
            "",
            "tail",
        ]
        .into_iter()
        .map(|t| line(DiffKind::Difft, t))
        .collect();
        assert_eq!(hunk_starts(&lines), [0, 3, 7]);
    }

    #[test]
    fn the_next_change_is_the_first_one_after_the_line_and_the_previous_the_last_before_it() {
        let starts = [1, 4, 6];
        assert_eq!(hunk_from(&starts, None, Way::Next), Some(1));
        assert_eq!(hunk_from(&starts, Some(0), Way::Next), Some(1));
        assert_eq!(hunk_from(&starts, Some(1), Way::Next), Some(4));
        assert_eq!(hunk_from(&starts, Some(3), Way::Next), Some(4));
        assert_eq!(hunk_from(&starts, Some(6), Way::Next), None);
        assert_eq!(hunk_from(&starts, Some(6), Way::Prev), Some(4));
        assert_eq!(hunk_from(&starts, Some(5), Way::Prev), Some(4));
        assert_eq!(hunk_from(&starts, Some(1), Way::Prev), None);
        assert_eq!(hunk_from(&starts, None, Way::Prev), None);
        assert_eq!(hunk_from(&[], Some(0), Way::Next), None);
    }

    #[test]
    fn pictures_are_compared_side_by_side_then_swiped_then_faded_and_round_again() {
        let mut how = Pictures::default();
        assert_eq!(how, Pictures::SideBySide);
        let mut seen = Vec::new();
        for _ in 0..4 {
            how = how.next();
            seen.push(how);
        }
        assert_eq!(
            seen,
            [
                Pictures::Swipe,
                Pictures::Onion,
                Pictures::SideBySide,
                Pictures::Swipe
            ]
        );
    }

    fn at(rev: Option<&str>, path: &str) -> Option<Source> {
        Some(Source {
            rev: rev.map(Into::into),
            path: path.into(),
        })
    }

    #[test]
    fn a_comparable_branch_switches_from_its_base_to_what_is_uncommitted_and_what_it_committed() {
        assert_eq!(
            changes(Some("m")),
            [
                Rev::Base("m".into()),
                Rev::WorkingCopy,
                Rev::Branch("m".into())
            ]
        );
        assert_eq!(changes(None), [Rev::WorkingCopy]);
        assert_eq!(called(&Rev::Base("m".into())), "Branch");
        assert_eq!(
            called(&Rev::Range("0123456789".into(), "abcdefghij".into())),
            "0123456..abcdefg"
        );
    }

    #[test]
    fn two_revisions_are_written_with_two_dots_or_a_space_between_them() {
        let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        assert_eq!(compared("main..HEAD"), pair("main", "HEAD"));
        assert_eq!(compared(" v1 .. v2 "), pair("v1", "v2"));
        assert_eq!(compared("main...topic"), pair("main", "topic"));
        assert_eq!(compared("v1 v2"), pair("v1", "v2"));
        for nothing in ["", "main", "main..", "..main", "a b c", "a..b c"] {
            assert_eq!(compared(nothing), None, "{nothing:?}");
        }
    }

    #[test]
    fn a_picture_is_read_on_each_side_of_the_change_it_is_in() {
        let edit = change(" M", "a.png", None);
        assert_eq!(
            versions(&Rev::WorkingCopy, &edit),
            (at(Some("HEAD"), "a.png"), at(None, "a.png"))
        );
        assert_eq!(
            versions(&Rev::Commit("abc".into()), &edit),
            (at(Some("abc^"), "a.png"), at(Some("abc"), "a.png"))
        );
        assert_eq!(
            versions(&Rev::Branch("base".into()), &edit),
            (at(Some("base"), "a.png"), at(Some("HEAD"), "a.png"))
        );
        assert_eq!(
            versions(&Rev::Base("base".into()), &edit),
            (at(Some("base"), "a.png"), at(None, "a.png"))
        );
        assert_eq!(
            versions(&Rev::Range("v1".into(), "v2".into()), &edit),
            (at(Some("v1"), "a.png"), at(Some("v2"), "a.png"))
        );
    }

    #[test]
    fn an_added_picture_has_no_old_side_and_a_deleted_one_no_new_side() {
        for status in ["??", "A ", "A", "AM"] {
            let (old, new) = versions(&Rev::WorkingCopy, &change(status, "a.png", None));
            assert_eq!((old, new.is_some()), (None, true), "{status}");
        }
        for status in [" D", "D ", "D"] {
            let (old, new) = versions(&Rev::Commit("abc".into()), &change(status, "a.png", None));
            assert_eq!((old.is_some(), new), (true, None), "{status}");
        }
    }

    #[test]
    fn a_renamed_picture_is_read_under_its_old_name_on_the_old_side() {
        let moved = change("R ", "new.png", Some("old.png"));
        assert_eq!(
            versions(&Rev::WorkingCopy, &moved),
            (at(Some("HEAD"), "old.png"), at(None, "new.png"))
        );
    }

    fn env(name: &str) -> EnvironmentId {
        EnvironmentId::new(name)
    }

    #[test]
    fn a_worktree_starts_with_the_files_listed_and_lines_not_wrapped() {
        let views = DiffViews::default();
        let view = views.of(&env("here"), Path::new("/w"));
        assert!(view.files);
        assert!(!view.wrap);
        assert_eq!(view.pictures, Pictures::SideBySide);
    }

    #[test]
    fn what_is_chosen_is_kept_for_that_worktree_alone() {
        let mut views = DiffViews::default();
        views.choose(&env("here"), Path::new("/w"), Choice::Wrap);
        views.choose(&env("here"), Path::new("/w"), Choice::Files);
        views.choose(&env("here"), Path::new("/w"), Choice::Pictures);
        let view = views.of(&env("here"), Path::new("/w"));
        assert!(view.wrap);
        assert!(!view.files);
        assert_eq!(view.pictures, Pictures::Swipe);
        assert_eq!(
            views.of(&env("here"), Path::new("/other")),
            DiffView::default()
        );
        assert_eq!(
            views.of(&env("there"), Path::new("/w")),
            DiffView::default()
        );
        views.choose(&env("here"), Path::new("/w"), Choice::Wrap);
        assert!(!views.of(&env("here"), Path::new("/w")).wrap);
    }
}
