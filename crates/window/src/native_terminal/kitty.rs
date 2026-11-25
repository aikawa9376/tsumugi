//! A private kitty process presents the host's screen without owning its input.
use crate::native_layer::Layer;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_ulong, c_void};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};
use tsumugi_term::Size;

#[link(name = "gdk-3")]
unsafe extern "C" {
    fn gdk_x11_display_get_xdisplay(display: *mut gtk::gdk::ffi::GdkDisplay) -> *mut c_void;
    fn gdk_x11_display_error_trap_push(display: *mut gtk::gdk::ffi::GdkDisplay);
    fn gdk_x11_display_error_trap_pop_ignored(display: *mut gtk::gdk::ffi::GdkDisplay);
}
#[link(name = "X11")]
unsafe extern "C" {
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XQueryTree(
        display: *mut c_void,
        window: c_ulong,
        root: *mut c_ulong,
        parent: *mut c_ulong,
        children: *mut *mut c_ulong,
        count: *mut c_uint,
    ) -> c_int;
    fn XFetchName(display: *mut c_void, window: c_ulong, name: *mut *mut c_char) -> c_int;
    fn XFree(data: *mut c_void) -> c_int;
    fn XMapWindow(display: *mut c_void, window: c_ulong) -> c_int;
    fn XSync(display: *mut c_void, discard: c_int) -> c_int;
}

pub(crate) fn available() -> Result<(), String> {
    let display = gtk::gdk::Display::default().ok_or("No desktop display")?;
    if display.type_().name() != "GdkX11Display" {
        return Err("kitty needs an X11/XWayland Tsumugi window (GDK_BACKEND=x11); using the built-in display".into());
    }
    let env: Vec<_> = std::env::vars().collect();
    if !tsumugi_sys::path::runnable("kitty", Path::new("/"), &env) {
        return Err("kitty is not installed; using the built-in display".into());
    }
    Ok(())
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> io::Result<Self> {
        let mut random = [0; 16];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let name: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path = PathBuf::from(format!("/tmp/tsumugi-k-{name}"));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        let directory = Self(path);
        // Host socket startup can briefly narrow this process's umask.
        tsumugi_sys::fs::set_mode(&directory.0, 0o700)?;
        Ok(directory)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        for file in ["screen.sock", "control.sock"] {
            let _ = std::fs::remove_file(self.0.join(file));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[derive(Default)]
struct Output {
    current: Vec<u8>,
    sent: usize,
    latest: Option<String>,
}
impl Output {
    fn replace(&mut self, frame: String) {
        if self.sent == 0 {
            self.current = frame.into_bytes();
        } else {
            self.latest = Some(frame);
        }
    }
    /// `frame` written as far as `writer` takes it now: a frame that waited
    /// for the next poll would show a key's echo a poll late.
    fn send(&mut self, frame: String, writer: &mut impl Write) -> io::Result<()> {
        self.replace(frame);
        self.flush(writer)
    }
    fn flush(&mut self, writer: &mut impl Write) -> io::Result<()> {
        loop {
            while self.sent < self.current.len() {
                match writer.write(&self.current[self.sent..]) {
                    Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                    Ok(n) => self.sent += n,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
            self.sent = 0;
            self.current.clear();
            match self.latest.take() {
                Some(frame) => self.current = frame.into_bytes(),
                None => return Ok(()),
            }
        }
    }
}

fn geometry(message: &str) -> Option<Size> {
    let values: Vec<u16> = message
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [cols, rows, width, height] = values.as_slice() else {
        return None;
    };
    if !(1..=1000).contains(cols) || !(1..=1000).contains(rows) || width < cols || height < rows {
        return None;
    }
    Some(Size {
        cols: *cols,
        rows: *rows,
        cell_width: f64::from(*width) / f64::from(*cols),
        cell_height: f64::from(*height) / f64::from(*rows),
    })
}

fn uncovered([width, height]: [f64; 2], cover: Option<[f64; 4]>) -> gtk::cairo::Region {
    let region = gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(
        0,
        0,
        width.ceil() as i32,
        height.ceil() as i32,
    ));
    if let Some([x, y, w, h]) = cover {
        let (left, top) = (x.floor() as i32, y.floor() as i32);
        region
            .subtract_rectangle(&gtk::cairo::RectangleInt::new(
                left,
                top,
                (x + w).ceil() as i32 - left,
                (y + h).ceil() as i32 - top,
            ))
            .expect("valid console cover");
    }
    region
}

pub(super) struct Kitty {
    pub(super) widget: gtk::Socket,
    child: Child,
    listener: UnixListener,
    stream: Option<UnixStream>,
    output: Output,
    incoming: Vec<u8>,
    size: Option<Size>,
    attached: bool,
    started: Instant,
    title: CString,
    _directory: Directory,
    _layer: Rc<Layer>,
}
impl Kitty {
    pub(super) fn new(layer: Rc<Layer>) -> Result<Self, String> {
        Self::with_program(layer, &std::env::current_exe().map_err(|e| e.to_string())?)
    }
    fn with_program(layer: Rc<Layer>, program: &Path) -> Result<Self, String> {
        available()?;
        let directory = Directory::new().map_err(|e| e.to_string())?;
        let socket = directory.0.join("screen.sock");
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let title = CString::new(directory.0.to_string_lossy().as_bytes()).unwrap();
        let widget = gtk::Socket::new();
        widget.set_can_focus(false);
        widget.connect_plug_removed(|_| true);
        layer.fixed.put(&widget, 0, 0);
        let child = Command::new("kitty")
            .args([
                "--class",
                "tsumugi-kitty",
                "--title",
                title.to_str().unwrap(),
                "--start-as=hidden",
                "--session",
                "none",
                "--listen-on",
            ])
            .arg(format!(
                "unix:{}",
                directory.0.join("control.sock").display()
            ))
            .args([
                "-o",
                "allow_remote_control=socket-only",
                "-o",
                "linux_display_server=x11",
                "-o",
                "hide_window_decorations=yes",
                "-o",
                "window_padding_width=0",
                "-o",
                "window_margin_width=0",
                "-o",
                "placement_strategy=top-left",
                "-o",
                "window_border_width=0",
                "-o",
                "tab_bar_style=hidden",
                "-o",
                "background_opacity=1",
                "-o",
                "shell_integration=disabled",
                "-o",
                "confirm_os_window_close=0",
            ])
            .args(["-o", "cursor_shape_unfocused=unchanged"])
            // kitty batches its program's output for 3 ms and repaints at
            // most every 10 ms; a frame here comes whole and only when the
            // screen changed, so the waits only hold a key's echo back.
            .args(["-o", "input_delay=0", "-o", "repaint_delay=2"])
            .arg(program)
            .arg("__kitty_display")
            .arg(&socket)
            .env_remove("KITTY_LISTEN_ON")
            .env_remove("KITTY_WINDOW_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let child = match child {
            Ok(child) => child,
            Err(e) => {
                // SAFETY: the widget belongs to this GTK thread.
                unsafe { widget.destroy() };
                return Err(format!("Cannot start kitty: {e}"));
            }
        };
        Ok(Self {
            widget,
            child,
            listener,
            stream: None,
            output: Output::default(),
            incoming: Vec::new(),
            size: None,
            attached: false,
            started: Instant::now(),
            title,
            _directory: directory,
            _layer: layer,
        })
    }
    pub(super) fn rect(&self, [x, y, w, h]: [f64; 4]) {
        self._layer
            .fixed
            .move_(&self.widget, x.round() as i32, y.round() as i32);
        self.widget
            .set_size_request(w.floor().max(1.0) as i32, h.floor().max(1.0) as i32);
    }
    pub(super) fn cover(&self, size: [f64; 2], cover: Option<[f64; 4]>) {
        if let Some(window) = self.widget.window() {
            // A native child otherwise covers page content regardless of its
            // CSS z-index, including the banner that explains direct input.
            window.shape_combine_region(Some(&uncovered(size, cover)), 0, 0);
        }
    }
    pub(super) fn feed(&mut self, screen: &tsumugi_term::Screen, focused: bool) {
        // Atomic painting avoids showing cleared rows between complete frames.
        let frame = super::frame(screen, focused);
        let frame = format!(
            "\x1b[?2026h\x1b]11;{}\x1b\\{frame}\x1b[?2026l",
            screen.background.hex()
        );
        match &mut self.stream {
            // A failure shows again as `poll` writes, which ends the display.
            Some(stream) => {
                let _ = self.output.send(frame, stream);
            }
            None => self.output.replace(frame),
        }
    }
    pub(super) fn poll(&mut self) -> Result<Option<Size>, String> {
        if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("kitty display exited; using the built-in display".into());
        }
        if !self.attached && self.widget.is_realized() {
            self.attach();
        }
        if self.stream.is_none() {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
                    self.stream = Some(stream);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        if let Some(stream) = &mut self.stream {
            let mut bytes = [0; 256];
            loop {
                match stream.read(&mut bytes) {
                    Ok(0) => return Err("kitty display disconnected".into()),
                    Ok(n) => self.incoming.extend_from_slice(&bytes[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.to_string()),
                }
                while let Some(end) = self.incoming.iter().position(|b| *b == b'\n') {
                    if let Ok(line) = std::str::from_utf8(&self.incoming[..end])
                        && let Some(size) = geometry(line)
                    {
                        self.size = Some(size);
                    }
                    self.incoming.drain(..=end);
                }
                if self.incoming.len() > 256 {
                    return Err("Invalid kitty display geometry".into());
                }
            }
            self.output.flush(stream).map_err(|e| e.to_string())?;
        }
        if (!self.attached || self.size.is_none())
            && self.started.elapsed() > Duration::from_secs(10)
        {
            return Err(
                "kitty did not attach to the terminal tab; using the built-in display".into(),
            );
        }
        let scale = f64::from(self.widget.scale_factor().max(1));
        Ok(self.size.filter(|_| self.attached).map(|size| Size {
            cell_width: size.cell_width / scale,
            cell_height: size.cell_height / scale,
            ..size
        }))
    }
    fn attach(&mut self) {
        let display = self.widget.display();
        // SAFETY: available() established an X11 display. Root children may
        // disappear during enumeration, so GDK traps all requests in the scan.
        unsafe {
            let gdk = display.to_glib_none().0;
            let x = gdk_x11_display_get_xdisplay(gdk);
            gdk_x11_display_error_trap_push(gdk);
            let (mut root, mut parent, mut children, mut count) = (0, 0, std::ptr::null_mut(), 0);
            let mut found = None;
            if XQueryTree(
                x,
                XDefaultRootWindow(x),
                &mut root,
                &mut parent,
                &mut children,
                &mut count,
            ) != 0
                && !children.is_null()
            {
                for window in std::slice::from_raw_parts(children, count as usize) {
                    let mut name = std::ptr::null_mut();
                    if XFetchName(x, *window, &mut name) != 0 && !name.is_null() {
                        if CStr::from_ptr(name) == self.title.as_c_str() {
                            found = Some(*window);
                        }
                        XFree(name.cast());
                    }
                }
            }
            if !children.is_null() {
                XFree(children.cast());
            }
            XSync(x, 0);
            gdk_x11_display_error_trap_pop_ignored(gdk);
            if let Some(window) = found {
                gdk_x11_display_error_trap_push(gdk);
                self.widget.add_id(window as gtk::xlib::Window);
                XMapWindow(x, window);
                XSync(x, 0);
                gdk_x11_display_error_trap_pop_ignored(gdk);
                if let Some(window) = self.widget.window() {
                    // Excluding the socket's entire input region also excludes
                    // its child: keys, IME and hit testing stay in the WebView.
                    window.input_shape_combine_region(&gtk::cairo::Region::create(), 0, 0);
                }
                self.attached = true;
                self.output.replace("\x1b[?1049h\x1b[?7l".into());
                tracing::debug!(pid = self.child.id(), "kitty terminal display attached");
            }
        }
    }
}
impl Drop for Kitty {
    fn drop(&mut self) {
        self.stream.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // SAFETY: the widget belongs to this GTK thread; this only ends kitty's
        // display helper, never the host's managed program.
        unsafe { self.widget.destroy() };
    }
}

/// Internal display helper, dispatched before instance or host initialization.
pub(crate) fn display(args: &[String]) -> i32 {
    match display_inner(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("tsumugi: kitty display: {e}");
            1
        }
    }
}
fn display_inner(args: &[String]) -> io::Result<()> {
    let [path] = args else {
        return Err(io::Error::other("missing display socket"));
    };
    let mut stream = UnixStream::connect(path)?;
    let mut output = io::stdout().lock();
    let fd = output.as_raw_fd();
    // SAFETY: kitty provides a PTY. Raw output avoids the PTY rewriting frames;
    // replies from kitty have no route into the authoritative managed session.
    unsafe {
        let mut attrs = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut attrs) != 0 {
            return Err(io::Error::last_os_error());
        }
        libc::cfmakeraw(&mut attrs);
        if libc::tcsetattr(fd, libc::TCSANOW, &attrs) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let mut previous = None;
    loop {
        // SAFETY: ioctl writes the initialized winsize on this process's PTY.
        let size = unsafe {
            let mut size: libc::winsize = std::mem::zeroed();
            if libc::ioctl(fd, libc::TIOCGWINSZ, &mut size) < 0 {
                return Err(io::Error::last_os_error());
            }
            (size.ws_col, size.ws_row, size.ws_xpixel, size.ws_ypixel)
        };
        if previous != Some(size) {
            writeln!(stream, "{} {} {} {}", size.0, size.1, size.2, size.3)?;
            previous = Some(size);
        }
        let mut events = libc::pollfd {
            fd: stream.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll borrows one valid descriptor; the deadline also observes
        // resize events even when the host screen has not changed.
        let ready = unsafe { libc::poll(&mut events, 1, 100) };
        if ready < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if ready > 0 {
            let mut bytes = [0; 65536];
            match stream.read(&mut bytes) {
                Ok(0) => return Ok(()),
                Ok(n) => {
                    output.write_all(&bytes[..n])?;
                    output.flush()?;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests;
