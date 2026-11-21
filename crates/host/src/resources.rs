//! The resources each project keeps, told to every window as the sessions
//! are read, so what a session keeps through its tools shows without asking,
//! and kept or changed for a window that asks.
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
pub(crate) async fn pin(
    watched: &Watched,
    dir: &Path,
    target: &str,
    label: Option<String>,
) -> Result<Answer, String> {
    let dir = tsumugi_sys::fs::canonicalize(dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Choose an existing directory on the work machine".to_owned())?;
    let new = NewResource {
        id: SessionId::new().to_string(),
        project: crate::workspaces::root_of(&dir).await,
        target: target_in(&dir, target),
        label,
        note: None,
    };
    with_store(&root(), move |store| store.add_resource(new).map(drop))
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// Keep the resource of `id` no more.
pub(crate) async fn unpin(watched: &Watched, id: String) -> Result<Answer, String> {
    with_store(&root(), move |store| store.remove_resource(&id))
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// Replace the label and note of the resource of `id`.
pub(crate) async fn describe(
    watched: &Watched,
    id: String,
    label: Option<String>,
    note: Option<String>,
) -> Result<Answer, String> {
    with_store(&root(), move |store| {
        store.describe_resource(&id, label, note).map(drop)
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// `target` as the resource it is kept as: an address as typed, and a
/// relative path as one of `dir`.
fn target_in(dir: &Path, target: &str) -> String {
    let target = target.trim();
    match Reach::of(target) {
        Reach::Path if !Path::new(target).is_absolute() => {
            dir.join(target).to_string_lossy().into_owned()
        }
        _ => target.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_is_taken_from_the_directory_and_an_address_or_absolute_path_is_kept_as_typed()
     {
        let dir = Path::new("/w");
        assert_eq!(target_in(dir, " conf/a.toml "), "/w/conf/a.toml");
        assert_eq!(target_in(dir, "/etc/a.conf"), "/etc/a.conf");
        assert_eq!(target_in(dir, "https://a.dev/x"), "https://a.dev/x");
    }
}
