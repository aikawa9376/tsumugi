//! VTE is optional at runtime; GTK objects stay on the desktop's UI thread.
use crate::native_layer::Layer;
use crate::shell::Ui;
use dioxus::prelude::*;
use gtk::glib::translate::{FromGlibPtrNone, ToGlibPtr};
use gtk::prelude::*;
use std::cell::OnceCell;
use std::ffi::{CString, c_char, c_void};
use std::rc::Rc;
use std::time::Duration;
use tsumugi_core::content::{Area, Tab};
use tsumugi_core::msg::Msg;
use tsumugi_core::settings::TerminalRenderer;
use tsumugi_term::{Palette, Rgb, Size};

type Pointer = *mut c_void;

struct Api {
    new: unsafe extern "C" fn() -> *mut gtk::ffi::GtkWidget,
    feed: unsafe extern "C" fn(Pointer, *const c_char, isize),
    font: unsafe extern "C" fn(Pointer, *const gtk::pango::ffi::PangoFontDescription),
    columns: unsafe extern "C" fn(Pointer) -> libc::c_long,
    rows: unsafe extern "C" fn(Pointer) -> libc::c_long,
    width: unsafe extern "C" fn(Pointer) -> libc::c_long,
    height: unsafe extern "C" fn(Pointer) -> libc::c_long,
    colors: unsafe extern "C" fn(
        Pointer,
        *const gtk::gdk::ffi::GdkRGBA,
        *const gtk::gdk::ffi::GdkRGBA,
        Pointer,
        usize,
    ),
    cursor: unsafe extern "C" fn(Pointer, *const gtk::gdk::ffi::GdkRGBA),
    input: unsafe extern "C" fn(Pointer, libc::c_int),
}
impl Api {
    fn load() -> Result<Self, String> {
        let library = CString::new(
            std::env::var("TSUMUGI_VTE_LIBRARY").unwrap_or_else(|_| "libvte-2.91.so.0".into()),
        )
        .map_err(|e| e.to_string())?;
        // GTypes retain code pointers: keep the library loaded for the process.
        // SAFETY: dlopen validates the NUL-terminated library path.
        let handle = unsafe { libc::dlopen(library.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(
                "GTK3 VTE is not installed (libvte-2.91.so.0); using the built-in display".into(),
            );
        }
        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                // SAFETY: the handle remains open and the symbol is NUL-terminated.
                let pointer = unsafe { libc::dlsym(handle, concat!($name, "\0").as_ptr().cast()) };
                if pointer.is_null() {
                    return Err(concat!("Missing VTE symbol: ", $name).into());
                }
                // SAFETY: the signature matches VTE's public GTK3 C API.
                unsafe { std::mem::transmute::<Pointer, $ty>(pointer) }
            }};
        }
        Ok(Self {
            new: symbol!(
                "vte_terminal_new",
                unsafe extern "C" fn() -> *mut gtk::ffi::GtkWidget
            ),
            feed: symbol!(
                "vte_terminal_feed",
                unsafe extern "C" fn(Pointer, *const c_char, isize)
            ),
            font: symbol!(
                "vte_terminal_set_font",
                unsafe extern "C" fn(Pointer, *const gtk::pango::ffi::PangoFontDescription)
            ),
            columns: symbol!(
                "vte_terminal_get_column_count",
                unsafe extern "C" fn(Pointer) -> libc::c_long
            ),
            rows: symbol!(
                "vte_terminal_get_row_count",
                unsafe extern "C" fn(Pointer) -> libc::c_long
            ),
            width: symbol!(
                "vte_terminal_get_char_width",
                unsafe extern "C" fn(Pointer) -> libc::c_long
            ),
            height: symbol!(
                "vte_terminal_get_char_height",
                unsafe extern "C" fn(Pointer) -> libc::c_long
            ),
            colors: symbol!(
                "vte_terminal_set_colors",
                unsafe extern "C" fn(
                    Pointer,
                    *const gtk::gdk::ffi::GdkRGBA,
                    *const gtk::gdk::ffi::GdkRGBA,
                    Pointer,
                    usize,
                )
            ),
            cursor: symbol!(
                "vte_terminal_set_color_cursor",
                unsafe extern "C" fn(Pointer, *const gtk::gdk::ffi::GdkRGBA)
            ),
            input: symbol!(
                "vte_terminal_set_input_enabled",
                unsafe extern "C" fn(Pointer, libc::c_int)
            ),
        })
    }
}
thread_local! {
    static API: OnceCell<Result<Rc<Api>, String>> = const { OnceCell::new() };
}
fn api() -> Result<Rc<Api>, String> {
    API.with(|api| api.get_or_init(|| Api::load().map(Rc::new)).clone())
}
pub(crate) fn available() -> Result<(), String> {
    api().map(|_| ())
}

struct Native {
    widget: gtk::Widget,
    api: Rc<Api>,
    layer: Rc<Layer>,
}
impl Native {
    fn new(api: Rc<Api>, layer: Rc<Layer>) -> Self {
        // SAFETY: VTE returns a floating GtkWidget; the wrapper sinks its
        // reference. GTK is initialized by Dioxus on this thread.
        let widget = unsafe { gtk::Widget::from_glib_none((api.new)()) };
        match crate::bundled_font::native_map() {
            Ok(map) => widget.set_font_map(Some(&map)),
            Err(error) => tracing::warn!(%error, "native terminal fonts could not be registered"),
        }
        widget.set_can_focus(false);
        layer.fixed.put(&widget, 0, 0);
        let native = Self { widget, api, layer };
        // No PTY is attached. VTE only presents the host's authoritative screen.
        // SAFETY: this is a live VTE object on its owning thread.
        unsafe {
            (native.api.input)(native.pointer(), 0);
        }
        native.feed("\x1b[?1049h\x1b[?7l");
        native
    }
    fn pointer(&self) -> Pointer {
        let pointer: *mut gtk::ffi::GtkWidget = self.widget.to_glib_none().0;
        pointer.cast()
    }
    fn feed(&self, text: &str) {
        // SAFETY: widget and bytes remain live throughout the synchronous call.
        unsafe {
            (self.api.feed)(self.pointer(), text.as_ptr().cast(), text.len() as isize);
        }
    }
    fn font(&self, family: &str, pixels: f64, weight: u16) {
        let mut font = gtk::pango::FontDescription::new();
        let family = if family.is_empty() {
            "JetBrainsMono Nerd Font Mono"
        } else {
            family
        };
        font.set_family(&format!("{family},JetBrainsMono Nerd Font Mono,monospace"));
        font.set_absolute_size(pixels * f64::from(gtk::pango::SCALE));
        // SAFETY: Pango accepts numeric weights, bounded by Settings.
        font.set_weight(unsafe { gtk::glib::translate::from_glib(i32::from(weight)) });
        // SAFETY: the font pointer is borrowed only for the call.
        unsafe {
            (self.api.font)(self.pointer(), font.to_glib_none().0);
        }
    }
    fn colors(&self, palette: &Palette) {
        fn rgba(c: Rgb) -> gtk::gdk::RGBA {
            gtk::gdk::RGBA::new(
                f64::from(c.r) / 255.0,
                f64::from(c.g) / 255.0,
                f64::from(c.b) / 255.0,
                1.0,
            )
        }
        let fg = rgba(palette.foreground);
        let bg = rgba(palette.background);
        let cursor = rgba(palette.cursor);
        // SAFETY: pointers are borrowed for the calls; ANSI colors are supplied
        // explicitly by each frame, so there is no palette array to borrow.
        unsafe {
            (self.api.colors)(
                self.pointer(),
                fg.to_glib_none().0,
                bg.to_glib_none().0,
                std::ptr::null_mut(),
                0,
            );
            (self.api.cursor)(self.pointer(), cursor.to_glib_none().0);
        }
    }
    fn rect(&self, [x, y, w, h]: [f64; 4]) {
        self.layer
            .fixed
            .move_(&self.widget, x.round() as i32, y.round() as i32);
        self.widget
            .set_size_request(w.floor().max(1.0) as i32, h.floor().max(1.0) as i32);
    }
    fn size(&self) -> Size {
        // SAFETY: these getters inspect our live VTE object on the GTK thread.
        unsafe {
            Size {
                cols: (self.api.columns)(self.pointer()).clamp(1, 1000) as u16,
                rows: (self.api.rows)(self.pointer()).clamp(1, 1000) as u16,
                cell_width: (self.api.width)(self.pointer()).max(1) as f64,
                cell_height: (self.api.height)(self.pointer()).max(1) as f64,
            }
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        // SAFETY: the wrapper retains a reference even if the parent closed.
        // Destroying the display never ends the host's terminal.
        unsafe {
            self.widget.destroy();
        }
    }
}

enum Display {
    Vte(Native),
    Kitty(Box<super::kitty::Kitty>),
}
impl Display {
    fn widget(&self) -> &gtk::Widget {
        match self {
            Self::Vte(native) => &native.widget,
            Self::Kitty(kitty) => kitty.widget.upcast_ref(),
        }
    }
    fn font(&self, family: &str, pixels: f64, weight: u16) {
        if let Self::Vte(native) = self {
            native.font(family, pixels, weight);
        }
    }
    fn colors(&self, palette: &Palette) {
        if let Self::Vte(native) = self {
            native.colors(palette);
        }
    }
    fn rect(&self, rect: [f64; 4]) {
        match self {
            Self::Vte(native) => native.rect(rect),
            Self::Kitty(kitty) => kitty.rect(rect),
        }
    }
    fn feed(&mut self, screen: &tsumugi_term::Screen, focused: bool) {
        match self {
            Self::Vte(native) => native.feed(&super::frame(screen, focused)),
            Self::Kitty(kitty) => kitty.feed(screen, focused),
        }
    }
    fn poll(&mut self) -> Result<Option<Size>, String> {
        match self {
            Self::Vte(native) => Ok(Some(native.size())),
            Self::Kitty(kitty) => kitty.poll(),
        }
    }
    fn cover(&self, size: [f64; 2], cover: Option<[f64; 4]>) {
        if let Self::Kitty(kitty) = self {
            kitty.cover(size, cover);
        }
    }
}

/// A single overlay follows the focused terminal. Other splits, session
/// consoles, images, GUI Neovims and the reader keep their existing displays.
pub(crate) fn use_native_layer(ui: &Ui) {
    let size = use_signal(|| None);
    use_context_provider(|| super::NativeSize(size));
    let ui = ui.clone();
    use_future(move || {
        let ui = ui.clone();
        async move {
            follow(ui, size).await;
        }
    });
}
/// What woke the display to follow the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Woke {
    /// The tick, which reads the page's layout again.
    Tick,
    /// A pane on screen drawn anew.
    Drawn,
}

/// The page's layout as the display last read it: the tab it was read for,
/// the console's rectangle and what a banner covers of it.
#[derive(Default)]
struct Shown {
    tab: Option<Tab>,
    rect: Option<[f64; 4]>,
    cover: Option<Option<[f64; 4]>>,
}

impl Shown {
    /// The layout to draw `tab` in without asking the page: a redraw of the
    /// tab whose layout is known. Asking would wait on the page, busy with
    /// the very keys whose echo is drawn; the tick still asks.
    fn layout(&self, woke: Woke, tab: Option<&Tab>) -> Option<([f64; 4], Option<[f64; 4]>)> {
        if woke != Woke::Drawn || self.tab.as_ref() != tab {
            return None;
        }
        Some((self.rect?, self.cover?))
    }
}

async fn follow(ui: Ui, mut shown_size: Signal<Option<(Tab, Size)>>) {
    let desktop = dioxus::desktop::window();
    let mut layer = None;
    let mut native: Option<Display> = None;
    let mut last_frame = None;
    let mut shown = Shown::default();
    let mut last_font = None;
    let mut last_palette = None;
    let mut last_tab = None;
    let mut last_renderer = None;
    let mut failed = false;
    // The tick follows the page's layout; a frame drawn is shown at once
    // rather than at the next tick, which would hold a key's echo back.
    let mut tick = tokio::time::interval(Duration::from_millis(33));
    loop {
        let woke = tokio::select! {
            _ = tick.tick() => Woke::Tick,
            () = ui.hub.drawn.notified() => Woke::Drawn,
        };
        let app = ui.app;
        let renderer = app.config.peek().settings.console.renderer;
        if last_renderer != Some(renderer) {
            native = None;
            last_frame = None;
            shown = Shown::default();
            last_font = None;
            last_palette = None;
            last_renderer = Some(renderer);
            failed = false;
        }
        let tab = app.active();
        let pane = tab
            .as_ref()
            .and_then(|t| ui.hub.panes.borrow().get(t).cloned());
        let active = super::presents(renderer, tab.as_ref())
            && !failed
            && pane
                .as_ref()
                .is_some_and(|p| p.pictures().is_empty() && !p.exited())
            && tab.as_ref().is_none_or(|t| ui.hub.gui(t).is_none())
            && !app.console.peek().reading
            && app.held().is_empty()
            && app.content.peek().drag.is_none()
            && app.content.peek().moving.is_none()
            && app.chrome.peek().panels.drag.is_none();
        if !active {
            if let Some(native) = &native {
                if native.widget().is_visible() {
                    tracing::debug!("native terminal display hidden");
                }
                native.widget().hide();
            }
            if shown_size.peek().is_some() {
                shown_size.set(None);
            }
            last_frame = None;
            // Shown again, it reads the layout afresh.
            shown.tab = None;
            continue;
        }
        let Some(pane) = pane else { continue };
        let zoom = crate::scaling::current();
        let known = shown
            .layout(woke, tab.as_ref())
            .filter(|_| native.is_some());
        let (rect, cover) = match known {
            Some(known) => known,
            None => {
                let script = format!(
                    "const c = document.querySelector('#focused-console > .console-screen[data-native-pane=\"{}\"]');",
                    pane.id()
                );
                let mut query = document::eval(&format!(
            "{script}\n{}",
            "
             if (!c || !c.isConnected) { dioxus.send(null); } else {
               const r = c.getBoundingClientRect(); const s = getComputedStyle(c);
               const l = parseFloat(s.paddingLeft), t = parseFloat(s.paddingTop);
               const rr = parseFloat(s.paddingRight), b = parseFloat(s.paddingBottom);
               // The prompt card floats over the screen's foot; the terminal
               // stops above it, as the built-in display's rows do.
               const card = parseFloat(getComputedStyle(c.parentElement).getPropertyValue('--composer')) || 0;
               const rect = [r.x+l,r.y+t,r.width-l-rr,r.height-t-b-card];
               const banner = c.parentElement.querySelector(':scope > .direct-banner');
               const cover = banner ? banner.getBoundingClientRect() : null;
               dioxus.send([rect, cover ? [cover.x-rect[0],cover.y-rect[1],cover.width,cover.height] : null]);
             }"
        ));
                let (rect, cover) = match query.recv::<Option<([f64; 4], Option<[f64; 4]>)>>().await
                {
                    Ok(Some((rect, cover)))
                        if rect.iter().all(|v| v.is_finite()) && rect[2] > 1.0 && rect[3] > 1.0 =>
                    {
                        (
                            rect.map(|v| v * zoom),
                            cover
                                .filter(|c| {
                                    c.iter().all(|v| v.is_finite()) && c[2] > 0.0 && c[3] > 0.0
                                })
                                .map(|c| c.map(|v| v * zoom)),
                        )
                    }
                    _ => {
                        if let Some(display) = &native {
                            display.widget().hide();
                        }
                        if shown_size.peek().is_some() {
                            shown_size.set(None);
                        }
                        last_frame = None;
                        continue;
                    }
                };
                // WebKit may reply after the user switches tabs or opens a dialog.
                if app.active() != tab || !app.held().is_empty() {
                    if let Some(display) = &native {
                        display.widget().hide();
                    }
                    if shown_size.peek().is_some() {
                        shown_size.set(None);
                    }
                    continue;
                }
                (rect, cover)
            }
        };
        let font = {
            let config = app.config.peek();
            let s = &config.settings;
            (s.font_family.clone(), s.font_size * zoom, s.font_weight)
        };
        // kitty takes its font only as it starts.
        if matches!(native, Some(Display::Kitty(_))) && last_font.as_ref() != Some(&font) {
            native = None;
            last_font = None;
            last_frame = None;
            shown = Shown::default();
            continue;
        }
        if native.is_none() {
            if layer.is_none() {
                match Layer::install(&desktop) {
                    Ok(installed) => layer = Some(Rc::new(installed)),
                    Err(error) => {
                        tracing::warn!(%error);
                        continue;
                    }
                }
            }
            let layer = layer.as_ref().expect("installed").clone();
            let started = match renderer {
                TerminalRenderer::Vte => api().map(|api| Display::Vte(Native::new(api, layer))),
                TerminalRenderer::Kitty => super::kitty::Kitty::new(layer, &font.0, font.1)
                    .map(|k| Display::Kitty(Box::new(k))),
                TerminalRenderer::Builtin => unreachable!(),
            };
            match started {
                Ok(display) => native = Some(display),
                Err(error) => {
                    tracing::warn!(%error);
                    // The window's log goes nowhere a user sees once it is
                    // started from a launcher.
                    app.dispatch(Msg::Notice(error));
                    failed = true;
                    continue;
                }
            }
        }
        let display = native.as_mut().expect("created above");
        if last_font.as_ref() != Some(&font) {
            display.font(&font.0, font.1, font.2);
            last_font = Some(font);
            last_frame = None;
        }
        let palette = ui.hub.palette.peek().clone();
        if last_palette.as_ref() != Some(&palette) {
            display.colors(&palette);
            last_palette = Some(palette);
            last_frame = None;
        }
        let geometry_changed = shown.rect != Some(rect);
        if geometry_changed {
            display.rect(rect);
            shown.rect = Some(rect);
            last_frame = None;
        }
        shown.tab.clone_from(&tab);
        if !display.widget().is_visible() {
            tracing::debug!(pane = %pane.id(), "native terminal display shown");
        }
        display.widget().show();
        let size = match display.poll() {
            Ok(Some(size)) => size,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(%error);
                app.dispatch(Msg::Notice(error));
                native = None;
                shown_size.set(None);
                failed = true;
                continue;
            }
        };
        if geometry_changed || shown.cover != Some(cover) {
            display.cover([rect[2], rect[3]], cover);
            shown.cover = Some(cover);
        }
        let css_size = Size {
            cell_width: size.cell_width / zoom,
            cell_height: size.cell_height / zoom,
            ..size
        };
        let next_size = tab.clone().map(|t| (t, css_size));
        if *shown_size.peek() != next_size {
            shown_size.set(next_size);
        }
        let focused = app.focus() == Area::Console;
        let revision = (*ui.local.frame.peek(), focused, css_size);
        if last_tab != tab || last_frame != Some(revision) {
            let mut screen = pane.screen();
            if screen.cols != usize::from(size.cols) || screen.rows != usize::from(size.rows) {
                pane.resize(css_size);
                screen = pane.screen();
            }
            display.feed(&screen, focused);
            last_frame = Some(revision);
            last_tab = tab;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_term::{Daemon, Pane, Spawn};

    #[test]
    fn a_redraw_of_the_tab_shown_reuses_its_layout_and_a_tick_or_another_tab_asks_again() {
        let env = tsumugi_model::EnvironmentId::new("here");
        let (a, b) = (Tab::Terminal(env.clone(), 1), Tab::Terminal(env, 2));
        let rect = [1.0, 2.0, 300.0, 200.0];
        let cover = Some([0.0, 0.0, 300.0, 20.0]);
        let shown = Shown {
            tab: Some(a.clone()),
            rect: Some(rect),
            cover: Some(cover),
        };
        assert_eq!(shown.layout(Woke::Drawn, Some(&a)), Some((rect, cover)));
        assert_eq!(shown.layout(Woke::Tick, Some(&a)), None);
        assert_eq!(shown.layout(Woke::Drawn, Some(&b)), None);
        let unknown = Shown {
            tab: Some(a.clone()),
            rect: None,
            cover: None,
        };
        assert_eq!(unknown.layout(Woke::Drawn, Some(&a)), None);
    }

    fn settle() {
        let until = std::time::Instant::now() + Duration::from_millis(160);
        while std::time::Instant::now() < until {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn text(native: &Native) -> String {
        use std::ffi::CStr;
        // SAFETY: the production API has already loaded GTK3 VTE for this process.
        unsafe {
            let library = libc::dlopen(
                c"libvte-2.91.so.0".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            assert!(!library.is_null());
            let pointer = libc::dlsym(library, c"vte_terminal_get_text".as_ptr());
            assert!(!pointer.is_null());
            let get = std::mem::transmute::<
                Pointer,
                unsafe extern "C" fn(Pointer, Pointer, Pointer, Pointer) -> *mut c_char,
            >(pointer);
            let text = get(
                native.pointer(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            assert!(!text.is_null());
            let owned = CStr::from_ptr(text).to_string_lossy().into_owned();
            gtk::glib::ffi::g_free(text.cast());
            libc::dlclose(library);
            owned
        }
    }
    async fn wait_line(pane: &Pane, expected: &str) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if pane.screen().text().iter().any(|l| l.trim() == expected) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    #[ignore = "requires a GTK display and GTK3 VTE; run on Wayland and X11"]
    async fn native_display_preserves_the_managed_terminal_across_hiding_resizing_and_recreation() {
        gtk::init().unwrap();
        let dir = tempfile::Builder::new()
            .prefix("vte-")
            .tempdir_in(tsumugi_sys::local::temp_dir())
            .unwrap();
        tsumugi_sys::fs::set_mode(dir.path(), 0o700).unwrap();
        let socket = dir.path().join("d.sock");
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
        let palette = Palette::default();
        let pane = Pane::attach(&daemon, id, Size::new(80, 24), &palette, || {})
            .await
            .unwrap();
        daemon
            .press(id, b"retained=kept; printf '\\nNATIVE_INPUT_OK\\n'\r")
            .await
            .unwrap();
        wait_line(&pane, "NATIVE_INPUT_OK").await;
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Tsumugi VTE integration check");
        window.set_default_size(720, 480);
        let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let page = gtk::DrawingArea::new();
        parent.pack_start(&page, true, true, 0);
        window.add(&parent);
        let layer = Rc::new(Layer::wrap(&page).unwrap());
        assert!(layer.passes_input());
        let api = api().unwrap();
        crate::bundled_font::native_map().unwrap();
        let native = Native::new(api.clone(), layer.clone());
        native.font("", 15.0, 400);
        native.colors(&palette);
        native.rect([0.0, 0.0, 640.0, 360.0]);
        window.show_all();
        settle();
        assert!(
            !native.widget.can_focus(),
            "keyboard/IME focus belongs to the page"
        );
        let before = native.size();
        pane.resize(before);
        native.feed(&super::super::frame(&pane.screen(), true));
        settle();
        assert!(text(&native).contains("NATIVE_INPUT_OK"));
        let description =
            gtk::pango::FontDescription::from_string("JetBrainsMono Nerd Font Mono 15");
        let font = native
            .widget
            .pango_context()
            .load_font(&description)
            .unwrap();
        eprintln!(
            "Native font selected: {}; matching families: {:?}",
            font.describe(),
            native
                .widget
                .pango_context()
                .list_families()
                .iter()
                .filter(|f| f.name().contains("JetBrains"))
                .map(|f| f.name())
                .collect::<Vec<_>>()
        );
        assert!(
            font.describe().family().unwrap().contains("JetBrains"),
            "the bundled font must be available to VTE"
        );
        native.widget.hide();
        assert!(daemon.sessions().await.unwrap().iter().any(|s| s.id == id));
        drop(native);
        let native = Native::new(api, layer);
        native.font("", 15.0, 400);
        native.rect([0.0, 0.0, 320.0, 240.0]);
        native.widget.show();
        settle();
        let after = native.size();
        assert!(after.cols < before.cols);
        assert!(after.rows < before.rows);
        pane.resize(after);
        daemon
            .press(id, b"printf '\\nREATTACH_%s\\n' \"$retained\"\r")
            .await
            .unwrap();
        wait_line(&pane, "REATTACH_kept").await;
        native.feed(&super::super::frame(&pane.screen(), true));
        settle();
        assert!(text(&native).contains("REATTACH_kept"));
        assert_eq!(pane.id(), id);
        assert!(daemon.sessions().await.unwrap().iter().any(|s| s.id == id));
        drop(native);
        // SAFETY: all widgets are owned on this GTK thread.
        unsafe {
            window.destroy();
        }
        daemon.end(id).await.unwrap();
        task.abort();
        println!(
            "VTE_NATIVE_OK input=true hide=true recreate=true resize={}x{}->{}x{} retained=true",
            before.cols, before.rows, after.cols, after.rows
        );
    }
}
