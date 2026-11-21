//! Display scale belongs to each window, separately from user zoom settings.
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use dioxus::desktop::{DesktopContext, window};
use dioxus::prelude::*;
use tsumugi_core::zoom;

#[derive(Clone)]
struct Applied(Rc<Cell<Option<f64>>>);

/// The rendered zoom for native UI anchored to coordinates in the page.
pub(crate) fn current() -> f64 {
    try_consume_context::<Applied>()
        .and_then(|scale| scale.0.get())
        .unwrap_or(1.0)
}

/// Effective webview zoom on this window's current display, not its size.
pub(crate) fn for_window(desktop: &DesktopContext, user: f64, automatic: bool) -> f64 {
    let Some(monitor) = desktop
        .current_monitor()
        .or_else(|| desktop.primary_monitor())
    else {
        return user;
    };
    let size = monitor.size();
    zoom::for_display(
        user,
        automatic,
        size.width,
        size.height,
        monitor.scale_factor(),
    )
}

fn apply(desktop: &DesktopContext, user: f64, automatic: bool, applied: &Applied) {
    let effective = for_window(desktop, user, automatic);
    if applied.0.get() == Some(effective) {
        return;
    }
    match desktop.webview.zoom(effective) {
        Ok(()) => {
            applied.0.set(Some(effective));
            tracing::debug!(user, automatic, effective, "display scaling applied");
        }
        Err(error) => tracing::warn!(%error, "could not apply display scaling"),
    }
}

/// Watch preferences immediately and recheck the current monitor periodically.
/// Wayland need not report moves between displays sharing the same OS scale.
pub(crate) fn use_scaling(user: Memo<f64>, automatic: Memo<bool>) {
    let applied = use_context_provider(|| Applied(Rc::new(Cell::new(None))));
    use_effect({
        let applied = applied.clone();
        move || apply(&window(), user(), automatic(), &applied)
    });
    use_hook(move || {
        let desktop = window();
        spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(750)).await;
                apply(&desktop, *user.peek(), *automatic.peek(), &applied);
            }
        });
    });
}
