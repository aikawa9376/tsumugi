//! Browsing belongs to the client window; an environment names its profile.

mod annotations;
mod capture;
mod card;
mod feedback;
mod markup;
#[cfg(all(test, target_os = "linux"))]
mod native_tests;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::core::{Runtime, RuntimeGuard};
use dioxus::desktop::wry::{WebContext, WebView, WebViewBuilder};
use dioxus::prelude::*;
use sha2::{Digest, Sha256};
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

fn profile(root: &Path, environment: &EnvironmentId) -> PathBuf {
    let digest = Sha256::digest(environment.to_string().as_bytes());
    root.join(
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
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

/// Where a page goes for `typed`, an address typed or one given whole: the
/// address of a web page, with a loopback port swapped for the one `forward`
/// makes reach the environment's own loopback. Every way to point a page at an
/// address comes here, so none reaches this computer's loopback by mistake.
fn target(typed: &str, forward: impl FnOnce(u16) -> Result<u16, String>) -> Result<String, String> {
    address(typed)
        .map_err(str::to_string)
        .and_then(|url| through_forward(&url, forward))
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
    view: WebView,
    pending: Arc<Mutex<Option<String>>>,
    selection: Rc<RefCell<annotations::Selection>>,
    notes: Rc<RefCell<Vec<annotations::Note>>>,
    next_note: Cell<u64>,
    // WebKit's context must outlive every view built with it.
    _context: Rc<RefCell<WebContext>>,
    #[cfg(target_os = "linux")]
    child: crate::native_layer::Child,
}

#[derive(Default)]
struct Pool {
    pages: HashMap<BrowserId, Rc<Page>>,
    contexts: HashMap<EnvironmentId, Rc<RefCell<WebContext>>>,
    #[cfg(target_os = "linux")]
    layer: Option<Rc<crate::native_layer::Layer>>,
}

#[derive(Clone, Default)]
struct Browsers(Rc<RefCell<Pool>>);

impl Pool {
    fn page(
        &mut self,
        browser: BrowserId,
        id: WindowId,
        environment: &EnvironmentId,
        app: AppCtx,
    ) -> Result<Rc<Page>, String> {
        if let Some(page) = self.pages.get(&browser) {
            return Ok(page.clone());
        }
        let desktop = dioxus::desktop::window();
        let directory = profile(
            &crate::paths::data_dir().join("tsumugi/browser"),
            environment,
        );
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let context = self
            .contexts
            .entry(environment.clone())
            .or_insert_with(|| Rc::new(RefCell::new(WebContext::new(Some(directory)))))
            .clone();
        let mut context_ref = context.borrow_mut();
        let return_to = desktop.clone();
        // Native WebView callbacks run outside Dioxus's event dispatch.
        let runtime = Runtime::current();
        let pending = Arc::new(Mutex::new(None));
        let new_window = pending.clone();
        let selection = Rc::new(RefCell::new(annotations::Selection::default()));
        let receive = selection.clone();
        let loading = selection.clone();
        let builder = WebViewBuilder::new_with_web_context(&mut context_ref)
            .with_url("about:blank")
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
            .with_on_page_load_handler(move |event, _| {
                // The picking goes on across pages; what was picked on the
                // last one has gone with it.
                if matches!(event, dioxus::desktop::wry::PageLoadEvent::Started) {
                    loading.borrow_mut().pending.clear();
                }
            })
            .with_ipc_handler(move |request| {
                let _guard = RuntimeGuard::new(runtime.clone());
                // A page kept hidden runs its scripts too, so it is the page shown
                // that the keys and focus answer to.
                let shown = app.content.peek().shows_page(id, browser);
                match request.body().as_str() {
                    "focus" | "workspace" | "address" if !shown => {}
                    "focus" => {
                        FOCUSED.set(true);
                        app.dispatch(Msg::WindowFocus(id));
                        app.dispatch(Msg::Focus(tsumugi_core::content::Area::Console));
                    }
                    "blur" => FOCUSED.set(false),
                    "workspace" => {
                        FOCUSED.set(false);
                        let _ = return_to.webview.focus();
                    }
                    "address" => {
                        FOCUSED.set(false);
                        let _ = return_to.webview.focus();
                        let _ = return_to.webview.evaluate_script(&format!(
                            "document.querySelector('#browser-toolbar-{id} input')?.focus()"
                        ));
                    }
                    "annotation-cancel" => receive.borrow_mut().stop(),
                    message => {
                        receive.borrow_mut().receive(message);
                    }
                }
            });
        #[cfg(target_os = "macos")]
        let builder = {
            use dioxus::desktop::wry::WebViewBuilderExtDarwin;
            use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
            let digest = Sha256::digest(environment.to_string().as_bytes());
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
            view,
            pending,
            selection,
            notes: Default::default(),
            next_note: Cell::new(1),
            _context: context,
            #[cfg(target_os = "linux")]
            child,
        });
        self.pages.insert(browser, page.clone());
        Ok(page)
    }
}

pub(crate) fn use_browsers(app: AppCtx) {
    let browsers = use_context_provider(Browsers::default);
    use_context_provider(|| Titles(Signal::new(HashMap::new())));
    use_hook(move || {
        spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                // A page stays open with its worktree's layout put away, and is
                // told apart by its own identity: layouts number their windows
                // each from 1.
                let ids: Vec<BrowserId> = app
                    .content
                    .peek()
                    .pages()
                    .into_iter()
                    .map(|(_, page)| page.id)
                    .collect();
                browsers
                    .0
                    .borrow_mut()
                    .pages
                    .retain(|id, _| ids.contains(id));
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

#[component]
pub(crate) fn BrowserWindow(
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
    let page = use_hook(move || {
        browsers
            .0
            .borrow_mut()
            .page(id, window, &page_environment, app)
    });
    let mut typed = use_signal(String::new);
    let mut current = use_signal(String::new);
    let mut error = use_signal(String::new);
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
    use_hook({
        let page = page.clone();
        let starting = environment.clone();
        move || {
            if let Some(url) = START.with(|start| start.borrow_mut().remove(&id)) {
                match target(&url, |port| app.forward_port(&starting, port)) {
                    Ok(url) => *page.pending.lock().expect("browser navigation lock") = Some(url),
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
    use_hook({
        let page = page.clone();
        move || {
            spawn(async move {
                let mut bounds = None;
                let mut ticks = 0u32;
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
                    if !app.held().is_empty()
                        || app.content.peek().moving.is_some()
                        || app.content.peek().drag.is_some()
                        || app.chrome.peek().panels.drag.is_some()
                        || draft.peek().is_some()
                        || drawing.peek().is_some()
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
                        typed.set(url.clone());
                        titles.write().insert(id, name_of(&url));
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
                        typed.set(url);
                    }
                    Err(e) => error.set(e.to_string()),
                },
                Err(e) => error.set(e),
            }
        }
    };
    let back = page.clone();
    let forward = page.clone();
    let reload = page.clone();
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
                IconButton { icon: Icon::Refresh, title: "Reload", onclick: move |_| { let _ = reload.view.reload(); } }
                InlineField {
                    class: "browser-address", aria_label: "Web address", placeholder: "Enter a URL, e.g. localhost:3000", value: typed(),
                    focus: true, contained: true,
                    oninput: move |value| typed.set(value),
                    onenter: move |_| navigate.clone()(),
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
            if !error().is_empty() { div { class: "browser-error", "{error}" } }
            if !sending.error.read().is_empty() && drawing().is_some() { div { class: "browser-error", "{sending.error}" } }
            div { id: "browser-{window}", class: "browser-page",
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

    #[test]
    fn profile_names_never_interpret_environment_ids_as_paths() {
        let root = std::path::Path::new("/profiles");
        let a = profile(root, &tsumugi_model::EnvironmentId::new("../a"));
        let b = profile(root, &tsumugi_model::EnvironmentId::new("a"));
        assert_eq!(a.parent(), Some(root));
        assert_ne!(a, b);
        assert_ne!(profile(root, &Default::default()), b);
    }
}
