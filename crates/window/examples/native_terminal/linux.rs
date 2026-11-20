//! The native widget trial: geometry, terminal presentation and GTK lifetime.

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tsumugi_term::{Daemon, Palette, Pane, Screen, Size};

const TITLE: &str = "Tsumugi native terminal trial";

type Widget = *mut c_void;

#[repr(C)]
struct List {
    data: Widget,
    next: *mut List,
    previous: *mut List,
}

#[link(name = "gtk-3")]
unsafe extern "C" {
    fn gtk_window_list_toplevels() -> *mut List;
    fn gtk_window_get_title(window: Widget) -> *const c_char;
    fn gtk_bin_get_child(window: Widget) -> Widget;
    fn gtk_overlay_new() -> Widget;
    fn gtk_fixed_new() -> Widget;
    fn gtk_container_remove(container: Widget, child: Widget);
    fn gtk_container_add(container: Widget, child: Widget);
    fn gtk_overlay_add_overlay(overlay: Widget, child: Widget);
    fn gtk_fixed_put(fixed: Widget, child: Widget, x: c_int, y: c_int);
    fn gtk_fixed_move(fixed: Widget, child: Widget, x: c_int, y: c_int);
    fn gtk_widget_set_size_request(widget: Widget, width: c_int, height: c_int);
    fn gtk_widget_set_visible(widget: Widget, visible: c_int);
    fn gtk_widget_show_all(widget: Widget);
    fn gtk_widget_destroy(widget: Widget);
}

#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_ref(object: Widget) -> Widget;
    fn g_object_unref(object: Widget);
    fn g_signal_connect_data(
        object: Widget,
        signal: *const c_char,
        callback: Option<unsafe extern "C" fn()>,
        data: Widget,
        destroy: Option<unsafe extern "C" fn(Widget, Widget)>,
        flags: c_int,
    ) -> libc::c_ulong;
    fn g_signal_emit_by_name(object: Widget, signal: *const c_char, ...);
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_list_free(list: *mut List);
    fn g_free(data: Widget);
}

#[link(name = "pango-1.0")]
unsafe extern "C" {
    fn pango_font_description_from_string(font: *const c_char) -> Widget;
    fn pango_font_description_free(font: Widget);
}

struct Api {
    new: unsafe extern "C" fn() -> Widget,
    feed: unsafe extern "C" fn(Widget, *const c_char, isize),
    font: unsafe extern "C" fn(Widget, Widget),
    columns: unsafe extern "C" fn(Widget) -> libc::c_long,
    rows: unsafe extern "C" fn(Widget) -> libc::c_long,
    cell_width: unsafe extern "C" fn(Widget) -> libc::c_long,
    cell_height: unsafe extern "C" fn(Widget) -> libc::c_long,
    text: unsafe extern "C" fn(Widget, Widget, Widget, Widget) -> *mut c_char,
}

impl Api {
    fn load() -> Result<Self, String> {
        let library = CString::new(
            std::env::var("VTE_LIBRARY").unwrap_or_else(|_| "libvte-2.91.so.0".into()),
        )
        .map_err(|e| e.to_string())?;
        // GTypes retain code pointers, so the library stays loaded for the process.
        // SAFETY: library is NUL-terminated and dlopen checks the path.
        let handle = unsafe { libc::dlopen(library.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err("Cannot load VTE_LIBRARY (libvte-2.91.so.0)".into());
        }
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: handle is open and the symbol is NUL-terminated.
                let pointer = unsafe { libc::dlsym(handle, concat!($name, "\0").as_ptr().cast()) };
                if pointer.is_null() {
                    return Err(concat!("Missing VTE symbol: ", $name).into());
                }
                // SAFETY: this signature matches VTE's public C declaration.
                unsafe { std::mem::transmute::<*mut c_void, $ty>(pointer) }
            }};
        }
        Ok(Self {
            new: symbol!("vte_terminal_new", unsafe extern "C" fn() -> Widget),
            feed: symbol!(
                "vte_terminal_feed",
                unsafe extern "C" fn(Widget, *const c_char, isize)
            ),
            font: symbol!(
                "vte_terminal_set_font",
                unsafe extern "C" fn(Widget, Widget)
            ),
            columns: symbol!(
                "vte_terminal_get_column_count",
                unsafe extern "C" fn(Widget) -> libc::c_long
            ),
            rows: symbol!(
                "vte_terminal_get_row_count",
                unsafe extern "C" fn(Widget) -> libc::c_long
            ),
            cell_width: symbol!(
                "vte_terminal_get_char_width",
                unsafe extern "C" fn(Widget) -> libc::c_long
            ),
            cell_height: symbol!(
                "vte_terminal_get_char_height",
                unsafe extern "C" fn(Widget) -> libc::c_long
            ),
            text: symbol!(
                "vte_terminal_get_text",
                unsafe extern "C" fn(Widget, Widget, Widget, Widget) -> *mut c_char
            ),
        })
    }
}

struct Overlay(Widget);

impl Overlay {
    fn install() -> Result<Self, String> {
        // SAFETY: GTK is initialized by Dioxus; all calls run on its UI thread.
        unsafe {
            let list = gtk_window_list_toplevels();
            let mut item = list;
            let mut found = std::ptr::null_mut();
            while !item.is_null() {
                let window = (*item).data;
                let title = gtk_window_get_title(window);
                if !title.is_null() && CStr::from_ptr(title).to_bytes() == TITLE.as_bytes() {
                    found = window;
                    break;
                }
                item = (*item).next;
            }
            g_list_free(list);
            if found.is_null() {
                return Err("The trial window is not ready".into());
            }
            let content = gtk_bin_get_child(found);
            if content.is_null() {
                return Err("The desktop content is not ready".into());
            }
            g_object_ref(content);
            gtk_container_remove(found, content);
            let overlay = gtk_overlay_new();
            gtk_container_add(overlay, content);
            g_object_unref(content);
            gtk_container_add(found, overlay);
            let fixed = gtk_fixed_new();
            gtk_overlay_add_overlay(overlay, fixed);
            g_object_ref(fixed);
            gtk_widget_show_all(overlay);
            Ok(Self(fixed))
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: our strong reference keeps the GTK object alive after window close.
        unsafe {
            gtk_widget_destroy(self.0);
            g_object_unref(self.0);
        }
    }
}

struct Native {
    widget: Widget,
    api: Rc<Api>,
    _overlay: Rc<Overlay>,
}

impl Native {
    fn new(
        api: Rc<Api>,
        overlay: Rc<Overlay>,
        input: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        unsafe extern "C" fn commit(
            _terminal: Widget,
            text: *const c_char,
            len: c_uint,
            data: Widget,
        ) {
            // SAFETY: VTE owns text for this signal; data is the boxed sender below.
            unsafe {
                let sender = &*data.cast::<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>();
                let bytes = std::slice::from_raw_parts(text.cast::<u8>(), len as usize);
                let _ = sender.send(bytes.to_vec());
            }
        }
        unsafe extern "C" fn free_sender(data: Widget, _closure: Widget) {
            // SAFETY: GLib calls this once when disconnecting our signal handler.
            unsafe {
                drop(Box::from_raw(
                    data.cast::<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>(),
                ));
            }
        }
        // SAFETY: API was loaded from VTE; GTK objects are created on the UI thread.
        let widget = unsafe {
            let widget = (api.new)();
            let callback = std::mem::transmute::<
                unsafe extern "C" fn(Widget, *const c_char, c_uint, Widget),
                unsafe extern "C" fn(),
            >(commit);
            g_signal_connect_data(
                widget,
                c"commit".as_ptr(),
                Some(callback),
                Box::into_raw(Box::new(input)).cast(),
                Some(free_sender),
                0,
            );
            gtk_fixed_put(overlay.0, widget, 0, 0);
            g_object_ref(widget);
            let font = pango_font_description_from_string(c"monospace 14".as_ptr());
            (api.font)(widget, font);
            pango_font_description_free(font);
            gtk_widget_show_all(widget);
            widget
        };
        let native = Self {
            widget,
            api,
            _overlay: overlay,
        };
        native.feed("\x1b[?1049h\x1b[?7l");
        native
    }

    fn feed(&self, bytes: &str) {
        // SAFETY: widget is owned and bytes remains valid for the synchronous call.
        unsafe {
            (self.api.feed)(self.widget, bytes.as_ptr().cast(), bytes.len() as isize);
        }
    }

    fn rect(&self, rect: [f64; 4], compact: bool) {
        let [x, y, width, height] = rect;
        let width = if compact { width / 2.0 } else { width };
        // SAFETY: both widgets are owned and the request is bounded to the webview.
        unsafe {
            gtk_fixed_move(
                self._overlay.0,
                self.widget,
                x.max(0.0) as c_int,
                y.max(0.0) as c_int,
            );
            gtk_widget_set_size_request(
                self.widget,
                width.max(1.0) as c_int,
                height.max(1.0) as c_int,
            );
        }
    }

    fn size(&self) -> Size {
        // SAFETY: all functions inspect our live widget on the GTK thread.
        unsafe {
            Size {
                cols: (self.api.columns)(self.widget).clamp(1, 1000) as u16,
                rows: (self.api.rows)(self.widget).clamp(1, 1000) as u16,
                cell_width: (self.api.cell_width)(self.widget).max(1) as f64,
                cell_height: (self.api.cell_height)(self.widget).max(1) as f64,
            }
        }
    }

    fn visible(&self, visible: bool) {
        // SAFETY: widget is live and owned.
        unsafe {
            gtk_widget_set_visible(self.widget, c_int::from(visible));
        }
    }

    fn input_for_trial(&self, text: &str) {
        let text = CString::new(text).expect("static trial input");
        // SAFETY: the parameters match VTE's commit signal and remain valid.
        unsafe {
            g_signal_emit_by_name(
                self.widget,
                c"commit".as_ptr(),
                text.as_ptr(),
                text.as_bytes().len() as c_uint,
            );
        }
    }

    fn text(&self) -> String {
        // SAFETY: get_text returns an allocated string when no selection callback is supplied.
        unsafe {
            let text = (self.api.text)(
                self.widget,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if text.is_null() {
                return String::new();
            }
            let owned = CStr::from_ptr(text).to_string_lossy().into_owned();
            g_free(text.cast());
            owned
        }
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        // SAFETY: the strong reference survives destruction of the parent window.
        unsafe {
            gtk_widget_destroy(self.widget);
            g_object_unref(self.widget);
        }
    }
}

fn frame(screen: &Screen) -> String {
    let mut out = String::from("\x1b[?25l");
    for (row, line) in screen.lines.iter().enumerate() {
        let _ = write!(out, "\x1b[{};1H", row + 1);
        for run in &line.runs {
            let style = run.style;
            let bg = style.bg.unwrap_or(screen.background);
            let fg = style.fg;
            let _ = write!(
                out,
                "\x1b[0m\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m",
                fg.r, fg.g, fg.b, bg.r, bg.g, bg.b
            );
            for (on, code) in [
                (style.bold, 1),
                (style.italic, 3),
                (style.underline, 4),
                (style.strike, 9),
            ] {
                if on {
                    let _ = write!(out, "\x1b[{code}m");
                }
            }
            out.extend(
                run.text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c }),
            );
        }
    }
    if let Some(cursor) = screen.cursor {
        let shape = match cursor.shape {
            tsumugi_term::CursorShape::Block => 2,
            tsumugi_term::CursorShape::Underline => 4,
            tsumugi_term::CursorShape::Beam => 6,
        } - u8::from(cursor.blink);
        let _ = write!(
            out,
            "\x1b[{};{}H\x1b[{shape} q\x1b[?25h",
            cursor.row + 1,
            cursor.col + 1
        );
    }
    out
}

#[derive(Clone)]
struct Backend {
    daemon: Daemon,
    pane: Arc<Pane>,
    changed: Arc<AtomicBool>,
}

async fn attach() -> Result<Backend, String> {
    let socket = std::env::var_os("TSUMUGI_DAEMON").ok_or("TSUMUGI_DAEMON is required")?;
    let resource = std::env::var("TSUMUGI_NATIVE_TERMINAL_RESOURCE").map_err(|e| e.to_string())?;
    let daemon = Daemon::new(socket);
    let session = daemon
        .sessions()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| {
            tsumugi_term::tags::value(&s.tags, "tsumugi.resource").as_deref() == Some(&resource)
        })
        .ok_or("The requested terminal resource was not found")?;
    let changed = Arc::new(AtomicBool::new(true));
    let wake = changed.clone();
    let pane = Pane::attach(
        &daemon,
        session.id,
        Size::new(80, 24),
        &Palette::default(),
        move || {
            wake.store(true, Ordering::Relaxed);
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(Backend {
        daemon,
        pane: Arc::new(pane),
        changed,
    })
}

#[derive(Clone)]
struct Trial {
    backend: Rc<RefCell<Option<Backend>>>,
    native: Rc<RefCell<Option<Native>>>,
    compact: Signal<bool>,
    shown: Signal<bool>,
    attached: Signal<bool>,
    status: Signal<String>,
}

#[component]
fn App() -> Element {
    let mut compact = use_signal(|| false);
    let mut shown = use_signal(|| true);
    let mut attached = use_signal(|| true);
    let status = use_signal(|| "Connecting…".to_string());
    let trial = use_hook(|| Trial {
        backend: Rc::default(),
        native: Rc::default(),
        compact,
        shown,
        attached,
        status,
    });
    use_future({
        let trial = trial.clone();
        move || {
            let trial = trial.clone();
            async move {
                if let Err(error) = serve(trial.clone()).await {
                    let mut status = trial.status;
                    status.set(error.clone());
                    eprintln!("NATIVE_PROTOTYPE_ERROR: {error}");
                }
            }
        }
    });
    use_future({
        let trial = trial.clone();
        move || {
            let trial = trial.clone();
            async move {
                if std::env::var_os("TSUMUGI_NATIVE_AUTOTEST").is_some()
                    && let Err(error) = automated(trial).await
                {
                    eprintln!("NATIVE_AUTOTEST_ERROR: {error}");
                }
            }
        }
    });
    rsx! {
        div { class: "trial",
            header {
                strong { "Tsumugi — native terminal trial" }
                button { onclick: move |_| shown.set(true), "Terminal" }
                button { onclick: move |_| shown.set(false), "Other tab" }
                button { onclick: move |_| compact.toggle(), "Resize terminal" }
                button { onclick: move |_| attached.toggle(), if attached() { "Detach view" } else { "Reattach view" } }
            }
            div { class: "status", "{status}" }
            div { id: "native-console",
                if !shown() { p { "Another tab. The terminal's program is still running." } }
                else if !attached() { p { "The view is detached. Reattach to the same running terminal." } }
            }
            footer { "Experimental VTE view inside the application. This is not kitty." }
        }
    }
}

async fn serve(mut trial: Trial) -> Result<(), String> {
    let backend = attach().await?;
    *trial.backend.borrow_mut() = Some(backend.clone());
    let api = Rc::new(Api::load()?);
    let overlay = Rc::new(Overlay::install()?);
    let (input, mut receiving) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let sending = backend.clone();
    spawn(async move {
        while let Some(bytes) = receiving.recv().await {
            if let Err(error) = sending.daemon.press(sending.pane.id(), &bytes).await {
                eprintln!("Native input: {error}");
            }
        }
    });
    let mut last_size = None;
    let mut last_rect = None;
    let mut last_compact = false;
    let mut tick = tokio::time::interval(Duration::from_millis(30));
    trial
        .status
        .set("Connected to the existing Tsumugi terminal".into());
    loop {
        tick.tick().await;
        let shown = *trial.shown.peek();
        if !*trial.attached.peek() {
            trial.native.borrow_mut().take();
            continue;
        }
        if trial.native.borrow().is_none() {
            *trial.native.borrow_mut() =
                Some(Native::new(api.clone(), overlay.clone(), input.clone()));
            backend.changed.store(true, Ordering::Relaxed);
            last_rect = None;
        }
        let mut query = document::eval(
            "const r = document.getElementById('native-console').getBoundingClientRect(); dioxus.send([r.x,r.y,r.width,r.height]);",
        );
        let rect: [f64; 4] = query.recv().await.map_err(|e| e.to_string())?;
        let compact = *trial.compact.peek();
        let native = trial.native.borrow();
        let native = native.as_ref().expect("created above");
        native.visible(shown);
        if !shown {
            continue;
        }
        if last_rect != Some(rect) || compact != last_compact {
            native.rect(rect, compact);
            last_rect = Some(rect);
            last_compact = compact;
        }
        let size = native.size();
        if last_size != Some(size) {
            backend.pane.resize(size);
            last_size = Some(size);
            println!("NATIVE_RESIZE {} {}", size.cols, size.rows);
        }
        if backend.changed.swap(false, Ordering::Relaxed) {
            native.feed(&frame(&backend.pane.screen()));
        }
    }
}

async fn wait_line(trial: &Trial, text: &str) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if trial.backend.borrow().as_ref().is_some_and(|b| {
                b.pane
                    .screen()
                    .text()
                    .iter()
                    .any(|line| line.trim() == text)
            }) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .map_err(|_| format!("The terminal did not show {text}"))
}

async fn automated(mut trial: Trial) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(15), async {
        while trial.native.borrow().is_none() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .map_err(|_| "The native widget did not appear")?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let id = trial
        .backend
        .borrow()
        .as_ref()
        .expect("connected")
        .pane
        .id();
    trial
        .native
        .borrow()
        .as_ref()
        .expect("widget")
        .input_for_trial("NATIVE_RETAINED=kept; printf '\\nNATIVE_INPUT_OK\\n'\r");
    wait_line(&trial, "NATIVE_INPUT_OK").await?;
    trial.shown.set(false);
    tokio::time::sleep(Duration::from_millis(200)).await;
    trial.shown.set(true);
    trial.attached.set(false);
    tokio::time::sleep(Duration::from_millis(200)).await;
    trial.attached.set(true);
    tokio::time::sleep(Duration::from_millis(200)).await;
    trial
        .native
        .borrow()
        .as_ref()
        .expect("reattached")
        .input_for_trial("printf '\\nREATTACH_%s\\n' \"$NATIVE_RETAINED\"\r");
    wait_line(&trial, "REATTACH_kept").await?;
    let before = trial.native.borrow().as_ref().expect("widget").size();
    trial.compact.set(true);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after = trial.native.borrow().as_ref().expect("widget").size();
    if before.cols == after.cols {
        return Err("Native resize did not change the column count".into());
    }
    trial
        .native
        .borrow()
        .as_ref()
        .expect("widget")
        .input_for_trial("stty size\r");
    wait_line(&trial, &format!("{} {}", after.rows, after.cols)).await?;
    let same_session = trial
        .backend
        .borrow()
        .as_ref()
        .expect("connected")
        .pane
        .id()
        == id;
    let rendered = trial
        .native
        .borrow()
        .as_ref()
        .expect("widget")
        .text()
        .contains("REATTACH_kept");
    if !same_session || !rendered {
        return Err("Native display or retained attachment changed".into());
    }
    trial
        .status
        .set("Input, tab switching, view reattachment and resize passed".into());
    println!(
        "NATIVE_PROTOTYPE_OK input=true reattach=true resize={}x{}->{}x{} same_session=true native_text=true",
        before.cols, before.rows, after.cols, after.rows
    );
    Ok(())
}

pub fn run() {
    let config = Config::new().with_menu(None)
        .with_window(WindowBuilder::new().with_title(TITLE).with_inner_size(LogicalSize::new(1000.0, 700.0)))
        .with_custom_head("<style>html,body,#main{margin:0;height:100%;background:#151e29;color:#e3e7ec;font:15px sans-serif}.trial{height:100%;display:flex;flex-direction:column}header{display:flex;align-items:center;gap:12px;padding:16px}button{padding:8px;cursor:pointer}.status,footer{padding:12px}#native-console{flex:1;margin:0 12px;min-height:180px;border:1px solid #526174}#native-console p{padding:16px}</style>".into());
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_term::{Cursor, CursorShape, Line, Palette, Run, Screen, Style};

    #[test]
    fn a_native_frame_preserves_wide_text_colours_and_the_programs_cursor() {
        let palette = Palette::default();
        let screen = Screen {
            cols: 4,
            rows: 1,
            lines: vec![Line {
                runs: vec![Run {
                    text: "日本".into(),
                    width: 4,
                    wide: true,
                    apart: true,
                    style: Style {
                        fg: palette.foreground,
                        bg: None,
                        bold: true,
                        italic: false,
                        underline: false,
                        strike: false,
                    },
                }],
            }],
            cursor: Some(Cursor {
                row: 0,
                col: 2,
                shape: CursorShape::Beam,
                blink: false,
            }),
            background: palette.background,
        };
        let frame = frame(&screen);
        assert!(frame.contains("日本"));
        assert!(frame.contains("\x1b[38;2;"));
        assert!(frame.contains("\x1b[1m"));
        assert!(frame.contains("\x1b[1;3H\x1b[6 q"));
        assert!(
            !frame.contains("\r\n"),
            "painting the last row must not scroll it"
        );
    }
}
