//! A process's read-only Linux input-context feed, independent of instances.

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tsumugi_core::console::InputTarget;

fn runtime_root(xdg: Option<OsString>, uid: u32) -> PathBuf {
    xdg.filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from(format!("/run/user/{uid}")), PathBuf::from)
}

pub(crate) fn page_target(target: InputTarget, window: bool, routing_field: bool) -> InputTarget {
    if window && routing_field {
        target
    } else {
        InputTarget::Gui
    }
}

#[derive(Clone)]
pub(crate) struct Publisher(Arc<Mutex<Published>>);

struct Published {
    target: InputTarget,
    suspended: usize,
    changes: broadcast::Sender<InputTarget>,
}

impl Published {
    fn target(&self) -> InputTarget {
        if self.suspended > 0 {
            InputTarget::Gui
        } else {
            self.target
        }
    }

    fn changed(&self, before: InputTarget) {
        if before != self.target() {
            let _ = self.changes.send(self.target());
        }
    }
}

impl Publisher {
    pub(crate) fn new() -> Self {
        let (changes, _) = broadcast::channel(64);
        Self(Arc::new(Mutex::new(Published {
            target: InputTarget::Gui,
            suspended: 0,
            changes,
        })))
    }

    pub(crate) fn update(&self, target: InputTarget) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let before = state.target();
        state.target = target;
        state.changed(before);
    }

    fn subscribe(&self) -> (InputTarget, broadcast::Receiver<InputTarget>) {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (state.target(), state.changes.subscribe())
    }

    pub(crate) fn suspend(&self) -> Suspended {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let before = state.target();
        state.suspended += 1;
        state.changed(before);
        Suspended(self.clone())
    }
}

pub(crate) struct Suspended(Publisher);

impl Drop for Suspended {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().unwrap_or_else(|e| e.into_inner());
        let before = state.target();
        state.suspended -= 1;
        state.changed(before);
    }
}

struct Endpoint {
    path: PathBuf,
    pid: u32,
    listener: std::os::unix::net::UnixListener,
}

impl Endpoint {
    fn bind(runtime: &Path, pid: u32) -> io::Result<Self> {
        tsumugi_sys::fs::check_private_dir(runtime)?;
        let directory = runtime.join("input-context");
        tsumugi_sys::fs::private_dir(&directory)?;
        let path = directory.join(format!("{pid}.sock"));
        if let Ok(meta) = std::fs::symlink_metadata(&path)
            && meta.file_type().is_socket()
            && let Err(error) = std::os::unix::net::UnixStream::connect(&path)
            && error.kind() == io::ErrorKind::ConnectionRefused
        {
            std::fs::remove_file(&path)?;
        }
        let listener = std::os::unix::net::UnixListener::bind(&path)?;
        let endpoint = Self {
            path,
            pid,
            listener,
        };
        tsumugi_sys::fs::set_mode(&endpoint.path, 0o600)?;
        endpoint.listener.set_nonblocking(true)?;
        Ok(endpoint)
    }

    async fn serve(self, publisher: Publisher) -> io::Result<()> {
        // Keep the path's guard with the listener, including while cancelled.
        let listener = tokio::net::UnixListener::from_std(self.listener.try_clone()?)?;
        let mut clients = JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let (target, changes) = publisher.subscribe();
                    clients.spawn(subscriber(stream, self.pid, target, changes));
                }
                _ = clients.join_next(), if !clients.is_empty() => {}
            }
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn subscriber(
    mut stream: UnixStream,
    pid: u32,
    mut target: InputTarget,
    mut changes: broadcast::Receiver<InputTarget>,
) -> io::Result<()> {
    #[derive(serde::Serialize)]
    struct Snapshot {
        version: u8,
        pid: u32,
        target: &'static str,
    }
    loop {
        let mut line = serde_json::to_vec(&Snapshot {
            version: 1,
            pid,
            target: match target {
                InputTarget::Gui => "gui",
                InputTarget::Neovim => "editor",
                InputTarget::Terminal => "terminal",
            },
        })?;
        line.push(b'\n');
        tokio::time::timeout(Duration::from_secs(1), stream.write_all(&line)).await??;
        let mut input = [0];
        tokio::select! {
            // This endpoint takes no requests. EOF also releases an idle client.
            _ = stream.read(&mut input) => return Ok(()),
            changed = changes.recv() => match changed {
                Ok(next) => target = next,
                Err(_) => return Ok(()),
            }
        }
    }
}

/// Start without filesystem operations or socket I/O on the UI thread. The
/// scoped task owns the endpoint and all of its clients until the window ends.
async fn serve(publisher: Publisher, path: Arc<Mutex<Option<PathBuf>>>) {
    let runtime = runtime_root(std::env::var_os("XDG_RUNTIME_DIR"), tsumugi_sys::uid());
    let endpoint =
        tokio::task::spawn_blocking(move || Endpoint::bind(&runtime, std::process::id())).await;
    let result = match endpoint {
        Ok(Ok(endpoint)) => {
            *path.lock().unwrap_or_else(|e| e.into_inner()) = Some(endpoint.path.clone());
            endpoint.serve(publisher).await
        }
        Ok(Err(error)) => Err(error),
        Err(error) => Err(io::Error::other(error)),
    };
    if let Err(error) = result {
        tracing::warn!(%error, "cannot publish input context");
    }
}

struct Running {
    task: tokio::task::JoinHandle<()>,
    path: Arc<Mutex<Option<PathBuf>>>,
}

impl Running {
    fn stop(&self) {
        self.task.abort();
        if let Some(path) = self.path.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn use_input_context(ui: &crate::shell::Ui, focused: Signal<bool>) {
    let publisher = use_context_provider(Publisher::new);
    let running = use_hook({
        let publisher = publisher.clone();
        move || {
            let path = Arc::new(Mutex::new(None));
            let task = tokio::spawn(serve(publisher, path.clone()));
            std::rc::Rc::new(Running { task, path })
        }
    });
    // Tao may terminate the process without dropping the virtual DOM.
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if matches!(event, dioxus::desktop::tao::event::Event::LoopDestroyed) {
            running.stop();
        }
    });
    use_effect({
        let ui = ui.clone();
        move || {
            let _ = (
                ui.local.frame.read(),
                ui.local.attached.read(),
                ui.consoles.nvim.read(),
                ui.consoles.reader.read(),
            );
            let target = ui.app.input_target(&ui.probe());
            publisher.update(page_target(target, ui.app.chrome.read().focused, focused()));
        }
    });
}

#[cfg(test)]
mod tests;
