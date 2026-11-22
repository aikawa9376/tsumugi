//! The downloads of a page, which a shelf under its toolbar lists while they
//! run and after they have finished.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;

use crate::icons::{Icon, IconView};

/// How far a download has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub id: u64,
    /// What the file is called where it is kept.
    pub name: String,
    /// The address it was asked of.
    pub url: String,
    /// Where it is kept.
    pub path: PathBuf,
    pub state: State,
}

#[derive(Debug, Default)]
pub(super) struct Shelf {
    rows: Vec<Row>,
    next: u64,
    /// How many times the rows changed, which each page that shows them
    /// reads against what it saw last.
    version: u64,
}

impl Shelf {
    /// A download of `url` starts, to be kept at `path`: the path is moved to
    /// one nothing is kept at, as `exists` says, so that no file is
    /// overwritten.
    pub fn started(&mut self, url: String, path: &mut PathBuf, exists: impl Fn(&Path) -> bool) {
        *path = unused(path, exists);
        self.next += 1;
        self.rows.push(Row {
            id: self.next,
            name: path
                .file_name()
                .map_or_else(|| url.clone(), |name| name.to_string_lossy().into_owned()),
            url,
            path: path.clone(),
            state: State::Running,
        });
        self.version += 1;
    }

    /// The oldest running download of `url` ends, kept at `path` when the
    /// platform says where.
    pub fn finished(&mut self, url: &str, path: Option<PathBuf>, ok: bool) {
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.url == url && row.state == State::Running)
        else {
            return;
        };
        row.state = if ok { State::Done } else { State::Failed };
        if let Some(path) = path.filter(|_| ok) {
            row.path = path;
        }
        self.version += 1;
    }

    /// The row `id` leaves the shelf.
    pub fn dismiss(&mut self, id: u64) {
        self.rows.retain(|row| row.id != id);
        self.version += 1;
    }

    /// The rows, if they changed since the version `seen`, which becomes the
    /// version these are of.
    pub fn since(&self, seen: &mut u64) -> Option<Vec<Row>> {
        (std::mem::replace(seen, self.version) != self.version).then(|| self.rows.clone())
    }

    pub fn row(&self, id: u64) -> Option<&Row> {
        self.rows.iter().find(|row| row.id == id)
    }
}

/// The shelf under a page's toolbar: one line to a download, with what can
/// be done with it.
#[component]
pub(super) fn Rows(
    rows: Vec<Row>,
    on_open: EventHandler<u64>,
    on_show: EventHandler<u64>,
    on_dismiss: EventHandler<u64>,
) -> Element {
    rsx! {
        div { class: "browser-downloads", role: "list", aria_label: "Downloads",
            for row in rows {
                div { key: "{row.id}", class: "browser-download", role: "listitem",
                    match row.state {
                        State::Running => rsx! { IconView { icon: Icon::FolderDown, size: 14 } },
                        State::Done => rsx! { IconView { icon: Icon::Check, size: 14 } },
                        State::Failed => rsx! { IconView { icon: Icon::AlertTriangle, size: 14 } },
                    }
                    div { class: "download-text",
                        span { class: "download-name", title: "{row.path.display()}", "{row.name}" }
                        span { class: "download-status",
                            match row.state {
                                State::Running => format!("Downloading from {}", super::name_of(&row.url)),
                                State::Done => "Downloaded".to_string(),
                                State::Failed => "Download failed".to_string(),
                            }
                        }
                    }
                    if row.state == State::Done {
                        button { class: "download-action", r#type: "button",
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| on_open.call(row.id),
                            "Open"
                        }
                        button { class: "download-action", r#type: "button",
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| on_show.call(row.id),
                            "Show"
                        }
                    }
                    if row.state != State::Running {
                        button { class: "download-action", r#type: "button", aria_label: "Dismiss",
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| on_dismiss.call(row.id),
                            IconView { icon: Icon::X, size: 12 }
                        }
                    }
                }
            }
        }
    }
}

/// `path`, or the first of `name (1).ext`, `name (2).ext`… beside it that
/// `exists` does not say is taken.
fn unused(path: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    if !exists(path) {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){extension}")))
        .find(|candidate| !exists(candidate))
        .expect("an unbounded count has an unused name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing(_: &Path) -> bool {
        false
    }

    fn start(shelf: &mut Shelf, url: &str, path: &str) -> PathBuf {
        let mut path = PathBuf::from(path);
        shelf.started(url.into(), &mut path, nothing);
        path
    }

    #[test]
    fn a_download_is_listed_by_the_name_of_its_file_while_it_runs() {
        let mut shelf = Shelf::default();
        let mut seen = 0;
        start(&mut shelf, "https://a.example/x", "/d/report.pdf");

        let rows = shelf.since(&mut seen).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "report.pdf");
        assert_eq!(rows[0].state, State::Running);
        assert_eq!(shelf.since(&mut seen), None);
    }

    #[test]
    fn a_file_already_there_is_never_overwritten() {
        let mut shelf = Shelf::default();
        let mut seen = 0;
        let taken = |p: &Path| p == Path::new("/d/a.tar.gz") || p == Path::new("/d/a.tar (1).gz");
        let mut path = PathBuf::from("/d/a.tar.gz");
        shelf.started("https://a.example/".into(), &mut path, taken);

        assert_eq!(path, Path::new("/d/a.tar (2).gz"));
        assert_eq!(shelf.since(&mut seen).unwrap()[0].name, "a.tar (2).gz");
        let mut free = PathBuf::from("/d/b");
        shelf.started("https://b.example/".into(), &mut free, taken);
        assert_eq!(free, Path::new("/d/b"));
    }

    #[test]
    fn a_download_ends_done_where_it_was_kept_or_failed() {
        let mut shelf = Shelf::default();
        let mut seen = 0;
        start(&mut shelf, "https://a.example/1", "/d/1");
        start(&mut shelf, "https://a.example/2", "/d/2");
        shelf.since(&mut seen);

        shelf.finished("https://a.example/1", Some("/elsewhere/1".into()), true);
        shelf.finished("https://a.example/2", None, false);

        let rows = shelf.since(&mut seen).unwrap();
        assert_eq!(rows[0].state, State::Done);
        assert_eq!(rows[0].path, Path::new("/elsewhere/1"));
        assert_eq!(rows[1].state, State::Failed);
        assert_eq!(rows[1].path, Path::new("/d/2"));
    }

    #[test]
    fn a_platform_that_does_not_say_where_leaves_the_path_it_started_at() {
        let mut shelf = Shelf::default();
        let mut seen = 0;
        start(&mut shelf, "https://a.example/1", "/d/1");
        shelf.finished("https://a.example/1", None, true);

        let rows = shelf.since(&mut seen).unwrap();
        assert_eq!(rows[0].state, State::Done);
        assert_eq!(rows[0].path, Path::new("/d/1"));
    }

    #[test]
    fn two_downloads_of_one_address_end_in_the_order_they_began() {
        let mut shelf = Shelf::default();
        let mut seen = 0;
        start(&mut shelf, "https://a.example/f", "/d/f");
        start(&mut shelf, "https://a.example/f", "/d/f2");
        shelf.finished("https://a.example/f", None, true);

        let rows = shelf.since(&mut seen).unwrap();
        assert_eq!(rows[0].state, State::Done);
        assert_eq!(rows[1].state, State::Running);
    }

    #[test]
    fn every_page_that_shows_the_shelf_sees_a_change_once() {
        let mut shelf = Shelf::default();
        let (mut one, mut two) = (0, 0);
        start(&mut shelf, "https://a.example/1", "/d/1");

        assert_eq!(shelf.since(&mut one).map(|rows| rows.len()), Some(1));
        assert_eq!(shelf.since(&mut one), None);
        assert_eq!(shelf.since(&mut two).map(|rows| rows.len()), Some(1));
    }

    #[test]
    fn a_row_dismissed_leaves_the_shelf() {
        let mut shelf = Shelf::default();
        let mut seen = 0;
        start(&mut shelf, "https://a.example/1", "/d/1");
        start(&mut shelf, "https://a.example/2", "/d/2");
        let id = shelf.since(&mut seen).unwrap()[0].id;

        shelf.dismiss(id);

        let rows = shelf.since(&mut seen).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "2");
        assert!(shelf.row(id).is_none());
    }
}
