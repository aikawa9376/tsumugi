//! The shell commands each project keeps to run again, beside the sessions
//! that read them.

use std::path::{Path, PathBuf};

use rusqlite::params;
use tsumugi_model::{NewSavedCommand, SavedCommand};

use super::{Code, Error, Result, Store};

impl Store {
    /// Keep `new` under its id in `project`, the root of the workspace that
    /// holds its directory; one kept under that id before is replaced in the
    /// place it had. A command kept by another project is not touched.
    pub fn save_command(&mut self, project: &Path, new: NewSavedCommand) -> Result<SavedCommand> {
        if let Some(refusal) = tsumugi_model::refuse_command(&new) {
            return Err(Error::new(Code::Invalid, refusal));
        }
        let project = project.to_string_lossy().into_owned();
        let label = new.label.trim().to_owned();
        let changed = self.connection.execute(
            "INSERT INTO saved_commands (id, project, label, command) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET label = excluded.label, command = excluded.command
             WHERE project = excluded.project",
            params![new.id, project, label, new.command],
        )?;
        if changed != 1 {
            return Err(Error::new(
                Code::Invalid,
                "Another project keeps a command with this id",
            ));
        }
        Ok(SavedCommand {
            id: new.id,
            project: PathBuf::from(project),
            label,
            command: new.command,
        })
    }

    /// What `project` keeps, in the order it was first kept; what every
    /// project keeps when none is named.
    pub fn saved_commands(&self, project: Option<&Path>) -> Result<Vec<SavedCommand>> {
        let mut query = self.connection.prepare(
            "SELECT id, project, label, command FROM saved_commands
             WHERE ?1 IS NULL OR project = ?1 ORDER BY seq",
        )?;
        let project = project.map(|p| p.to_string_lossy().into_owned());
        let rows = query.query_map([project], |row| {
            Ok(SavedCommand {
                id: row.get(0)?,
                project: PathBuf::from(row.get::<_, String>(1)?),
                label: row.get(2)?,
                command: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    }

    /// Keep the command of `id` no more.
    pub fn delete_saved_command(&mut self, id: &str) -> Result<()> {
        let removed = self
            .connection
            .execute("DELETE FROM saved_commands WHERE id = ?1", [id])?;
        if removed != 1 {
            return Err(Error::new(Code::NotFound, "No saved command has this id"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use tsumugi_model::NewSavedCommand;

    fn new(label: &str, command: &str) -> NewSavedCommand {
        NewSavedCommand {
            id: SessionId::new().to_string(),
            dir: "/p".into(),
            label: label.into(),
            command: command.into(),
        }
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
        (dir, store)
    }

    fn labels(store: &Store, project: Option<&Path>) -> Vec<String> {
        store
            .saved_commands(project)
            .unwrap()
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn a_project_lists_what_it_keeps_in_the_order_it_kept_it() {
        let (_dir, mut store) = store();
        store.save_command(Path::new("/p"), new("b", "x")).unwrap();
        store.save_command(Path::new("/p"), new("a", "x")).unwrap();
        store.save_command(Path::new("/q"), new("c", "x")).unwrap();
        assert_eq!(labels(&store, Some(Path::new("/p"))), ["b", "a"]);
        assert_eq!(labels(&store, None).len(), 3);
    }

    #[test]
    fn a_command_text_with_lines_spaces_and_unicode_is_kept_as_it_was_written() {
        let (_dir, mut store) = store();
        let text = "cd '/tmp/a b/日本語'\n  just ci \n";
        store
            .save_command(Path::new("/p"), new(" Checks ", text))
            .unwrap();
        let kept = store.saved_commands(None).unwrap();
        assert_eq!(kept[0].command, text);
        assert_eq!(kept[0].label, "Checks");
    }

    #[test]
    fn keeping_an_id_again_replaces_the_command_in_its_place() {
        let (_dir, mut store) = store();
        let first = new("one", "a");
        let id = first.id.clone();
        store.save_command(Path::new("/p"), first).unwrap();
        store
            .save_command(Path::new("/p"), new("two", "b"))
            .unwrap();
        let mut again = new("one edited", "a2");
        again.id = id;
        store.save_command(Path::new("/p"), again).unwrap();
        let kept = store.saved_commands(None).unwrap();
        assert_eq!(
            kept.iter()
                .map(|c| (&*c.label, &*c.command))
                .collect::<Vec<_>>(),
            [("one edited", "a2"), ("two", "b")]
        );
    }

    #[test]
    fn a_project_cannot_replace_a_command_another_project_keeps() {
        let (_dir, mut store) = store();
        let mine = new("mine", "a");
        let id = mine.id.clone();
        store.save_command(Path::new("/p"), mine).unwrap();
        let mut theirs = new("theirs", "b");
        theirs.id = id;
        let error = store.save_command(Path::new("/q"), theirs).unwrap_err();
        assert_eq!(error.code, Code::Invalid);
        assert_eq!(store.saved_commands(None).unwrap()[0].label, "mine");
    }

    #[test]
    fn a_command_that_holds_together_no_better_is_refused() {
        let (_dir, mut store) = store();
        let error = store
            .save_command(Path::new("/p"), new("", "x"))
            .unwrap_err();
        assert_eq!(error.code, Code::Invalid);
        assert!(store.saved_commands(None).unwrap().is_empty());
    }

    #[test]
    fn a_deleted_command_is_gone_and_one_never_kept_cannot_be_deleted() {
        let (_dir, mut store) = store();
        let kept = store.save_command(Path::new("/p"), new("a", "x")).unwrap();
        store.delete_saved_command(&kept.id).unwrap();
        assert!(store.saved_commands(None).unwrap().is_empty());
        assert_eq!(
            store.delete_saved_command(&kept.id).unwrap_err().code,
            Code::NotFound
        );
    }

    #[test]
    fn a_database_from_before_saved_commands_gains_the_table_and_keeps_its_resources() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        store
            .add_resource(tsumugi_model::NewResource {
                id: SessionId::new().to_string(),
                project: "/p".into(),
                target: "/etc/a.conf".into(),
                label: None,
                note: None,
            })
            .unwrap();
        drop(store);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE saved_commands; PRAGMA user_version = 12;")
            .unwrap();
        drop(connection);
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.resources(None).unwrap().len(), 1);
        store.save_command(Path::new("/p"), new("a", "x")).unwrap();
        assert_eq!(store.saved_commands(None).unwrap().len(), 1);
    }
}
