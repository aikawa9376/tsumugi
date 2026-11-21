//! The commands each project keeps to run again, told to every window as the
//! sessions are read and kept or deleted for a window that asks.
use tsumugi_model::NewSavedCommand;
use tsumugi_protocol::{Answer, TreeChange};

use crate::delegation::app::{root, with_store};
use crate::watch::Watched;

pub(crate) async fn publish(watched: &Watched) -> Result<(), String> {
    let commands = with_store(&root(), |store| store.saved_commands(None))
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::SavedCommands { commands }]);
    Ok(())
}

/// Keep `new` in the project holding its directory, which is the workspace
/// root of a worktree of its repository, so every worktree shares it.
pub(crate) async fn save(watched: &Watched, mut new: NewSavedCommand) -> Result<Answer, String> {
    new.dir = tsumugi_sys::fs::canonicalize(&new.dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Choose an existing directory on the work machine".to_owned())?;
    let project = crate::workspaces::root_of(&new.dir).await;
    with_store(&root(), move |store| {
        store.save_command(&project, new).map(drop)
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}

/// Keep the command of `id` no more.
pub(crate) async fn delete(watched: &Watched, id: String) -> Result<Answer, String> {
    with_store(&root(), move |store| store.delete_saved_command(&id))
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    Ok(Answer::Done)
}
