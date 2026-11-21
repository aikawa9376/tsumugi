//! The paths and addresses each project keeps, beside the sessions that read them.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};
use tsumugi_model::{NewResource, Resource};

use super::{Code, Error, Result, Store};

impl Store {
    /// Keep `new` in its project. A target the project keeps already keeps
    /// its identity and takes the label and note given anew, leaving the ones
    /// not given as they were. What is kept is returned.
    pub fn add_resource(&mut self, new: NewResource) -> Result<Resource> {
        if let Some(refusal) = tsumugi_model::refuse(&new) {
            return Err(Error::new(Code::Invalid, refusal));
        }
        let target = new.target.trim().to_owned();
        let project = new.project.to_string_lossy().into_owned();
        let said =
            |text: Option<String>| text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
        let (label, note) = (said(new.label), said(new.note));
        self.connection.execute(
            "INSERT INTO resources (id, project, target, label, note) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project, target) DO UPDATE SET
                 label = COALESCE(excluded.label, label),
                 note = COALESCE(excluded.note, note)",
            params![new.id, project, target, label, note],
        )?;
        self.connection
            .query_row(
                "SELECT id, project, target, label, note FROM resources WHERE project = ?1 AND target = ?2",
                params![project, target],
                read,
            )
            .map_err(Into::into)
    }

    /// What `project` keeps, in the order it was first kept; what every
    /// project keeps when none is named.
    pub fn resources(&self, project: Option<&Path>) -> Result<Vec<Resource>> {
        let mut query = self.connection.prepare(
            "SELECT id, project, target, label, note FROM resources
             WHERE ?1 IS NULL OR project = ?1 ORDER BY seq",
        )?;
        let project = project.map(|p| p.to_string_lossy().into_owned());
        let rows = query.query_map([project], read)?;
        rows.collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    }

    /// Keep the resource of `id` no more.
    pub fn remove_resource(&mut self, id: &str) -> Result<()> {
        let removed = self
            .connection
            .execute("DELETE FROM resources WHERE id = ?1", [id])?;
        if removed != 1 {
            return Err(Error::new(Code::NotFound, "No resource has this id"));
        }
        Ok(())
    }

    /// The label and note of the resource of `id`, replaced as given.
    pub fn describe_resource(
        &mut self,
        id: &str,
        label: Option<String>,
        note: Option<String>,
    ) -> Result<Resource> {
        let said =
            |text: Option<String>| text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
        let (label, note) = (said(label), said(note));
        let kept = self
            .connection
            .query_row(
                "SELECT id, project, target, label, note FROM resources WHERE id = ?1",
                [id],
                read,
            )
            .optional()?
            .ok_or_else(|| Error::new(Code::NotFound, "No resource has this id"))?;
        let changed = Resource {
            label,
            note,
            ..kept
        };
        if let Some(refusal) = tsumugi_model::refuse(&NewResource {
            id: changed.id.clone(),
            project: changed.project.clone(),
            target: changed.target.clone(),
            label: changed.label.clone(),
            note: changed.note.clone(),
        }) {
            return Err(Error::new(Code::Invalid, refusal));
        }
        self.connection.execute(
            "UPDATE resources SET label = ?2, note = ?3 WHERE id = ?1",
            params![changed.id, changed.label, changed.note],
        )?;
        Ok(changed)
    }
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Resource> {
    Ok(Resource {
        id: row.get(0)?,
        project: PathBuf::from(row.get::<_, String>(1)?),
        target: row.get(2)?,
        label: row.get(3)?,
        note: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use tsumugi_model::NewResource;

    fn new(project: &str, target: &str) -> NewResource {
        NewResource {
            id: SessionId::new().to_string(),
            project: project.into(),
            target: target.into(),
            label: None,
            note: None,
        }
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("state.sqlite3")).unwrap();
        (dir, store)
    }

    #[test]
    fn a_project_lists_what_it_keeps_in_the_order_it_kept_it() {
        let (_dir, mut store) = store();
        store.add_resource(new("/p", "/etc/b.conf")).unwrap();
        store
            .add_resource(new("/p", "https://example.com"))
            .unwrap();
        store.add_resource(new("/q", "/etc/other")).unwrap();
        let targets = |project| -> Vec<String> {
            store
                .resources(project)
                .unwrap()
                .into_iter()
                .map(|r| r.target)
                .collect()
        };
        assert_eq!(
            targets(Some(Path::new("/p"))),
            ["/etc/b.conf", "https://example.com"]
        );
        assert_eq!(targets(None).len(), 3);
    }

    #[test]
    fn keeping_a_target_again_keeps_one_resource_and_only_what_was_given_anew() {
        let (_dir, mut store) = store();
        let mut first = new("/p", "/etc/a.conf");
        first.label = Some("Web server".into());
        first.note = Some("Proxy".into());
        let first = store.add_resource(first).unwrap();
        let mut again = new("/p", "/etc/a.conf");
        again.note = Some("Proxy and TLS".into());
        let again = store.add_resource(again).unwrap();
        assert_eq!(again.id, first.id);
        assert_eq!(again.label.as_deref(), Some("Web server"));
        assert_eq!(again.note.as_deref(), Some("Proxy and TLS"));
        assert_eq!(store.resources(None).unwrap().len(), 1);
    }

    #[test]
    fn two_projects_keep_the_same_target_apart() {
        let (_dir, mut store) = store();
        store.add_resource(new("/p", "/etc/a.conf")).unwrap();
        store.add_resource(new("/q", "/etc/a.conf")).unwrap();
        assert_eq!(store.resources(Some(Path::new("/q"))).unwrap().len(), 1);
        assert_eq!(store.resources(None).unwrap().len(), 2);
    }

    #[test]
    fn a_label_or_note_of_only_spaces_is_not_kept() {
        let (_dir, mut store) = store();
        let mut blank = new("/p", "/etc/a.conf");
        blank.label = Some("  ".into());
        blank.note = Some("\n".into());
        let kept = store.add_resource(blank).unwrap();
        assert_eq!((kept.label, kept.note), (None, None));
    }

    #[test]
    fn a_resource_that_holds_together_no_better_is_refused() {
        let (_dir, mut store) = store();
        let error = store.add_resource(new("/p", "relative.txt")).unwrap_err();
        assert_eq!(error.code, Code::Invalid);
        assert!(store.resources(None).unwrap().is_empty());
    }

    #[test]
    fn a_removed_resource_is_gone_and_one_never_kept_cannot_be_removed() {
        let (_dir, mut store) = store();
        let kept = store.add_resource(new("/p", "/etc/a.conf")).unwrap();
        store.remove_resource(&kept.id).unwrap();
        assert!(store.resources(None).unwrap().is_empty());
        assert_eq!(
            store.remove_resource(&kept.id).unwrap_err().code,
            Code::NotFound
        );
    }

    #[test]
    fn describing_a_resource_replaces_its_label_and_note_and_nothing_else() {
        let (_dir, mut store) = store();
        let mut first = new("/p", "/etc/a.conf");
        first.label = Some("Old".into());
        first.note = Some("Old note".into());
        let kept = store.add_resource(first).unwrap();
        let changed = store
            .describe_resource(&kept.id, Some("New".into()), None)
            .unwrap();
        assert_eq!(changed.label.as_deref(), Some("New"));
        assert_eq!(changed.note, None);
        assert_eq!(changed.target, "/etc/a.conf");
        assert_eq!(store.resources(None).unwrap(), [changed]);
    }

    #[test]
    fn a_database_from_before_resources_gains_the_table_and_keeps_its_issues() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.sqlite3");
        let mut store = Store::open(&path).unwrap();
        store
            .add_local_issue(tsumugi_model::NewLocalIssue {
                id: SessionId::new().to_string(),
                dir: std::env::temp_dir(),
                title: "Kept".into(),
                body: "body".into(),
            })
            .unwrap();
        drop(store);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("DROP TABLE resources; PRAGMA user_version = 11;")
            .unwrap();
        drop(connection);
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.local_issues().unwrap().len(), 1);
        store.add_resource(new("/p", "/etc/a.conf")).unwrap();
        assert_eq!(store.resources(None).unwrap().len(), 1);
    }
}
