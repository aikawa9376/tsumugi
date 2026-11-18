//! A GUI attaches to the user's persistent editor without replacing its config.
#![cfg(unix)]
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tsumugi_nvim::Gui;

struct Editor(Child);
impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn connect(socket: &std::path::Path) -> Gui {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match Gui::connect(socket, 80, 20, || {}).await {
                Ok(gui) => return gui,
                Err(error) if socket.exists() => eprintln!("GUI attach: {error}"),
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("editor did not accept a GUI")
}
async fn drawn(gui: &Gui, text: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if gui.screen().text().iter().any(|line| line.contains(text)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing {text:?}: {:?}", gui.screen().text()));
}
#[tokio::test]
async fn a_gui_keeps_user_configuration_unicode_edits_and_unsaved_work_across_reattachment() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("editor.sock");
    let config = root.path().join("config/nvim");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("init.lua"),
        r#"
vim.opt.swapfile = false
vim.opt.shadafile = 'NONE'
vim.opt.laststatus = 0
vim.api.nvim_set_hl(0, 'Normal', { fg = '#839496', bg = 'NONE' })
vim.keymap.set('n', '<F6>', function()
  vim.api.nvim_buf_set_lines(0, 0, -1, false, {'usual-config-loaded'})
end)
"#,
    )
    .unwrap();
    let file = root.path().join("日本語 file ; $(literal).txt");
    std::fs::write(&file, "original\n").unwrap();
    let child = Command::new(tsumugi_nvim::program())
        .args(["--headless", "--listen"])
        .arg(&socket)
        .arg("--")
        .arg(&file)
        .env_remove("NVIM_APPNAME")
        .env_remove("VIM")
        .env_remove("VIMRUNTIME")
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_STATE_HOME", root.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut editor = Editor(child);
    let gui = connect(&socket).await;
    let solarized = tsumugi_term::Palette {
        background: tsumugi_term::Rgb::new(0x00, 0x2b, 0x36),
        ..tsumugi_term::Palette::default()
    };
    gui.set_palette(&solarized).await.unwrap();
    drawn(&gui, "original").await;
    assert_eq!(gui.screen().background, solarized.background);
    gui.input("<F6>").await.unwrap();
    drawn(&gui, "usual-config-loaded").await;
    gui.input("gg0i日本語<lt>GUI<Esc>").await.unwrap();
    drawn(&gui, "日本語<GUIusual-config-loaded").await;
    gui.resize(96, 24).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while gui.screen().cols != 96 || gui.screen().rows != 24 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    gui.input(":qa<CR>").await.unwrap();
    drawn(&gui, "E37").await;
    // Acknowledge the error's hit-enter prompt before hiding the GUI. If
    // a UI is attached during a blocking prompt, Neovim defers that RPC;
    // the app keeps its terminal visible until the prompt is answered.
    gui.input("<CR>").await.unwrap();
    drawn(&gui, "日本語<GUIusual-config-loaded").await;
    drop(gui);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        editor.0.try_wait().unwrap().is_none(),
        "editor exited after GUI detach"
    );
    let gui = connect(&socket).await;
    drawn(&gui, "日本語<GUIusual-config-loaded").await;
    gui.input("<Esc>:w<CR>").await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::fs::read_to_string(&file).unwrap() != "日本語<GUIusual-config-loaded\n" {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    gui.input(":qa<CR>").await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while editor.0.try_wait().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn a_missing_gui_socket_is_an_error_so_the_terminal_can_remain_visible() {
    let root = tempfile::tempdir().unwrap();
    assert!(
        Gui::connect(&root.path().join("missing.sock"), 80, 24, || {})
            .await
            .is_err()
    );
}
