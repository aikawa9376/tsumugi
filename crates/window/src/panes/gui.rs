//! GUI attachments to local persistent file editors; remote and older editors
//! keep their terminal rendering when no local GUI socket is available.
use crate::bridge::AppCtx;
use crate::console::Hub;
use crate::content::Tab;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tsumugi_core::settings::TerminalRenderer;
use tsumugi_model::{TerminalProgram, TerminalResourceId};
use tsumugi_nvim::Gui;

pub(crate) type Guis = Rc<RefCell<HashMap<Tab, Arc<Gui>>>>;

/// Whether a file editor is drawn through its GUI rather than its terminal.
/// A remote one has no GUI socket here, and kitty draws the terminal itself.
fn draws_gui(renderer: TerminalRenderer, remote: bool) -> bool {
    !remote && renderer != TerminalRenderer::Kitty
}

fn resource(app: AppCtx, tab: &Tab) -> Option<TerminalResourceId> {
    let Tab::Terminal(environment, id) = tab else {
        return None;
    };
    let renderer = app.config.peek().settings.console.renderer;
    if !draws_gui(renderer, app.remote_status_of(environment).is_some()) {
        return None;
    }
    let hosts = app.hosts.peek();
    tsumugi_core::tree::terminals(hosts.work(environment))
        .iter()
        .find(|terminal| terminal.id == *id && terminal.program == TerminalProgram::Neovim)
        .map(|terminal| terminal.resource_id)
}

pub(crate) async fn follow(app: AppCtx, hub: Hub) {
    loop {
        // Keep the handle on the hub when its tab is hidden; only ending the
        // pane or the window detaches the GUI. The process stays in its host.
        hub.guis.borrow_mut().retain(|tab, gui| {
            !gui.ended()
                && resource(app, tab).is_some()
                && hub.panes.borrow().get(tab).is_some_and(|p| !p.exited())
        });
        let candidates: Vec<_> = hub
            .panes
            .borrow()
            .iter()
            .filter(|(_, pane)| !pane.exited())
            .filter_map(|(tab, pane)| {
                resource(app, tab).map(|resource| {
                    let screen = pane.screen();
                    (
                        tab.clone(),
                        resource,
                        screen.cols as u16,
                        screen.rows as u16,
                    )
                })
            })
            .collect();
        for (tab, resource, cols, rows) in candidates {
            let existing = hub.guis.borrow().get(&tab).cloned();
            if let Some(gui) = existing {
                let palette = hub.palette.peek().clone();
                let _ =
                    tokio::time::timeout(Duration::from_secs(1), gui.set_palette(&palette)).await;
                let _ = tokio::time::timeout(Duration::from_secs(2), gui.resize(cols, rows)).await;
                continue;
            }
            let socket = tsumugi_model::paths::neovim_socket(tsumugi_sys::euid(), resource);
            if !socket.exists() {
                continue;
            }
            let wake = hub.wake.clone();
            let woken = tab.clone();
            let gui = tokio::time::timeout(
                Duration::from_secs(2),
                Gui::connect(&socket, cols, rows, move || {
                    let _ = wake.send(woken.clone());
                }),
            )
            .await;
            match gui {
                Ok(Ok(gui)) => {
                    let palette = hub.palette.peek().clone();
                    let _ = tokio::time::timeout(Duration::from_secs(1), gui.set_palette(&palette))
                        .await;
                    hub.guis.borrow_mut().insert(tab.clone(), Arc::new(gui));
                    let _ = hub.wake.send(tab);
                }
                Ok(Err(error)) => tracing::debug!(%error, "file editor GUI not available"),
                Err(_) => tracing::debug!("file editor GUI connection timed out"),
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

impl Hub {
    pub(crate) fn gui(&self, tab: &Tab) -> Option<Arc<Gui>> {
        self.guis
            .borrow()
            .get(tab)
            .filter(|gui| !gui.ended())
            .cloned()
    }
}

impl crate::shell::Ui {
    pub(crate) fn active_gui(&self) -> Option<Arc<Gui>> {
        self.hub.gui(&self.app.active()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_file_editors_are_drawn_through_their_gui_unless_kitty_draws_consoles() {
        assert!(draws_gui(TerminalRenderer::Builtin, false));
        assert!(draws_gui(TerminalRenderer::Vte, false));
        assert!(!draws_gui(TerminalRenderer::Kitty, false));
        assert!(!draws_gui(TerminalRenderer::Builtin, true));
    }
}
