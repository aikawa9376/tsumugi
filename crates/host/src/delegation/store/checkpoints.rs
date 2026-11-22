//! The line and the status each worktree shows, beside the sessions that
//! set them.

use std::path::{Path, PathBuf};

use rusqlite::params;
use tsumugi_model::{Checkpoint, Set, WorktreeStatus};

use super::{Result, Store};

impl Store {
    /// Change the checkpoint of `worktree` at `now_ms`: what is `Keep` stays,
    /// what is `Clear` is left empty. A checkpoint that comes to say nothing
    /// is not kept. What it now says is returned.
    pub fn set_checkpoint(
        &mut self,
        worktree: &Path,
        comment: Set<String>,
        status: Set<WorktreeStatus>,
        now_ms: u64,
    ) -> Result<Checkpoint> {
        let transaction = self.connection.transaction()?;
        let key = worktree.to_string_lossy().into_owned();
        let before = transaction
            .query_row(
                "SELECT comment, status, noted FROM checkpoints WHERE worktree = ?1",
                [&key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .ok();
        let (old_comment, old_status, noted) = before.unwrap_or_default();
        let comment = match comment {
            Set::Keep => old_comment,
            Set::Clear => String::new(),
            Set::To(text) => text,
        };
        let status = match status {
            Set::Keep => old_status.as_deref().and_then(WorktreeStatus::parse),
            Set::Clear => None,
            Set::To(status) => Some(status),
        };
        let now = Checkpoint {
            worktree: PathBuf::from(&key),
            comment,
            status,
            noted,
            updated_ms: now_ms,
        };
        if now.is_empty() {
            transaction.execute("DELETE FROM checkpoints WHERE worktree = ?1", [&key])?;
        } else {
            transaction.execute(
                "INSERT INTO checkpoints (worktree, comment, status, noted, updated_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(worktree) DO UPDATE SET comment = excluded.comment,
                     status = excluded.status, updated_ms = excluded.updated_ms",
                params![
                    key,
                    now.comment,
                    now.status.map(WorktreeStatus::word),
                    now.noted,
                    now_ms as i64
                ],
            )?;
        }
        transaction.commit()?;
        Ok(now)
    }

    /// Keep `note`, the latest an agent working in `worktree` wrote on its
    /// timeline, to show while the worktree has no comment. A note that is
    /// empty leaves what is kept, and the time the checkpoint was set is
    /// not changed: a note is not something anyone set.
    pub fn note_checkpoint(&mut self, worktree: &Path, note: &str) -> Result<()> {
        if note.is_empty() {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO checkpoints (worktree, comment, status, noted, updated_ms)
             VALUES (?1, '', NULL, ?2, 0)
             ON CONFLICT(worktree) DO UPDATE SET noted = excluded.noted",
            params![worktree.to_string_lossy(), note],
        )?;
        Ok(())
    }

    /// Every checkpoint that says something, the latest set last.
    pub fn checkpoints(&self) -> Result<Vec<Checkpoint>> {
        let mut query = self.connection.prepare(
            "SELECT worktree, comment, status, updated_ms, noted FROM checkpoints
             ORDER BY updated_ms, worktree",
        )?;
        let rows = query.query_map([], |row| {
            Ok(Checkpoint {
                worktree: PathBuf::from(row.get::<_, String>(0)?),
                comment: row.get(1)?,
                status: row
                    .get::<_, Option<String>>(2)?
                    .as_deref()
                    .and_then(WorktreeStatus::parse),
                noted: row.get(4)?,
                updated_ms: row.get::<_, i64>(3)? as u64,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
        (dir, store)
    }

    fn said(store: &Store) -> Vec<(String, String, Option<WorktreeStatus>)> {
        store
            .checkpoints()
            .unwrap()
            .into_iter()
            .map(|c| (c.worktree.display().to_string(), c.comment, c.status))
            .collect()
    }

    #[test]
    fn the_comment_and_the_status_are_set_apart_and_what_is_not_named_is_kept() {
        let (_dir, mut store) = store();
        let w = Path::new("/w");
        store
            .set_checkpoint(
                w,
                Set::To("tests are red".into()),
                Set::To(WorktreeStatus::InProgress),
                1,
            )
            .unwrap();
        store
            .set_checkpoint(w, Set::Keep, Set::To(WorktreeStatus::InReview), 2)
            .unwrap();
        assert_eq!(
            said(&store),
            [(
                "/w".into(),
                "tests are red".into(),
                Some(WorktreeStatus::InReview)
            )]
        );
        let now = store
            .set_checkpoint(w, Set::To("green".into()), Set::Keep, 3)
            .unwrap();
        assert_eq!(
            (now.comment.as_str(), now.status, now.updated_ms),
            ("green", Some(WorktreeStatus::InReview), 3)
        );
    }

    #[test]
    fn a_checkpoint_cleared_of_both_parts_is_no_longer_kept() {
        let (_dir, mut store) = store();
        let w = Path::new("/w");
        store
            .set_checkpoint(w, Set::To("x".into()), Set::To(WorktreeStatus::Todo), 1)
            .unwrap();
        store.set_checkpoint(w, Set::Clear, Set::Keep, 2).unwrap();
        assert_eq!(said(&store).len(), 1);
        store.set_checkpoint(w, Set::Keep, Set::Clear, 3).unwrap();
        assert!(said(&store).is_empty());
        let none = store.set_checkpoint(w, Set::Keep, Set::Keep, 4).unwrap();
        assert!(none.is_empty());
        assert!(said(&store).is_empty());
    }

    #[test]
    fn each_worktree_keeps_its_own_and_the_latest_set_is_listed_last() {
        let (_dir, mut store) = store();
        store
            .set_checkpoint(Path::new("/b"), Set::To("b".into()), Set::Keep, 5)
            .unwrap();
        store
            .set_checkpoint(Path::new("/a"), Set::To("a".into()), Set::Keep, 9)
            .unwrap();
        let worktrees: Vec<_> = said(&store).into_iter().map(|c| c.0).collect();
        assert_eq!(worktrees, ["/b", "/a"]);
    }

    #[test]
    fn a_timeline_note_is_kept_beside_what_was_set_and_survives_clearing_it() {
        let (_dir, mut store) = store();
        let w = Path::new("/w");
        store.note_checkpoint(w, "read the diff").unwrap();
        let [only] = &store.checkpoints().unwrap()[..] else {
            panic!("one checkpoint")
        };
        assert_eq!(
            (only.noted.as_str(), only.comment.as_str()),
            ("read the diff", "")
        );
        store
            .set_checkpoint(w, Set::To("mine".into()), Set::To(WorktreeStatus::Todo), 5)
            .unwrap();
        store.note_checkpoint(w, "wrote the test").unwrap();
        store.note_checkpoint(w, "").unwrap();
        let now = store.set_checkpoint(w, Set::Clear, Set::Clear, 6).unwrap();
        assert_eq!(now.noted, "wrote the test");
        assert!(!now.is_empty());
        assert_eq!(store.checkpoints().unwrap().len(), 1);
    }

    #[test]
    fn a_database_from_before_timeline_notes_keeps_what_worktrees_said() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        drop(Store::open(&path).unwrap());
        let connection = rusqlite::Connection::open(&path).unwrap();
        let before: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        connection
            .execute_batch(&format!(
                "DROP TABLE checkpoints;
                 CREATE TABLE checkpoints (worktree TEXT PRIMARY KEY, comment TEXT NOT NULL, status TEXT, updated_ms INTEGER NOT NULL);
                 INSERT INTO checkpoints VALUES ('/w', 'tests are red', 'in_review', 4);
                 PRAGMA user_version = {};",
                before - 1
            ))
            .unwrap();
        drop(connection);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            said(&store),
            [(
                "/w".into(),
                "tests are red".into(),
                Some(WorktreeStatus::InReview)
            )]
        );
        assert_eq!(store.checkpoints().unwrap()[0].noted, "");
    }

    #[test]
    fn a_database_from_before_checkpoints_gains_the_table() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        drop(Store::open(&path).unwrap());
        let connection = rusqlite::Connection::open(&path).unwrap();
        let before: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        connection
            .execute_batch(&format!(
                "DROP TABLE checkpoints; PRAGMA user_version = {};",
                before - 2
            ))
            .unwrap();
        drop(connection);
        let mut store = Store::open(&path).unwrap();
        store
            .set_checkpoint(Path::new("/w"), Set::To("x".into()), Set::Keep, 1)
            .unwrap();
        assert_eq!(said(&store).len(), 1);
    }
}
