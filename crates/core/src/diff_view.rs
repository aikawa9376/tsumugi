//! What a diff tab shows beside the diff itself: the changes to move over,
//! the files to move between, how long lines and pictures are drawn, and
//! what a worktree's tabs keep of those choices.

use std::collections::HashMap;
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

/// What a worktree's diff tabs keep between one and the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffView {
    /// Long lines are broken to the width instead of scrolled across.
    pub wrap: bool,
    /// The list of the files changed is beside the diff.
    pub files: bool,
    pub pictures: Pictures,
}

impl Default for DiffView {
    fn default() -> Self {
        DiffView {
            wrap: false,
            files: true,
            pictures: Pictures::default(),
        }
    }
}

/// A choice a key makes about how a diff is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Choice {
    Wrap,
    Files,
    Pictures,
}

/// The choices of each worktree, for as long as the window runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiffViews(HashMap<(EnvironmentId, PathBuf), DiffView>);

impl DiffViews {
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
        }
    }
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
