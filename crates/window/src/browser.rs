//! Browsing belongs to the client window; an environment names its profile.

mod agent;
mod annotations;
mod capture;
mod card;
mod closed;
mod downloads;
mod feedback;
mod find;
mod markup;
#[cfg(all(test, target_os = "linux"))]
mod native_tests;
mod profiles;
mod reach;
mod reload;
#[cfg(all(test, target_os = "linux"))]
mod reload_native_tests;
mod retention;
mod reuse;
mod viewport;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::core::{Runtime, RuntimeGuard};
use dioxus::desktop::wry::{WebContext, WebView, WebViewBuilder};
use dioxus::prelude::*;
use tsumugi_core::actions::Action;
use tsumugi_core::content::{BrowserId, Tab, WindowId};
use tsumugi_core::msg::Msg;
use tsumugi_model::EnvironmentId;

use crate::bridge::AppCtx;
use crate::components::{IconButton, InlineField};
use crate::icons::{Icon, IconView};

thread_local! {
    /// The address a page opened for one starts at, kept until its view is made.
    static START: RefCell<HashMap<BrowserId, String>> = RefCell::new(HashMap::new());
    /// The windows whose page was asked for a screenshot, until their loop
    /// takes the asking.
    static SHOTS: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
    static FOCUSED: Cell<bool> = const { Cell::new(false) };
}

/// The page of the window `window` is asked for a screenshot to keep.
pub(crate) fn ask_screenshot(window: WindowId) {
    SHOTS.with(|shots| shots.borrow_mut().push(window));
}

fn take_screenshot(window: WindowId) -> bool {
    SHOTS.with(|shots| {
        let mut shots = shots.borrow_mut();
        let asked = shots.contains(&window);
        shots.retain(|w| *w != window);
        asked
    })
}

pub(crate) use closed::{open_at, reopen};
/// The page of the window `window` is asked to open its search bar.
pub(crate) use find::ask as ask_find;

/// The page `id` goes to `url` once its view is made.
pub(crate) fn start_at(id: BrowserId, url: String) {
    START.with(|start| start.borrow_mut().insert(id, url));
}

pub(crate) fn has_focus() -> bool {
    FOCUSED.get()
}

/// What each page is called, by where it is: the tree and the tab bar show
/// it where the core, which knows no address, only says "Browser".
#[derive(Clone, Copy)]
struct Titles(Signal<HashMap<BrowserId, String>>);

/// The pages freed to save memory, which the tree marks.
#[derive(Clone, Copy)]
struct Freed(Signal<std::collections::HashSet<BrowserId>>);

/// Whether `tab` is a browser page freed to save memory.
pub(crate) fn is_freed(tab: &Tab) -> bool {
    let Tab::Browser { id, .. } = tab else {
        return false;
    };
    try_consume_context::<Freed>().is_some_and(|freed| freed.0.read().contains(id))
}

/// What `tab` is called when it is a browser page that has loaded something.
pub(crate) fn title(tab: &Tab) -> Option<String> {
    let Tab::Browser { id, .. } = tab else {
        return None;
    };
    try_consume_context::<Titles>().and_then(|titles| titles.0.read().get(id).cloned())
}

/// What a page at `url` is called: its site, without `www.`.
fn name_of(url: &str) -> String {
    let host = url
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default())
        .and_then(|authority| authority.rsplit('@').next())
        .map(|host| host.strip_prefix("www.").unwrap_or(host))
        .filter(|host| !host.is_empty());
    host.map_or_else(|| "New page".into(), str::to_string)
}

/// The numbers the page draws on the elements picked follow the notes.
fn sync_marks(page: &Page) {
    let keys = page
        .notes
        .borrow()
        .iter()
        .map(|note| note.key.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let _ = page
        .view
        .evaluate_script(&format!("window.tsumugiInspector?.marks([{keys}])"));
}

fn address(typed: &str) -> Result<String, &'static str> {
    let typed = typed.trim();
    if typed.is_empty() || typed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Enter a web address");
    }
    let lower = typed.to_ascii_lowercase();
    let url = if lower.starts_with("https://") || lower.starts_with("http://") {
        typed.to_string()
    } else if lower.starts_with("localhost:")
        || lower.starts_with("127.0.0.1:")
        || lower.starts_with("[::1]:")
        || lower == "localhost"
        || typed.split(['/', '?', '#']).next().is_some_and(|host| {
            host.rsplit_once(':').is_some_and(|(host, port)| {
                !host.is_empty() && !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())
            })
        })
    {
        format!("http://{typed}")
    } else if typed.contains(':') {
        return Err("Only HTTP and HTTPS addresses can be opened");
    } else {
        format!("https://{typed}")
    };
    let authority = url
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default())
        .unwrap_or_default();
    if authority.is_empty() || authority.starts_with(':') || authority.contains('\\') {
        return Err("Enter a web address with a host");
    }
    Ok(url)
}

/// `url` with a loopback address's port swapped for the port of this
/// computer `forward` makes reach it: the loopback of an environment on a
/// work machine is that machine's, which a page here cannot reach.
fn through_forward(
    url: &str,
    forward: impl FnOnce(u16) -> Result<u16, String>,
) -> Result<String, String> {
    let Some((scheme, rest)) = url.split_once("://") else {
        return Ok(url.to_string());
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, after) = rest.split_at(end);
    let Some((host, port)) = authority.rsplit_once(':') else {
        return Ok(url.to_string());
    };
    let loopback = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "[::1]";
    let Some(port) = port.parse::<u16>().ok().filter(|_| loopback) else {
        return Ok(url.to_string());
    };
    let local = forward(port)?;
    Ok(format!("{scheme}://{host}:{local}{after}"))
}

/// `url` as the environment itself writes it: a loopback port that a forward
/// made here is swapped back for the port of the work machine it reaches, so
/// that the address shown goes through the forward again when entered.
fn shown_address(url: &str, work_port: impl FnOnce(u16) -> Option<u16>) -> String {
    through_forward(url, |local| Ok(work_port(local).unwrap_or(local)))
        .unwrap_or_else(|_| url.to_string())
}

/// Where a page goes for `typed`, an address typed or one given whole: the
/// address of a web page, with a loopback port swapped for the one `forward`
/// makes reach the environment's own loopback. Every way to point a page at an
/// address comes here, so none reaches this computer's loopback by mistake.
fn target(typed: &str, forward: impl FnOnce(u16) -> Result<u16, String>) -> Result<String, String> {
    address(typed)
        .map_err(str::to_string)
        .and_then(|url| through_forward(&url, forward))
}

/// Load `page` again. A page that went through a forward goes through one
/// made again, as the one it was loaded through went with the connection that
/// made it; any other is reloaded as it is, past the cache when `hard`.
fn reload_page(app: AppCtx, page: &Page, mut error: Signal<String>, hard: bool) {
    let home = page.home.borrow().clone();
    let url = page.view.url().unwrap_or_default();
    let blank = url.is_empty() || url == "about:blank";
    let reach::Reload::Through(home) = reach::reload(&home, blank) else {
        if hard {
            reload::hard(&page.view);
        } else {
            let _ = page.view.reload();
        }
        return;
    };
    match target(home, |port| app.forward_port(&page.environment, port)) {
        Ok(url) => match page.view.load_url(&url) {
            Ok(()) => error.set(String::new()),
            Err(e) => error.set(e.to_string()),
        },
        Err(message) => error.set(message),
    }
}

fn allowed(url: String) -> bool {
    url == "about:blank" || address(&url).is_ok_and(|a| a == url)
}

fn set_bounds(page: &Page, rect: [f64; 4]) {
    #[cfg(target_os = "linux")]
    page.child.set_bounds(rect);
    #[cfg(not(target_os = "linux"))]
    {
        let [x, y, w, h] = rect;
        let _ = page.view.set_bounds(dioxus::desktop::wry::Rect {
            position: dioxus::desktop::LogicalPosition::new(x, y).into(),
            size: dioxus::desktop::LogicalSize::new(w, h).into(),
        });
    }
}

/// Whether this computer's GStreamer can play sound. WebKitGTK aborts its
/// whole page process when a page starts media and no audio sink exists,
/// which Google's search results do, so such a computer browses without
/// media.
#[cfg(target_os = "linux")]
fn media_would_abort() -> bool {
    let dirs = std::env::var("GST_PLUGIN_SYSTEM_PATH_1_0")
        .or_else(|_| std::env::var("GST_PLUGIN_SYSTEM_PATH"))
        .map(|paths| paths.split(':').map(PathBuf::from).collect::<Vec<_>>())
        .unwrap_or_else(|_| {
            [
                "/usr/lib/gstreamer-1.0",
                "/usr/lib64/gstreamer-1.0",
                "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
                "/usr/lib/aarch64-linux-gnu/gstreamer-1.0",
                "/usr/local/lib/gstreamer-1.0",
            ]
            .map(PathBuf::from)
            .to_vec()
        });
    !dirs
        .iter()
        .any(|dir| dir.join("libgstautodetect.so").exists())
}

/// Turn off media in `view`.
#[cfg(target_os = "linux")]
fn without_media(view: &WebView) {
    use dioxus::desktop::wry::WebViewExtUnix;
    use gtk::glib::object::ObjectType;
    // The WebKit dependency is owned by Wry; these stable C entry points
    // avoid a second binding just for two settings.
    unsafe extern "C" {
        fn webkit_web_view_get_settings(
            view: *mut gtk::glib::gobject_ffi::GObject,
        ) -> *mut std::ffi::c_void;
        fn webkit_settings_set_enable_media(settings: *mut std::ffi::c_void, on: i32);
        fn webkit_settings_set_enable_webaudio(settings: *mut std::ffi::c_void, on: i32);
    }
    let webview = view.webview();
    // SAFETY: the view is alive, and the settings it returns are borrowed
    // from it only for these calls.
    unsafe {
        let settings = webkit_web_view_get_settings(webview.as_ptr().cast());
        if !settings.is_null() {
            webkit_settings_set_enable_media(settings, 0);
            webkit_settings_set_enable_webaudio(settings, 0);
        }
    }
}

fn set_visible(view: &WebView, on: bool) {
    #[cfg(target_os = "linux")]
    {
        use dioxus::desktop::wry::WebViewExtUnix;
        use gtk::prelude::*;
        if let Some(parent) = view.webview().parent() {
            parent.set_visible(on);
        }
    }
    let _ = view.set_visible(on);
}

struct Page {
    environment: EnvironmentId,
    view: WebView,
    /// Whether the user has typed into the page since it loaded; such a page
    /// is not freed.
    edited: Rc<Cell<bool>>,
    /// Where the page is scrolled to, as the page last said.
    scroll: Rc<Cell<[f64; 2]>>,
    /// Where to scroll the page once it has loaded, after it was freed.
    restore: Cell<Option<[f64; 2]>>,
    /// The address the page shows as its environment writes it, kept while
    /// the page is away from its work machine and for reloading it through
    /// a forward made again.
    home: RefCell<String>,
    ports: RefCell<reach::Ports>,
    /// How many loads of this page have finished, which a command that
    /// points it elsewhere waits on.
    loads: Rc<Cell<u64>>,
    pending: Arc<Mutex<Option<String>>>,
    selection: Rc<RefCell<annotations::Selection>>,
    notes: Rc<RefCell<Vec<annotations::Note>>>,
    /// What the page was asked to download, which the shelf lists.
    downloads: Rc<RefCell<downloads::Shelf>>,
    next_note: Cell<u64>,
    // WebKit's context must outlive every view built with it.
    _context: Rc<RefCell<WebContext>>,
    #[cfg(target_os = "linux")]
    child: crate::native_layer::Child,
}

/// What the pages of one environment share: the engine's context, which
/// keeps their website data, and the downloads they were asked for.
struct Site {
    context: Rc<RefCell<WebContext>>,
    downloads: Rc<RefCell<downloads::Shelf>>,
    /// Whether a page of the context records its downloads already.
    recording: Cell<bool>,
}

#[derive(Default)]
struct Pool {
    pages: HashMap<BrowserId, Rc<Page>>,
    contexts: HashMap<(EnvironmentId, Option<String>), Site>,
    #[cfg(target_os = "linux")]
    layer: Option<Rc<crate::native_layer::Layer>>,
}

#[derive(Clone, Default)]
struct Browsers(Rc<RefCell<Pool>>);

impl Pool {
    fn page(
        &mut self,
        browser: BrowserId,
        environment: &EnvironmentId,
        app: AppCtx,
    ) -> Result<Rc<Page>, String> {
        if let Some(page) = self.pages.get(&browser) {
            return Ok(page.clone());
        }
        let desktop = dioxus::desktop::window();
        let profile = profiles::chosen(browser);
        let directory = profiles::directory(
            &crate::paths::data_dir().join("tsumugi/browser"),
            environment,
            profile.as_deref(),
        );
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let site = self
            .contexts
            .entry((environment.clone(), profile.clone()))
            .or_insert_with(|| Site {
                context: Rc::new(RefCell::new(WebContext::new(Some(directory)))),
                downloads: Default::default(),
                recording: Cell::new(false),
            });
        let context = site.context.clone();
        let downloads = site.downloads.clone();
        // WebKitGTK takes download handlers on the context the pages share,
        // so one page of a context records for all of them; the other
        // engines take them on the page.
        let records = !cfg!(target_os = "linux") || !site.recording.replace(true);
        let mut context_ref = context.borrow_mut();
        let return_to = desktop.clone();
        // Native WebView callbacks run outside Dioxus's event dispatch.
        let runtime = Runtime::current();
        let pending = Arc::new(Mutex::new(None));
        let new_window = pending.clone();
        let selection = Rc::new(RefCell::new(annotations::Selection::default()));
        let receive = selection.clone();
        let loading = selection.clone();
        let loads = Rc::new(Cell::new(0));
        let loaded = loads.clone();
        let edited = Rc::new(Cell::new(false));
        let (edit_seen, edit_reset) = (edited.clone(), edited.clone());
        let scroll = Rc::new(Cell::new([0.0, 0.0]));
        let scroll_seen = scroll.clone();
        // A page freed to save memory loads again where it was, through a
        // forward made again; while its work machine is away it waits blank
        // for the retry.
        let saved = retention::take(browser);
        let start = saved
            .as_ref()
            .and_then(|saved| {
                target(&saved.address, |port| app.forward_port(environment, port)).ok()
            })
            .unwrap_or_else(|| "about:blank".into());
        let download_started = downloads.clone();
        let download_finished = downloads.clone();
        let builder = WebViewBuilder::new_with_web_context(&mut context_ref)
            .with_url(start)
            .with_visible(false)
            .with_navigation_handler(allowed)
            .with_new_window_req_handler(move |url, _| {
                if allowed(url.clone()) {
                    *new_window.lock().expect("browser navigation lock") = Some(url);
                }
                dioxus::desktop::wry::NewWindowResponse::Deny
            })
            .with_initialization_script(include_str!("../assets/js/browser.js"))
            .with_initialization_script(include_str!("../assets/js/browser-annotate.js"))
            .with_initialization_script(find::SCRIPT)
            .with_initialization_script(include_str!("../assets/js/browser-agent.js"))
            .with_on_page_load_handler(move |event, _| {
                // The picking goes on across pages; what was picked on the
                // last one has gone with it.
                match event {
                    dioxus::desktop::wry::PageLoadEvent::Started => {
                        loading.borrow_mut().pending.clear();
                        edit_reset.set(false);
                    }
                    dioxus::desktop::wry::PageLoadEvent::Finished => loaded.set(loaded.get() + 1),
                }
            })
            .with_ipc_handler(move |request| {
                let _guard = RuntimeGuard::new(runtime.clone());
                // A page kept hidden runs its scripts too, so it is the page shown
                // that the keys and focus answer to, in whichever window shows it.
                let shown = app.content.peek().window_of_page(browser);
                match (request.body().as_str(), shown) {
                    ("focus" | "workspace" | "address" | "find" | "reopen", None) => {}
                    ("focus", Some(window)) => {
                        FOCUSED.set(true);
                        app.dispatch(Msg::WindowFocus(window));
                        app.dispatch(Msg::Focus(tsumugi_core::content::Area::Console));
                    }
                    ("find" | "reopen", Some(_)) => {
                        FOCUSED.set(false);
                        let _ = return_to.webview.focus();
                        app.perform(
                            if request.body() == "find" {
                                Action::BrowserFind
                            } else {
                                Action::BrowserReopen
                            },
                            None,
                        );
                    }
                    ("blur", _) => FOCUSED.set(false),
                    ("workspace", Some(_)) => {
                        FOCUSED.set(false);
                        let _ = return_to.webview.focus();
                    }
                    ("address", Some(window)) => {
                        FOCUSED.set(false);
                        let _ = return_to.webview.focus();
                        let _ = return_to.webview.evaluate_script(&format!(
                            "document.querySelector('#browser-toolbar-{window} input')?.focus()"
                        ));
                    }
                    ("annotation-cancel", _) => receive.borrow_mut().stop(),
                    ("edited", _) => edit_seen.set(true),
                    (message, _) if message.starts_with("scroll:") => {
                        if let Some(at) = retention::scrolled(message) {
                            scroll_seen.set(at);
                        }
                    }
                    (message, _) => {
                        receive.borrow_mut().receive(message);
                    }
                }
            });
        let builder = if records {
            builder
                .with_download_started_handler(move |url, path| {
                    download_started
                        .borrow_mut()
                        .started(url, path, |path| path.exists());
                    true
                })
                .with_download_completed_handler(move |url, path, ok| {
                    download_finished.borrow_mut().finished(&url, path, ok);
                })
        } else {
            builder
        };
        #[cfg(target_os = "macos")]
        let builder = {
            use dioxus::desktop::wry::WebViewBuilderExtDarwin;
            use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
            let digest = profiles::digest(environment, profile.as_deref());
            let persistent = NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
                NSOperatingSystemVersion {
                    majorVersion: 14,
                    minorVersion: 0,
                    patchVersion: 0,
                },
            );
            builder
                .with_incognito(!persistent)
                .with_data_store_identifier(digest[..16].try_into().expect("SHA-256 has 32 bytes"))
        };
        #[cfg(target_os = "linux")]
        let (view, child) = {
            use dioxus::desktop::wry::WebViewBuilderExtUnix;
            let layer = match &self.layer {
                Some(layer) => layer.clone(),
                None => {
                    let layer = Rc::new(crate::native_layer::Layer::install(&desktop)?);
                    self.layer = Some(layer.clone());
                    layer
                }
            };
            let child = layer.interactive_child();
            (builder.build_gtk(&child.widget), child)
        };
        #[cfg(not(target_os = "linux"))]
        let view = builder.build_as_child(desktop.window.as_ref());
        let view = view.map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        if media_would_abort() {
            without_media(&view);
        }
        drop(context_ref);
        let page = Rc::new(Page {
            environment: environment.clone(),
            view,
            edited,
            scroll,
            restore: Cell::new(saved.as_ref().map(|saved| saved.scroll)),
            home: RefCell::new(saved.map(|saved| saved.address).unwrap_or_default()),
            ports: RefCell::default(),
            loads,
            pending,
            selection,
            notes: Default::default(),
            downloads,
            next_note: Cell::new(1),
            _context: context,
            #[cfg(target_os = "linux")]
            child,
        });
        self.pages.insert(browser, page.clone());
        Ok(page)
    }
}

pub(crate) use agent::enqueue;

pub(crate) fn use_browsers(app: AppCtx) {
    let browsers = use_context_provider(Browsers::default);
    use_context_provider(|| Titles(Signal::new(HashMap::new())));
    let mut freed = use_context_provider(|| Freed(Signal::new(Default::default()))).0;
    use_context_provider(|| Moves(Signal::new(0)));
    use_hook(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                // A page stays open with its worktree's layout put away, and is
                // told apart by its own identity: layouts number their windows
                // each from 1.
                let listed = app.content.peek().pages();
                closed::track(&listed);
                let open: Vec<BrowserId> = listed.iter().map(|(_, page)| page.id).collect();
                let mut pool = browsers.0.borrow_mut();
                let gone: Vec<BrowserId> = pool
                    .pages
                    .keys()
                    .filter(|id| !open.contains(id))
                    .copied()
                    .collect();
                for id in gone {
                    retention::forget(id);
                    if let Some(page) = pool.pages.remove(&id) {
                        reuse::forget(id);
                        let url = page.view.url().unwrap_or_default();
                        let address = page
                            .ports
                            .borrow_mut()
                            .shown(&url, |local| app.work_port(&page.environment, local));
                        closed::closing_at(id, address);
                    }
                }
                // A page closed while it was freed can still be reopened.
                for id in retention::freed()
                    .into_iter()
                    .filter(|id| !open.contains(id))
                {
                    if let Some(address) = retention::forget(id) {
                        closed::closing_at(id, address);
                    }
                }
                retention::sweep(app, &mut pool, &open);
                drop(pool);
                let now = retention::freed();
                if *freed.peek() != now {
                    freed.set(now);
                }
                for (environment, call) in agent::take() {
                    let browsers = browsers.clone();
                    spawn(agent::run(app, browsers, environment, call));
                }
            }
        })
    });
}

/// Capture the visible part of `page` and keep it among the resources of
/// the project holding `worktree`, with the page's address as its note.
async fn keep_screenshot(app: AppCtx, page: &Page, environment: &EnvironmentId, worktree: PathBuf) {
    use base64::Engine;
    let url = page.view.url().unwrap_or_default();
    if url.is_empty() || url == "about:blank" {
        app.notice("Open a page to take a screenshot of".into());
        return;
    }
    let png = match capture::page(&page.view, &url).await {
        Ok((_, png)) => png,
        Err(message) => {
            app.notice(message);
            return;
        }
    };
    let answer = app
        .ask_in(
            environment,
            tsumugi_protocol::Ask::KeepScreenshot {
                dir: worktree,
                data: base64::engine::general_purpose::STANDARD.encode(png),
                label: Some(format!("Screenshot of {}", name_of(&url))),
                note: Some(url),
            },
        )
        .await;
    match answer {
        tsumugi_protocol::Answer::Done => app.notice("Kept the screenshot in resources".into()),
        tsumugi_protocol::Answer::Failed { text }
        | tsumugi_protocol::Answer::Unconfirmed { text } => app.notice(text),
        answer => app.notice(format!("the work host answered {answer:?}")),
    }
}

/// How many times a page was moved to another profile, which builds its view
/// again: the engine fixes a view's website data when it is made.
#[derive(Clone, Copy)]
struct Moves(Signal<u64>);

/// The page `id`, shown by its own view in the profile it is in.
#[component]
pub(crate) fn BrowserWindow(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    id: BrowserId,
) -> Element {
    // A moved page is a new view; the old one goes with its component.
    let _ = use_context::<Moves>().0();
    let profile = profiles::chosen(id).unwrap_or_default();
    rsx! { BrowserView { key: "{profile}", window, environment, worktree, id } }
}

#[component]
fn BrowserView(
    window: WindowId,
    environment: EnvironmentId,
    worktree: PathBuf,
    id: BrowserId,
) -> Element {
    let app = use_context::<AppCtx>();
    let prompts = use_context::<crate::shell::Ui>().consoles.prompts;
    let browsers = use_context::<Browsers>();
    let mut titles = use_context::<Titles>().0;
    let page_environment = environment.clone();
    let building = browsers.clone();
    let page = use_hook(move || building.0.borrow_mut().page(id, &page_environment, app));
    let mut searching = find::use_find();
    let mut size = use_signal(move || viewport::chosen(id).unwrap_or("responsive"));
    // What was typed and not entered stays with the page, so that putting
    // the page away or freeing it does not lose it.
    let mut typed = use_signal(move || retention::typed(id).unwrap_or_default());
    let mut current = use_signal(String::new);
    let mut error = use_signal(String::new);
    let mut route = use_signal(|| reach::Route::Here);
    let mut shelf = use_signal(Vec::<downloads::Row>::new);
    let moves = use_context::<Moves>().0;
    let mut naming = use_signal(|| false);
    let mut profile_typed = use_signal(String::new);
    let mut inspecting = use_signal(|| false);
    let mut grabbing = use_signal(|| false);
    // The element picked and waiting for its instruction, and the page
    // being drawn on: either keeps a still of the page where the page
    // itself, a native view, cannot be drawn over.
    let mut draft = use_signal(|| None::<Rc<card::Draft>>);
    let mut drawing = use_signal(|| None::<Rc<markup::Still>>);
    let sending = feedback::Sending {
        busy: use_signal(|| false),
        error: use_signal(String::new),
        notes: use_signal(|| {
            page.as_ref()
                .map(|page| page.notes.borrow().clone())
                .unwrap_or_default()
        }),
        chosen: use_signal(String::new),
    };
    let page = match page {
        Ok(page) => page,
        Err(message) => {
            return rsx! { div { class: "browser-error", "Browser could not start: {message}" } };
        }
    };
    // Shown from the page freed until it has loaded again where it was.
    let mut restoring = use_signal({
        let page = page.clone();
        move || page.restore.get().is_some()
    });
    use_hook({
        let page = page.clone();
        let starting = environment.clone();
        move || {
            if let Some(url) = START.with(|start| start.borrow_mut().remove(&id)) {
                match target(&url, |port| app.forward_port(&starting, port)) {
                    Ok(url) => {
                        reuse::at(
                            id,
                            shown_address(&url, |local| app.work_port(&starting, local)),
                        );
                        *page.pending.lock().expect("browser navigation lock") = Some(url);
                    }
                    Err(message) => error.set(message),
                }
            }
        }
    });
    use_drop({
        let page = page.clone();
        move || {
            page.selection.borrow_mut().stop();
            let _ = page
                .view
                .evaluate_script("window.tsumugiInspector?.set(false)");
            set_visible(&page.view, false);
            FOCUSED.set(false);
        }
    });
    let (shot_environment, shot_worktree) = (environment.clone(), worktree.clone());
    let listed = environment.clone();
    use_hook({
        let page = page.clone();
        move || {
            spawn(async move {
                let mut bounds = None;
                let mut ticks = 0u32;
                let mut keeps_typed = retention::typed(id).is_some();
                let mut downloads_seen = 0;
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    ticks = ticks.wrapping_add(1);
                    let (armed, grab) = {
                        let selection = page.selection.borrow();
                        (selection.armed, selection.grab)
                    };
                    if *inspecting.peek() != (armed && !grab) {
                        inspecting.set(armed && !grab);
                    }
                    if *grabbing.peek() != (armed && grab) {
                        grabbing.set(armed && grab);
                    }
                    // A page that has just loaded starts without the picker.
                    if armed && ticks.is_multiple_of(10) {
                        let _ = page
                            .view
                            .evaluate_script("window.tsumugiInspector?.set(true)");
                    }
                    // One element at a time waits for its instruction.
                    let selected = if draft.peek().is_none() {
                        page.selection.borrow_mut().pending.pop_front()
                    } else {
                        None
                    };
                    if let Some(annotations::Pick { key, element }) = selected {
                        // Capture before the page changes under the tray, and
                        // without the markers of earlier picks.
                        let _ = page
                            .view
                            .evaluate_script("window.tsumugiInspector?.cover(true)");
                        tokio::time::sleep(Duration::from_millis(60)).await;
                        let shot = capture::page(&page.view, &element.url).await;
                        let _ = page
                            .view
                            .evaluate_script("window.tsumugiInspector?.cover(false)");
                        let (image, image_error, still) = match shot {
                            Ok((whole, png)) => {
                                use base64::Engine;
                                let (image, image_error) = match capture::crop(&whole, &element) {
                                    Ok(image) => (Some(image), None),
                                    Err(error) => (None, Some(error)),
                                };
                                (
                                    image,
                                    image_error,
                                    Some(base64::engine::general_purpose::STANDARD.encode(png)),
                                )
                            }
                            Err(error) => (None, Some(error), None),
                        };
                        if grab {
                            let note = page.next_note.get();
                            page.next_note.set(note + 1);
                            page.notes.borrow_mut().push(annotations::Note {
                                id: note,
                                key,
                                element,
                                comment: String::new(),
                                intent: annotations::Intent::Change,
                                grab: true,
                                image,
                                image_error,
                            });
                            sync_marks(&page);
                            page.selection.borrow_mut().stop();
                            let _ = page
                                .view
                                .evaluate_script("window.tsumugiInspector?.set(false)");
                        } else {
                            draft.set(Some(Rc::new(card::Draft {
                                key,
                                element,
                                image,
                                image_error,
                                still,
                            })));
                        }
                        FOCUSED.set(false);
                        let _ = dioxus::desktop::window().webview.focus();
                    }
                    // An agent can choose the size of the page too.
                    let chosen = viewport::chosen(id).unwrap_or("responsive");
                    if *size.peek() != chosen {
                        size.set(chosen);
                    }
                    if let Some(rows) = page.downloads.borrow().since(&mut downloads_seen) {
                        shelf.set(rows);
                    }
                    if find::take(window) {
                        searching.open.set(true);
                        searching.asked += 1;
                    }
                    if take_screenshot(window) {
                        let (page, environment, worktree) = (
                            page.clone(),
                            shot_environment.clone(),
                            shot_worktree.clone(),
                        );
                        spawn(async move {
                            keep_screenshot(app, &page, &environment, worktree).await;
                        });
                    }
                    let pending = page.pending.lock().expect("browser navigation lock").take();
                    if let Some(url) = pending {
                        let _ = page.view.load_url(&url);
                    }
                    let shown_url = page.view.url().unwrap_or_default();
                    let blank = shown_url.is_empty() || shown_url == "about:blank";
                    // A page freed to save memory is scrolled back once it has
                    // loaded where it was.
                    if let Some(at) = page.restore.get()
                        && !blank
                        && page.loads.get() > 0
                    {
                        page.restore.set(None);
                        restoring.set(false);
                        let _ = page
                            .view
                            .evaluate_script(&format!("window.scrollTo({}, {})", at[0], at[1]));
                    }
                    // What the page shows is the environment's own address, kept
                    // while its work machine is away, as is how it is reached.
                    if !blank {
                        let address = page
                            .ports
                            .borrow_mut()
                            .shown(&shown_url, |local| app.work_port(&listed, local));
                        if *page.home.borrow() != address {
                            *page.home.borrow_mut() = address;
                        }
                    }
                    let reached =
                        reach::route(app.remote_status_of(&listed).as_ref(), &page.home.borrow());
                    if *route.peek() != reached {
                        let was_away = route.peek().is_away();
                        route.set(reached.clone());
                        // The forward the page was loaded through went with the
                        // connection: once it is back, load the page through one
                        // made again.
                        if was_away && !reached.is_away() {
                            reload_page(app, &page, error, false);
                        }
                    }
                    if !app.held().is_empty()
                        || app.content.peek().moving.is_some()
                        || app.content.peek().drag.is_some()
                        || app.chrome.peek().panels.drag.is_some()
                        || draft.peek().is_some()
                        || drawing.peek().is_some()
                        || route.peek().is_away()
                    {
                        set_visible(&page.view, false);
                        FOCUSED.set(false);
                        continue;
                    }
                    let mut query = document::eval(&format!(
                        "const e=document.getElementById('browser-{window}');const r=e?.getBoundingClientRect();dioxus.send(r ? [r.x,r.y,r.width,r.height] : null);"
                    ));
                    let rect = query.recv::<Option<[f64; 4]>>().await.ok().flatten();
                    if !app.held().is_empty() || draft.peek().is_some() || drawing.peek().is_some()
                    {
                        set_visible(&page.view, false);
                        continue;
                    }
                    if let Some(rect) =
                        rect.filter(|r| r.iter().all(|v| v.is_finite()) && r[2] > 1.0 && r[3] > 1.0)
                    {
                        let zoom = crate::scaling::current();
                        let rect = rect.map(|v| v * zoom);
                        let rect =
                            viewport::fit(rect, viewport::chosen(id).and_then(viewport::preset));
                        if bounds != Some(rect) {
                            set_bounds(&page, rect);
                            bounds = Some(rect);
                        }
                        set_visible(&page.view, true);
                    } else {
                        set_visible(&page.view, false);
                    }
                    if let Ok(url) = page.view.url()
                        && url != "about:blank"
                        && *current.peek() != url
                    {
                        let address = page
                            .ports
                            .borrow_mut()
                            .shown(&url, |local| app.work_port(&listed, local));
                        titles.write().insert(id, name_of(&address));
                        reuse::at(id, address.clone());
                        // The page the address was typed for is the one shown
                        // for the first time since it was put away: what was
                        // typed is not replaced by where the page is.
                        if !std::mem::take(&mut keeps_typed) {
                            typed.set(address);
                            retention::type_into(id, None);
                        }
                        current.set(url);
                    }
                }
            })
        }
    });
    let navigate = {
        let page = page.clone();
        let navigating = environment.clone();
        move || {
            let entered = typed.peek().clone();
            match target(&entered, |port| app.forward_port(&navigating, port)) {
                Ok(url) => match page.view.load_url(&url) {
                    Ok(()) => {
                        error.set(String::new());
                        retention::type_into(id, None);
                        typed.set(shown_address(&url, |local| {
                            app.work_port(&navigating, local)
                        }));
                    }
                    Err(e) => error.set(e.to_string()),
                },
                Err(e) => error.set(e),
            }
        }
    };
    let shelves = page.clone();
    // Move the page to `to`, the default profile for none, at the address it
    // is at: its website data is the profile's, so its view is made again.
    let move_to = {
        let page = page.clone();
        let moving = environment.clone();
        Rc::new(move |to: Option<String>| {
            let url = page.view.url().unwrap_or_default();
            if !url.is_empty() && url != "about:blank" {
                start_at(
                    id,
                    shown_address(&url, |local| app.work_port(&moving, local)),
                );
            }
            profiles::choose(id, to);
            browsers.0.borrow_mut().pages.remove(&id);
            let mut moves = moves;
            moves += 1;
        })
    };
    let in_profile = profiles::chosen(id);
    let made = profiles::made();
    let (picked, named) = (move_to.clone(), move_to.clone());
    let back = page.clone();
    let forward = page.clone();
    let reload = page.clone();
    let retry = page.clone();
    let retrying = environment.clone();
    let annotate = page.clone();
    let grab = page.clone();
    let cancel = page.clone();
    let draw = page.clone();
    let add = page.clone();
    let finish = page.clone();
    let withdraw = page.clone();
    let picks = move |page: &Page, grabbing: bool, on: bool| {
        if on && page.notes.borrow().len() >= 20 {
            return Err("Remove an annotation before selecting more elements");
        }
        {
            let mut selection = page.selection.borrow_mut();
            if on {
                selection.armed = true;
                selection.grab = grabbing;
            } else {
                selection.stop();
            }
        }
        let _ = page
            .view
            .evaluate_script(&format!("window.tsumugiInspector?.set({on})"));
        Ok(())
    };
    let hint = if *grabbing.read() {
        "Click an element to add it to the prompt as it is."
    } else if sending.notes.read().is_empty() {
        "Click an element to add feedback for the agent."
    } else {
        "Select another element, or send the annotations."
    };
    let picking = inspecting() || grabbing();
    let (add_environment, add_worktree) = (environment.clone(), worktree.clone());
    rsx! {
        div { class: "browser",
            div { id: "browser-toolbar-{window}", class: "browser-toolbar",
                IconButton { icon: Icon::SquareArrowLeft, title: "Back", onclick: move |_| { let _ = back.view.evaluate_script("history.back()"); } }
                IconButton { icon: Icon::SquareArrowRight, title: "Forward", onclick: move |_| { let _ = forward.view.evaluate_script("history.forward()"); } }
                IconButton {
                    icon: Icon::Refresh,
                    title: "Reload (Shift-click or right-click: past the cache)",
                    onmousedown: {
                        let page = reload.clone();
                        move |e: MouseEvent| {
                            if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Secondary) {
                                reload_page(app, &page, error, true);
                            }
                        }
                    },
                    onclick: move |e: MouseEvent| reload_page(app, &reload, error, e.modifiers().shift()),
                }
                if let reach::Route::Through { host, connected } = route() {
                    span {
                        class: if connected { "browser-host" } else { "browser-host offline" },
                        title: if connected { "Browsing through {host}" } else { "{host} is not connected" },
                        IconView { icon: Icon::Server, size: 15 }
                    }
                }
                InlineField {
                    class: "browser-address", aria_label: "Web address", placeholder: "Enter a URL, e.g. localhost:3000", value: typed(),
                    focus: true, contained: true,
                    oninput: move |value: String| {
                        retention::type_into(id, Some(value.clone()));
                        typed.set(value);
                    },
                    onenter: move |_| navigate.clone()(),
                }
                select {
                    class: "browser-profile", aria_label: "Profile",
                    title: "The profile whose cookies and website data the page uses",
                    value: in_profile.as_ref().map_or_else(|| "d".to_string(), |name| format!("p:{name}")),
                    onchange: move |e| match e.value().as_str() {
                        "n" => { profile_typed.set(String::new()); naming.set(true); }
                        "d" => picked(None),
                        other => picked(other.strip_prefix("p:").map(str::to_string)),
                    },
                    option { value: "d", "{profiles::DEFAULT}" }
                    for name in made {
                        option { value: "p:{name}", "{name}" }
                    }
                    option { value: "n", "New profile…" }
                }
                if naming() {
                    InlineField {
                        class: "browser-profile-name", aria_label: "Profile name", placeholder: "Profile name", value: profile_typed(),
                        focus: true, contained: true,
                        oninput: move |value| profile_typed.set(value),
                        onescape: move |_| naming.set(false),
                        onenter: move |_| match profiles::make(&profile_typed.peek()) {
                            Ok(name) => { naming.set(false); error.set(String::new()); named(Some(name)); }
                            Err(message) => error.set(message),
                        },
                    }
                }
                select {
                    class: "browser-viewport", aria_label: "Page size",
                    title: "Show the page at the size of a device",
                    value: size(),
                    onchange: move |e| {
                        viewport::choose(id, &e.value());
                        size.set(viewport::chosen(id).unwrap_or("responsive"));
                    },
                    option { value: "responsive", "Responsive" }
                    for preset in viewport::PRESETS {
                        option { value: preset.id, "{preset.label}" }
                    }
                }
                div { class: "browser-tools", role: "group", aria_label: "Page tools",
                    button {
                        class: if inspecting() { "tool active" } else { "tool" },
                        r#type: "button", aria_pressed: inspecting(),
                        title: "Annotate: pick page elements and write what to change for each (Escape finishes)",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| {
                            let on = !*inspecting.peek();
                            match picks(&annotate, false, on) {
                                Ok(()) => { error.set(String::new()); inspecting.set(on); grabbing.set(false); }
                                Err(message) => error.set(message.into()),
                            }
                        },
                        IconView { icon: Icon::Crosshair, size: 15 }
                        span { "Annotate" }
                    }
                    button {
                        class: if grabbing() { "tool active" } else { "tool" },
                        r#type: "button", aria_pressed: grabbing(),
                        title: "Grab: pick one element and add it to the prompt as it is",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| {
                            let on = !*grabbing.peek();
                            match picks(&grab, true, on) {
                                Ok(()) => { error.set(String::new()); grabbing.set(on); inspecting.set(false); }
                                Err(message) => error.set(message.into()),
                            }
                        },
                        IconView { icon: Icon::Pointer, size: 15 }
                        span { "Grab" }
                    }
                    button {
                        class: if drawing().is_some() { "tool active" } else { "tool" },
                        r#type: "button", aria_pressed: drawing().is_some(),
                        title: "Draw on a screenshot of the page and add it to the prompt",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| {
                            if drawing.peek().is_some() { return; }
                            let _ = picks(&draw, false, false);
                            inspecting.set(false);
                            grabbing.set(false);
                            let page = draw.clone();
                            spawn(async move {
                                let url = page.view.url().unwrap_or_default();
                                if url.is_empty() || url == "about:blank" {
                                    error.set("Open a page to draw on".into());
                                    return;
                                }
                                match capture::page(&page.view, &url).await {
                                    Ok((_, png)) => {
                                        use base64::Engine;
                                        error.set(String::new());
                                        drawing.set(Some(Rc::new(markup::Still {
                                            png: base64::engine::general_purpose::STANDARD.encode(png),
                                            url,
                                        })));
                                    }
                                    Err(message) => error.set(message),
                                }
                            });
                        },
                        IconView { icon: Icon::Pencil, size: 15 }
                        span { "Draw" }
                    }
                }
                span { class: "browser-hint", "Ctrl+Shift+Esc: workspace" }
            }
            if picking && draft().is_none() {
                div { class: "browser-banner",
                    IconView { icon: Icon::Crosshair, size: 13 }
                    span { class: "banner-text", "{hint}" }
                    button { class: "banner-cancel", r#type: "button",
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| {
                            let _ = picks(&cancel, false, false);
                            inspecting.set(false);
                            grabbing.set(false);
                        },
                        "Cancel"
                    }
                }
            }
            if *searching.open.read() {
                find::Bar { window, page: find::Searched(page.clone()), find: searching }
            }
            if !error().is_empty() { div { class: "browser-error", "{error}" } }
            if !shelf.read().is_empty() {
                downloads::Rows {
                    rows: shelf(),
                    on_open: {
                        let page = shelves.clone();
                        move |id| {
                            if let Some(row) = page.downloads.borrow().row(id) {
                                crate::bridge::open([row.path.to_string_lossy().into_owned()]);
                            }
                        }
                    },
                    on_show: {
                        let page = shelves.clone();
                        move |id| {
                            if let Some(row) = page.downloads.borrow().row(id) {
                                crate::bridge::open(["-R".into(), row.path.to_string_lossy().into_owned()]);
                            }
                        }
                    },
                    on_dismiss: {
                        let page = shelves.clone();
                        move |id| page.downloads.borrow_mut().dismiss(id)
                    },
                }
            }
            if !sending.error.read().is_empty() && drawing().is_some() { div { class: "browser-error", "{sending.error}" } }
            if restoring() {
                div { class: "browser-notice",
                    IconView { icon: Icon::Refresh, size: 13 }
                    span { "Loading again a page that was freed to save memory" }
                }
            }
            div { id: "browser-{window}", class: "browser-page",
                if let reach::Route::Through { host, connected: false } = route() {
                    div { class: "browser-offline",
                        IconView { icon: Icon::Server, size: 22 }
                        div { class: "offline-title", "Can't reach the work machine" }
                        div { class: "offline-text", "This page loads through {host}, which isn't connected right now. It loads again when the connection returns." }
                        button { class: "button", r#type: "button",
                            onclick: move |_| {
                                app.retry_remote(&retrying);
                                reload_page(app, &retry, error, false);
                            },
                            "Retry"
                        }
                    }
                }
                if let Some(pending) = draft() {
                    card::Card {
                        draft: pending.clone(),
                        on_add: {
                            let page = add.clone();
                            let pending = pending.clone();
                            let mut notes = sending.notes;
                            move |(comment, intent): (String, annotations::Intent)| {
                                let note = page.next_note.get();
                                page.next_note.set(note + 1);
                                page.notes.borrow_mut().push(annotations::Note {
                                    id: note,
                                    key: pending.key,
                                    element: pending.element.clone(),
                                    comment,
                                    intent,
                                    grab: false,
                                    image: pending.image.clone(),
                                    image_error: pending.image_error.clone(),
                                });
                                notes.set(page.notes.borrow().clone());
                                sync_marks(&page);
                                draft.set(None);
                            }
                        },
                        on_cancel: {
                            let page = withdraw.clone();
                            move |()| {
                                // The page forgets an element that has no note.
                                sync_marks(&page);
                                draft.set(None);
                            }
                        },
                    }
                }
                if let Some(still) = drawing() {
                    markup::Markup {
                        window,
                        still: still.clone(),
                        busy: *sending.busy.read(),
                        on_cancel: move |()| drawing.set(None),
                        on_done: {
                            let page = finish.clone();
                            let environment = add_environment.clone();
                            let worktree = add_worktree.clone();
                            let still = still.clone();
                            move |png: Vec<u8>| {
                                let after: Box<dyn FnOnce()> = Box::new(move || drawing.set(None));
                                feedback::deliver(
                                    app,
                                    prompts,
                                    page.clone(),
                                    environment.clone(),
                                    worktree.clone(),
                                    sending,
                                    feedback::Delivery {
                                        text: format!(
                                            "The user drew on a screenshot of {}. The attached image shows the page with their marks; apply what the marks point at.",
                                            still.url
                                        ),
                                        images: vec![png],
                                        sent: Vec::new(),
                                        after: Some(after),
                                    },
                                );
                            }
                        },
                    }
                }
            }
            feedback::Feedback { window, browser: id, environment, worktree, state: sending }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_accept_web_pages_and_local_development_servers() {
        for (typed, expected) in [
            (" example.com/docs ", "https://example.com/docs"),
            ("localhost:3000/a", "http://localhost:3000/a"),
            ("127.0.0.1:8080", "http://127.0.0.1:8080"),
            ("[::1]:3000", "http://[::1]:3000"),
            ("dev.local:5173/a", "http://dev.local:5173/a"),
            ("https://example.com", "https://example.com"),
            ("http://localhost", "http://localhost"),
        ] {
            assert_eq!(address(typed).as_deref(), Ok(expected));
        }
    }

    #[test]
    fn a_loopback_address_goes_through_the_forward_and_any_other_stays_as_it_is() {
        let forwarded = |url: &str| through_forward(url, |port| Ok(port + 1000));
        for (url, want) in [
            (
                "http://localhost:3000/a?b=1#c",
                "http://localhost:4000/a?b=1#c",
            ),
            ("http://127.0.0.1:8080", "http://127.0.0.1:9080"),
            ("http://[::1]:5173/", "http://[::1]:6173/"),
            ("http://example.com:3000/", "http://example.com:3000/"),
            ("https://example.com/", "https://example.com/"),
            ("http://localhost/", "http://localhost/"),
        ] {
            assert_eq!(forwarded(url).as_deref(), Ok(want), "{url}");
        }
        assert_eq!(
            through_forward("http://localhost:3000", |_| Err("not connected".into())),
            Err("not connected".into())
        );
    }

    #[test]
    fn an_address_a_page_is_opened_at_reaches_the_work_machine_as_a_typed_one_does() {
        // The details panel and resources open pages at addresses already
        // whole; on a remote environment `localhost` there is the work
        // machine's, not this computer's.
        let remote = |url: &str| target(url, |port| Ok(port + 1000));
        assert_eq!(
            remote("http://localhost:3000/a").as_deref(),
            Ok("http://localhost:4000/a")
        );
        assert_eq!(
            remote("https://example.com/").as_deref(),
            Ok("https://example.com/")
        );
        assert!(remote("file:///etc/passwd").is_err());
        let here = |url: &str| target(url, Ok);
        assert_eq!(
            here("http://localhost:3000/").as_deref(),
            Ok("http://localhost:3000/")
        );
        assert_eq!(
            target("localhost:3000", |_| Err("not connected".into())),
            Err("not connected".into())
        );
    }

    #[test]
    fn a_forwarded_address_is_shown_as_the_work_machines_own() {
        // So that entering the address again forwards the work machine's port,
        // not a port of the work machine that happens to equal the local one.
        let shown = |url: &str| shown_address(url, |local| (local == 41000).then_some(3000));
        assert_eq!(
            shown("http://localhost:41000/a?b#c"),
            "http://localhost:3000/a?b#c"
        );
        for url in [
            "http://localhost:5173/",
            "http://example.com:41000/",
            "https://example.com/",
            "about:blank",
        ] {
            assert_eq!(shown(url), url, "{url}");
        }
    }

    #[test]
    fn a_page_is_called_by_its_site() {
        for (url, name) in [
            ("https://www.google.com/search?q=rust", "google.com"),
            ("http://localhost:3000/a#b", "localhost:3000"),
            ("https://user@example.com/", "example.com"),
            ("about:blank", "New page"),
            ("", "New page"),
        ] {
            assert_eq!(name_of(url), name, "{url}");
        }
    }

    #[test]
    fn an_address_cannot_run_script_or_open_the_clients_files() {
        for typed in [
            "",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "about:config",
            "https://",
            "https://a b",
            "https://a\nb",
        ] {
            assert!(address(typed).is_err(), "{typed}");
        }
    }
}
