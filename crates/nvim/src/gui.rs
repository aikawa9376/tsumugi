//! A window's UI connection to a persistent, normally configured Neovim.
use crate::UiState;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::error::CallError;
use nvim_rs::{Handler, Neovim, UiAttachOptions};
use rmpv::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::WriteHalf;
use tokio::net::UnixStream;
use tokio::task::JoinHandle;

type Writer = Compat<WriteHalf<UnixStream>>;
type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone)]
struct Listener {
    state: Arc<Mutex<UiState>>,
    changed: Arc<dyn Fn() + Send + Sync>,
}
#[async_trait::async_trait]
impl Handler for Listener {
    type Writer = Writer;
    async fn handle_notify(&self, name: String, args: Vec<Value>, _: Neovim<Writer>) {
        if name != "redraw" {
            return;
        }
        let flushed = args.iter().any(|event| {
            event
                .as_array()
                .and_then(|e| e.first())
                .and_then(Value::as_str)
                == Some("flush")
        });
        self.state.lock().unwrap().apply(&args);
        if flushed {
            (self.changed)();
        }
    }
}

/// The GUI owns a connection, never the editor process or its configuration.
pub struct Gui {
    nvim: Neovim<Writer>,
    state: Arc<Mutex<UiState>>,
    io: JoinHandle<()>,
    connection: tokio::task::AbortHandle,
    size: Mutex<(u16, u16)>,
}
impl Drop for Gui {
    fn drop(&mut self) {
        // Detach this UI before closing RPC. The host owns the editor and
        // its unsaved buffers independently of this window.
        let connection = self.connection.clone();
        let watcher = self.io.abort_handle();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let nvim = self.nvim.clone();
            runtime.spawn(async move {
                let _ =
                    tokio::time::timeout(std::time::Duration::from_secs(1), nvim.ui_detach()).await;
                connection.abort();
                watcher.abort();
            });
        } else {
            connection.abort();
            watcher.abort();
        }
    }
}
impl Gui {
    pub async fn connect(
        socket: &Path,
        cols: u16,
        rows: u16,
        changed: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let state = Arc::new(Mutex::new(UiState::default()));
        let changed: Arc<dyn Fn() + Send + Sync> = Arc::new(changed);
        let listener = Listener {
            state: state.clone(),
            changed: changed.clone(),
        };
        let (nvim, io) = nvim_rs::create::tokio::new_path(socket, listener).await?;
        let connection = io.abort_handle();
        let completed = changed.clone();
        let io = tokio::spawn(async move {
            let _ = io.await;
            completed();
        });
        let gui = Self {
            nvim,
            state,
            io,
            connection,
            size: Mutex::new((cols, rows)),
        };
        let mut options = UiAttachOptions::new();
        // Keep command lines, messages and completion in the editor's grid.
        options.set_rgb(true).set_linegrid_external(true);
        gui.ask(move |n| async move { n.ui_attach(cols.into(), rows.into(), &options).await })
            .await?;
        Ok(gui)
    }
    async fn ask<T, F>(&self, request: impl FnOnce(Neovim<Writer>) -> F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, Box<CallError>>> + Send + 'static,
    {
        // A dropped view must not cancel a pending nvim-rs RPC response.
        Ok(tokio::spawn(request(self.nvim.clone())).await??)
    }
    pub fn screen(&self) -> tsumugi_term::Screen {
        self.state.lock().unwrap().screen()
    }
    pub async fn set_palette(&self, palette: &tsumugi_term::Palette) -> Result<(), Error> {
        // Neovim reports black/white UI defaults even when Normal has no RGB
        // background. Ask the highlight itself to preserve that transparency.
        let normal = self
            .ask(|nvim| async move {
                nvim.get_hl(
                    0,
                    vec![
                        ("name".into(), "Normal".into()),
                        ("link".into(), false.into()),
                    ],
                )
                .await
            })
            .await?;
        let colour = |name| {
            let rgb = normal
                .iter()
                .find(|(key, _)| key.as_str() == Some(name))?
                .1
                .as_u64()?;
            Some(tsumugi_term::Rgb::new(
                (rgb >> 16) as u8,
                (rgb >> 8) as u8,
                rgb as u8,
            ))
        };
        let mut state = self.state.lock().unwrap();
        state.set_palette(palette);
        state.normal_colours(colour("fg"), colour("bg"));
        Ok(())
    }
    pub fn ended(&self) -> bool {
        self.io.is_finished()
    }
    pub async fn input(&self, keys: &str) -> Result<(), Error> {
        let mut keys = keys.to_string();
        // nvim_input may consume only part of a large input buffer.
        while !keys.is_empty() {
            let part = keys.clone();
            let used = self
                .ask(move |n| async move { n.input(&part).await })
                .await?;
            if used == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            } else {
                keys.drain(..used as usize);
            }
        }
        Ok(())
    }
    pub async fn text(&self, text: &str, pasted: bool) -> Result<(), Error> {
        if pasted {
            let text = text.to_string();
            self.ask(move |n| async move { n.paste(&text, false, -1).await })
                .await?;
        } else {
            self.input(&text.replace('<', "<lt>")).await?;
        }
        Ok(())
    }
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
        if *self.size.lock().unwrap() == (cols, rows) {
            return Ok(());
        }
        self.ask(move |n| async move { n.ui_try_resize(cols.into(), rows.into()).await })
            .await?;
        *self.size.lock().unwrap() = (cols, rows);
        Ok(())
    }
    pub async fn click(&self, col: usize, row: usize) -> Result<(), Error> {
        self.ask(move |n| async move {
            n.input_mouse("left", "press", "", 0, row as i64, col as i64)
                .await?;
            n.input_mouse("left", "release", "", 0, row as i64, col as i64)
                .await
        })
        .await
    }
    pub async fn wheel(&self, lines: i32, col: usize, row: usize) -> Result<(), Error> {
        self.ask(move |n| async move {
            let direction = if lines > 0 { "up" } else { "down" };
            for _ in 0..lines.unsigned_abs().min(100) {
                n.input_mouse("wheel", direction, "", 0, row as i64, col as i64)
                    .await?;
            }
            Ok(())
        })
        .await
    }
}
