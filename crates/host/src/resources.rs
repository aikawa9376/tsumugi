//! The resources each project keeps, told to every window as the sessions
//! are read, so what a session keeps through its tools shows without asking.
use tsumugi_protocol::TreeChange;

use crate::delegation::app::{root, with_store};
use crate::watch::Watched;

pub(crate) async fn publish(watched: &Watched) -> Result<(), String> {
    let resources = with_store(&root(), |store| store.resources(None))
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::Resources { resources }]);
    Ok(())
}
