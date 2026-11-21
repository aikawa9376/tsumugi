//! The resources each project keeps, told to every window as the sessions
//! are read, so what a session keeps through its tools shows without asking,
//! and what a window asks to keep, change or drop.
use std::path::Path;

use tsumugi_model::{NewResource, Reach, SessionId};
use tsumugi_protocol::{Answer, TreeChange};

use crate::delegation::app::{root, with_store};
use crate::watch::Watched;

pub(crate) async fn publish(watched: &Watched) -> Result<(), String> {
    let resources = with_store(&root(), |store| store.resources(None))
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::Resources { resources }]);
    Ok(())
}

/// Keep `target` in the project holding `dir`.
pub(crate) async fn keep(
    watched: &Watched,
    dir: &Path,
    target: &str,
    label: Option<String>,
    note: Option<String>,
) -> Result<Answer, String> {
    let new = new_resource(dir, target, label, note).await?;
    with_store(&root(), move |store| store.add_resource(new))
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// Keep the PNG `data` as a file of its own and as a resource of the
/// project holding `dir`. The file is the user's to keep: nothing prunes it.
pub(crate) async fn keep_screenshot(
    watched: &Watched,
    dir: &Path,
    data: &str,
    label: Option<String>,
    note: Option<String>,
) -> Result<Answer, String> {
    let into = crate::paths::data_dir().join("screenshots");
    let saved = {
        let data = data.to_owned();
        crate::environment::spawn_blocking(move || save_screenshot(&into, &data))
            .await
            .map_err(|e| e.to_string())??
    };
    keep(watched, dir, &saved.to_string_lossy(), label, note).await
}

/// Write the base64 PNG `data` as a new file in `into`.
fn save_screenshot(into: &Path, data: &str) -> Result<std::path::PathBuf, String> {
    crate::pasted::save(into, "image/png", data, std::time::SystemTime::now())
        .map_err(|e| e.to_string())
}

/// Replace the label and the note of the resource of `id`.
pub(crate) async fn describe(
    watched: &Watched,
    id: String,
    label: Option<String>,
    note: Option<String>,
) -> Result<Answer, String> {
    with_store(&root(), move |store| {
        store.describe_resource(&id, label, note)
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// Keep the resource of `id` no more.
pub(crate) async fn drop(watched: &Watched, id: String) -> Result<Answer, String> {
    with_store(&root(), move |store| store.remove_resource(&id))
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// What keeping `target` in the project holding `dir` keeps: a relative path
/// is taken from `dir`, an address as it is.
async fn new_resource(
    dir: &Path,
    target: &str,
    label: Option<String>,
    note: Option<String>,
) -> Result<NewResource, String> {
    let dir = tsumugi_sys::fs::canonicalize(dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Choose an existing directory on the work machine".to_owned())?;
    let target = target.trim();
    let target = match Reach::of(target) {
        Reach::Path if !Path::new(target).is_absolute() && !target.is_empty() => {
            dir.join(target).to_string_lossy().into_owned()
        }
        _ => target.to_owned(),
    };
    Ok(NewResource {
        id: SessionId::new().to_string(),
        project: crate::workspaces::root_of(&dir).await,
        target,
        label,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_relative_path_is_taken_from_the_directory_it_is_kept_from() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = tsumugi_sys::fs::canonicalize(dir.path()).unwrap();

        let new = new_resource(dir.path(), " docs/a.md ", Some("A".into()), None)
            .await
            .unwrap();

        assert_eq!(
            new.target,
            canonical.join("docs/a.md").to_string_lossy().into_owned()
        );
        assert_eq!(new.project, canonical);
        assert_eq!(new.label.as_deref(), Some("A"));
    }

    #[tokio::test]
    async fn an_address_and_an_absolute_path_are_kept_as_they_are() {
        let dir = tempfile::tempdir().unwrap();

        for target in ["https://example.com/a?b=c", "/etc/hosts"] {
            let new = new_resource(dir.path(), target, None, None).await.unwrap();
            assert_eq!(new.target, target);
        }
    }

    #[test]
    fn a_screenshot_is_written_as_a_png_file_of_its_own_each_time() {
        use base64::Engine;
        let dir = tempfile::tempdir().unwrap();
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        let data = base64::engine::general_purpose::STANDARD.encode(png);

        let first = save_screenshot(dir.path(), &data).unwrap();
        let second = save_screenshot(dir.path(), &data).unwrap();

        assert_ne!(first, second);
        assert_eq!(first.extension().and_then(|e| e.to_str()), Some("png"));
        assert_eq!(std::fs::read(&first).unwrap(), png);
    }

    #[test]
    fn what_is_not_base64_is_not_kept_as_a_screenshot() {
        let dir = tempfile::tempdir().unwrap();

        assert!(save_screenshot(dir.path(), "not base64 !!").is_err());
    }

    #[tokio::test]
    async fn nothing_is_kept_from_a_directory_that_is_not_there() {
        let dir = tempfile::tempdir().unwrap();

        let refused = new_resource(&dir.path().join("gone"), "/etc/hosts", None, None).await;

        assert!(refused.is_err());
    }
}
