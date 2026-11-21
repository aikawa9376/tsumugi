//! What needed the user, kept in the instance's state folder from one run of
//! the work host to the next.

use std::path::{Path, PathBuf};

use tsumugi_model::{Notice, Notices, Tree};

const FILE: &str = "notices.json";

/// The notices of one work host, and the file they are kept in.
#[derive(Debug)]
pub struct Kept {
    file: PathBuf,
    notices: Notices,
    /// Whether the file may be written: not once it held what could not be
    /// read, which is left as it is rather than written over.
    writable: bool,
}

impl Kept {
    /// What `directory` keeps; none before any is.
    pub fn open(directory: &Path) -> Self {
        let file = directory.join(FILE);
        match std::fs::read(&file) {
            Ok(bytes) => match Notices::from_json(&bytes) {
                Ok(notices) => Self {
                    file,
                    notices,
                    writable: true,
                },
                Err(error) => {
                    tracing::warn!(%error, file = %file.display(), "cannot read the notices kept");
                    Self {
                        file,
                        notices: Notices::default(),
                        writable: false,
                    }
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self {
                file,
                notices: Notices::default(),
                writable: true,
            },
            Err(error) => {
                tracing::warn!(%error, file = %file.display(), "cannot read the notices kept");
                Self {
                    file,
                    notices: Notices::default(),
                    writable: false,
                }
            }
        }
    }

    pub fn list(&self) -> &[Notice] {
        self.notices.list()
    }

    /// Look at `tree` at `now_ms`, keeping what it newly shows.
    pub fn look(&mut self, tree: &Tree, now_ms: u64) {
        if self.notices.look(tree, now_ms) && self.writable {
            let written = self
                .file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| {
                    tsumugi_sys::fs::replace(&self.file, self.notices.to_json().into_bytes(), 0o644)
                });
            if let Err(error) = written {
                tracing::warn!(%error, "cannot keep the notices");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::State;
    use tsumugi_model::testing::{session, tree};

    fn with(state: State) -> Tree {
        tree()
            .session(session("a").cwd("/api").state(state))
            .build()
    }

    #[test]
    fn what_was_noticed_comes_back_after_the_host_starts_again() {
        let directory = tempfile::tempdir().unwrap();
        let mut kept = Kept::open(directory.path());
        kept.look(&with(State::Working), 1);
        kept.look(&with(State::Done), 2);
        let back = Kept::open(directory.path());
        assert_eq!(back.list(), kept.list());
        assert_eq!(back.list().len(), 1);
    }

    #[test]
    fn what_still_stands_when_the_host_starts_again_is_not_noticed_twice() {
        let directory = tempfile::tempdir().unwrap();
        let mut kept = Kept::open(directory.path());
        kept.look(&with(State::Working), 1);
        kept.look(&with(State::Done), 2);
        let mut back = Kept::open(directory.path());
        back.look(&with(State::Done), 3);
        assert_eq!(back.list().len(), 1);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_written_over() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(FILE), "not json").unwrap();
        let mut kept = Kept::open(directory.path());
        kept.look(&with(State::Working), 1);
        kept.look(&with(State::Done), 2);
        assert_eq!(kept.list().len(), 1);
        assert_eq!(
            std::fs::read_to_string(directory.path().join(FILE)).unwrap(),
            "not json"
        );
    }
}
