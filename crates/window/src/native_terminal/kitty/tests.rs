use super::*;

#[test]
fn page_banners_remain_visible_over_the_kitty_display() {
    let region = uncovered([640.0, 360.0], Some([2.0, 0.0, 636.0, 25.5]));
    assert!(!region.contains_point(20, 10));
    assert!(!region.contains_point(20, 25));
    assert!(region.contains_point(20, 26));
    assert!(region.contains_point(0, 10));
    assert!(uncovered([640.0, 360.0], None).contains_point(20, 10));
}

#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_x11_window_get_xid(window: *mut gtk::gdk::ffi::GdkWindow) -> c_ulong;
}

#[test]
fn invalid_or_unallocated_kitty_geometry_does_not_resize_the_managed_terminal() {
    for message in [
        "",
        "0 24 640 480",
        "80 0 640 480",
        "80 24 0 480",
        "80 24 640 0",
        "1001 24 640 480",
        "80 24 640",
        "80 24 640 480 junk",
    ] {
        assert!(geometry(message).is_none(), "{message}");
    }
    assert_eq!(
        geometry("80 24 640 480"),
        Some(Size {
            cols: 80,
            rows: 24,
            cell_width: 8.0,
            cell_height: 20.0
        })
    );
}

#[test]
fn a_slow_display_keeps_complete_frames_and_coalesces_later_updates() {
    let mut output = Output::default();
    output.replace("日本語 first".into());
    let mut received = Vec::new();
    let mut limited = Limited {
        received: &mut received,
        allowance: 2,
    };
    output.flush(&mut limited).unwrap();
    output.replace("discarded".into());
    output.replace("newest".into());
    output.flush(&mut received).unwrap();
    assert_eq!(String::from_utf8(received).unwrap(), "日本語 firstnewest");
}

#[test]
fn a_frame_is_written_as_it_is_sent_not_at_the_next_poll() {
    let mut output = Output::default();
    let mut received = Vec::new();
    output.send("first".into(), &mut received).unwrap();
    assert_eq!(received, b"first");
    let mut slow = Limited {
        received: &mut received,
        allowance: 0,
    };
    output.send("second".into(), &mut slow).unwrap();
    output.send("newest".into(), &mut slow).unwrap();
    output.flush(&mut received).unwrap();
    assert_eq!(String::from_utf8(received).unwrap(), "firstnewest");
}

struct Limited<'a> {
    received: &'a mut Vec<u8>,
    allowance: usize,
}
impl Write for Limited<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.allowance == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = data.len().min(self.allowance);
        self.received.extend_from_slice(&data[..n]);
        self.allowance -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn settle() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
}

fn wait_display(kitty: &mut Kitty, check: impl Fn(Size) -> bool) -> Size {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        settle();
        if let Some(size) = kitty.poll().unwrap()
            && check(size)
        {
            return size;
        }
        assert!(Instant::now() < deadline, "kitty did not become ready");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Whether the X server shows kitty's window, asked of it rather than GDK.
fn viewable(kitty: &Kitty) -> bool {
    let out = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name"])
        .arg(format!("^{}$", kitty.title.to_str().unwrap()))
        .output()
        .unwrap();
    !String::from_utf8_lossy(&out.stdout).trim().is_empty()
}

fn visible(kitty: &Kitty) -> String {
    control(kitty, &["get-text", "--extent", "screen"])
}

fn control(kitty: &Kitty, args: &[&str]) -> String {
    let out = Command::new("kitty")
        .args(["@", "--to"])
        .arg(format!(
            "unix:{}",
            kitty._directory.0.join("control.sock").display()
        ))
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[tokio::test]
#[ignore = "requires X11, kitty, xdotool and TSUMUGI_KITTY_TEST_PROGRAM pointing to the desktop binary"]
async fn kitty_draws_the_managed_terminal_without_taking_its_input_or_ending_its_program() {
    use std::cell::Cell;
    use tsumugi_term::{Daemon, Palette, Pane, Spawn};
    gtk::init().unwrap();
    let program =
        PathBuf::from(std::env::var_os("TSUMUGI_KITTY_TEST_PROGRAM").expect("desktop binary"));
    let dir = tempfile::Builder::new()
        .prefix("kitty-")
        .tempdir_in(tsumugi_sys::local::temp_dir())
        .unwrap();
    tsumugi_sys::fs::set_mode(dir.path(), 0o700).unwrap();
    let socket = dir.path().join("daemon.sock");
    let task = crate::native_test_daemon::serve_process(socket.clone());
    let daemon = Daemon::new(socket);
    tokio::time::timeout(Duration::from_secs(5), async {
        while daemon.sessions().await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let id = daemon
        .spawn(Spawn {
            program: "/bin/bash".into(),
            args: vec!["--noprofile".into(), "--norc".into(), "-i".into()],
            cwd: dir.path().into(),
            cols: 80,
            rows: 24,
            ..Spawn::default()
        })
        .await
        .unwrap();
    let pane = Pane::attach(&daemon, id, Size::new(80, 24), &Palette::default(), || {})
        .await
        .unwrap();
    daemon
        .press(
            id,
            "retained=kept; printf '\\nKITTY_MANAGED_OK 日本語 ╭──╮\\n'\r".as_bytes(),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !pane
            .screen()
            .text()
            .iter()
            .any(|l| l.starts_with("KITTY_MANAGED_OK"))
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Tsumugi kitty integration check");
    window.set_default_size(720, 480);
    let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let page = gtk::DrawingArea::new();
    page.set_can_focus(true);
    page.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK | gtk::gdk::EventMask::KEY_PRESS_MASK);
    let clicks = Rc::new(Cell::new(0));
    let keys = Rc::new(Cell::new(0));
    page.connect_button_press_event({
        let clicks = clicks.clone();
        move |_, _| {
            clicks.set(clicks.get() + 1);
            gtk::glib::Propagation::Proceed
        }
    });
    page.connect_key_press_event({
        let keys = keys.clone();
        move |_, _| {
            keys.set(keys.get() + 1);
            gtk::glib::Propagation::Proceed
        }
    });
    parent.pack_start(&page, true, true, 0);
    window.add(&parent);
    let layer = Rc::new(Layer::wrap(&page).unwrap());
    let mut kitty = Kitty::with_program(layer.clone(), &program, "", 15.0).unwrap();
    kitty.rect([0.0, 0.0, 640.0, 360.0]);
    window.show_all();
    page.grab_focus();
    let before = wait_display(&mut kitty, |_| true);
    pane.resize(before);
    kitty.feed(&pane.screen(), true);
    wait_display(&mut kitty, |_| true);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !visible(&kitty).contains("KITTY_MANAGED_OK 日本語 ╭──╮") {
        wait_display(&mut kitty, |_| true);
        assert!(
            Instant::now() < deadline,
            "kitty did not paint the managed screen"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // kitty started hidden withdraws its window again once it has settled,
    // which can come after the window was embedded and mapped.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        wait_display(&mut kitty, |_| true);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        viewable(&kitty),
        "the embedded kitty window must stay mapped"
    );
    let colors = control(&kitty, &["get-colors"]);
    let background = colors
        .lines()
        .find_map(|line| line.strip_prefix("background "))
        .unwrap()
        .trim();
    assert_eq!(
        background,
        pane.screen().background.hex(),
        "unused edge pixels must have the host screen's background"
    );
    assert!(!kitty.widget.can_focus());
    let (_, x, y) = page.window().unwrap().origin();
    page.grab_focus();
    assert_eq!(window.focused_widget(), Some(page.clone().upcast()));
    // Address the key to this test's toplevel so another window becoming
    // active on the user's desktop cannot redirect the check into that app.
    // SAFETY: this test runs on X11 and the realized window is live.
    let xid = unsafe { gdk_x11_window_get_xid(window.window().unwrap().to_glib_none().0) };
    let status = Command::new("xdotool")
        .args([
            "mousemove",
            &(x + 30).to_string(),
            &(y + 30).to_string(),
            "click",
            "1",
            "key",
            "--window",
            &xid.to_string(),
            "a",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(3);
    while (clicks.get() == 0 || keys.get() == 0) && Instant::now() < deadline {
        settle();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        clicks.get() > 0,
        "mouse input must pass through kitty into the page"
    );
    assert!(keys.get() > 0, "keyboard focus must remain in the page");
    kitty.widget.hide();
    assert!(daemon.sessions().await.unwrap().iter().any(|s| s.id == id));
    let display_pid = kitty.child.id();
    drop(kitty);
    // SAFETY: signal zero only checks that Drop reaped our display process.
    assert_eq!(unsafe { libc::kill(display_pid as i32, 0) }, -1);
    let mut kitty = Kitty::with_program(layer, &program, "", 15.0).unwrap();
    kitty.rect([0.0, 0.0, 320.0, 220.0]);
    kitty.widget.show();
    let after = wait_display(&mut kitty, |size| {
        size.cols < before.cols && size.rows < before.rows
    });
    pane.resize(after);
    daemon
        .press(id, b"printf '\\nREATTACH_%s\\n' \"$retained\"\r")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !pane
            .screen()
            .text()
            .iter()
            .any(|l| l.trim() == "REATTACH_kept")
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    kitty.feed(&pane.screen(), true);
    wait_display(&mut kitty, |_| true);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !visible(&kitty).contains("REATTACH_kept") {
        wait_display(&mut kitty, |_| true);
        assert!(
            Instant::now() < deadline,
            "recreated kitty must show the retained program"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(kitty);
    // SAFETY: owned widgets are destroyed on the GTK thread.
    unsafe {
        window.destroy();
    }
    daemon.end(id).await.unwrap();
    task.abort();
    println!(
        "KITTY_NATIVE_OK input=true pointer=true recreate=true retained=true resize={}x{}->{}x{}",
        before.cols, before.rows, after.cols, after.rows
    );
}

#[test]
fn kitty_draws_in_the_console_font_at_the_size_the_built_in_display_uses() {
    // kitty sizes fonts in points at Xft.dpi, 96 when unset.
    assert_eq!(
        font_options("Liga Plemol Console NF", 14.0, 1, 96.0),
        [
            "-o",
            "font_family=Liga Plemol Console NF",
            "-o",
            "font_size=10.5"
        ]
    );
    assert_eq!(
        font_options("", 16.0, 2, 192.0)[1],
        "font_family=JetBrainsMono Nerd Font Mono"
    );
    assert_eq!(font_options("", 16.0, 2, 192.0)[3], "font_size=12");
    assert_eq!(font_options("Mono", 14.0, 1, 120.0)[3], "font_size=8.4");
}

#[test]
fn xft_dpi_is_read_from_the_resource_database_as_kitty_reads_it() {
    assert_eq!(xft_dpi("Xft.antialias:\t1\nXft.dpi:\t144\n"), Some(144.0));
    assert_eq!(xft_dpi("Xft.dpi: 120.5"), Some(120.5));
    assert_eq!(xft_dpi("Xcursor.size:\t24\n"), None);
    assert_eq!(xft_dpi("Xft.dpi:\tlarge\n"), None);
    assert_eq!(xft_dpi("Xft.dpi:\t0\n"), None);
}
