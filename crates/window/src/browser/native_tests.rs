use super::*;
use dioxus::desktop::wry::WebViewBuilderExtUnix;
use gtk::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

struct Site {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Site {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request);
                    let body = "<!doctype html><title>Browser fixture</title><p>BROWSER_FIXTURE</p><input id=field><input type=password value=private-secret><button id=save style='background:rgb(12,150,40);width:100px;height:40px;border:0'>Save</button><a href=/two>Next</a>";
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: frame-ancestors 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(reply.as_bytes());
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

pub(super) fn pump() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
    std::thread::sleep(Duration::from_millis(2));
}

fn value(view: &WebView, js: &str) -> serde_json::Value {
    let (tx, rx) = std::sync::mpsc::channel();
    view.evaluate_script_with_callback(js, move |result| {
        let _ = tx.send(result);
    })
    .unwrap();
    let start = Instant::now();
    loop {
        pump();
        if let Ok(result) = rx.try_recv() {
            return serde_json::from_str(&result).unwrap();
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "WebView did not answer: {js}"
        );
    }
}

fn loaded(view: &WebView, path: &str) {
    let start = Instant::now();
    loop {
        if value(
            view,
            "document.body?.textContent.includes('BROWSER_FIXTURE') && location.pathname",
        ) == path
        {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "Page did not load: {path}"
        );
        pump();
    }
}

#[test]
#[ignore = "requires a GTK display and WebKitGTK"]
fn browser_pages_keep_input_and_history_while_profiles_isolate_website_data() {
    gtk::init().unwrap();
    let site = Site::start();
    let data = tempfile::tempdir().unwrap();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Tsumugi browser integration check");
    window.set_default_size(800, 500);
    let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let page = gtk::DrawingArea::new();
    parent.pack_start(&page, true, true, 0);
    window.add(&parent);
    let layer = crate::native_layer::Layer::wrap(&page).unwrap();
    assert!(layer.passes_input());
    let a = EnvironmentId::new("a");
    let b = EnvironmentId::new("b");
    let mut a_context = WebContext::new(Some(profile(data.path(), &a)));
    let mut b_context = WebContext::new(Some(profile(data.path(), &b)));
    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let build = |context: &mut WebContext| {
        let child = layer.interactive_child();
        child.set_bounds([0.0, 0.0, 800.0, 500.0]);
        let ready = Rc::new(Cell::new(false));
        let loaded = ready.clone();
        let received = messages.clone();
        let view = WebViewBuilder::new_with_web_context(context)
            .with_url(format!("{}/one", site.url))
            .with_navigation_handler(allowed)
            .with_initialization_script(include_str!("../../assets/js/browser-annotate.js"))
            .with_ipc_handler(move |request| received.borrow_mut().push(request.body().clone()))
            .with_on_page_load_handler(move |event, _| {
                if matches!(event, dioxus::desktop::wry::PageLoadEvent::Finished) {
                    loaded.set(true);
                }
            })
            .build_gtk(&child.widget)
            .unwrap();
        let start = Instant::now();
        while !ready.get() {
            pump();
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "Initial navigation did not finish"
            );
        }
        (view, child)
    };
    let (a_view, a_child) = build(&mut a_context);
    let (b_view, _b_child) = build(&mut b_context);
    window.show_all();
    loaded(&a_view, "/one");
    loaded(&b_view, "/one");
    a_child.set_bounds([40.0, 60.0, 320.0, 240.0]);
    {
        use dioxus::desktop::wry::WebViewExtUnix;
        let child = a_view.webview().parent().unwrap();
        let start = Instant::now();
        loop {
            pump();
            let (x, y) = child.translate_coordinates(&page, 0, 0).unwrap();
            let rect = (x, y, child.allocated_width(), child.allocated_height());
            if rect == (40, 60, 320, 240) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "Browser rectangle after GTK layout: {rect:?}"
            );
        }
    }
    value(
        &a_view,
        "localStorage.setItem('workspace','A'); document.cookie='workspace=A; path=/'; document.getElementById('field').value='unfinished'; true",
    );
    assert_eq!(
        value(&b_view, "localStorage.getItem('workspace')"),
        serde_json::Value::Null
    );
    assert_eq!(value(&b_view, "document.cookie"), "");
    set_visible(&a_view, false);
    set_visible(&b_view, true);
    pump();
    set_visible(&b_view, false);
    set_visible(&a_view, true);
    assert_eq!(
        value(&a_view, "document.getElementById('field').value"),
        "unfinished"
    );
    let description = value(
        &a_view,
        "tsumugiInspector.describe(document.querySelector('#save'))",
    );
    let element: annotations::Element = serde_json::from_value(description.clone()).unwrap();
    assert_eq!(element.selector, "#save");
    assert_eq!(element.styles["background-color"], "rgb(12, 150, 40)");
    assert!(!element.nearby.contains("private-secret"));
    assert_eq!(element.text, "Save");
    value(
        &a_view,
        "tsumugiInspector.set(true); document.querySelector('#save').click(); true",
    );
    assert!(
        messages.borrow().is_empty(),
        "A synthetic click must not select an element"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let png = runtime.block_on(async {
        let image = capture::element(&a_view, &element);
        tokio::pin!(image);
        loop {
            tokio::select! {
                result = &mut image => break result.unwrap(),
                _ = tokio::time::sleep(Duration::from_millis(2)) => pump(),
            }
        }
    });
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(
        image.width() > 50 && image.height() > 20,
        "An element image must have visible pixels"
    );
    let color = image.get_pixel(4, 4).0;
    assert_eq!(
        color,
        [12, 150, 40, 255],
        "Snapshot must contain the selected element's rendered color"
    );
    a_view.load_url(&format!("{}/two", site.url)).unwrap();
    loaded(&a_view, "/two");
    a_view.evaluate_script("history.back()").unwrap();
    loaded(&a_view, "/one");
    a_view.evaluate_script("history.forward()").unwrap();
    loaded(&a_view, "/two");
    a_view.reload().unwrap();
    loaded(&a_view, "/two");
    assert_eq!(value(&a_view, "localStorage.getItem('workspace')"), "A");
    assert!(
        value(&a_view, "document.cookie")
            .as_str()
            .unwrap()
            .contains("workspace=A")
    );
    drop(a_view);
    drop(a_child);
    let (a_view, _a_child) = build(&mut a_context);
    loaded(&a_view, "/one");
    assert_eq!(value(&a_view, "localStorage.getItem('workspace')"), "A");
    assert_eq!(
        value(&b_view, "localStorage.getItem('workspace')"),
        serde_json::Value::Null
    );
    drop(a_view);
    drop(b_view);
    // SAFETY: this test owns the GTK window and every child on this thread.
    unsafe {
        window.destroy();
    }
    println!(
        "BROWSER_NATIVE_OK framing=true input=true history=true reload=true isolation=true reopen=true annotations=true screenshot=true"
    );
}

/// The browser-markup script, run against a stand-in for the channel the
/// workspace gives it, in a real WebKit: drawing a box on a white still must
/// leave red where its edge is, in an image as large as the still.
#[test]
#[ignore = "requires a GTK display and WebKitGTK"]
fn drawing_on_a_still_exports_the_still_with_the_marks() {
    use base64::Engine;
    gtk::init().unwrap();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(500, 400);
    let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let page = gtk::DrawingArea::new();
    parent.pack_start(&page, true, true, 0);
    window.add(&parent);
    let layer = crate::native_layer::Layer::wrap(&page).unwrap();
    let child = layer.interactive_child();
    child.set_bounds([0.0, 0.0, 500.0, 400.0]);
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        800,
        600,
        image::Rgba([255, 255, 255, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let still = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
    let html = format!(
        "<!doctype html><body style='margin:0'><div style='position:relative;width:400px;height:300px'>\
         <img id='browser-markup-base-7' src='data:image/png;base64,{still}' style='position:absolute;inset:0;width:100%;height:100%'>\
         <canvas id='browser-markup-canvas-7' style='position:absolute;inset:0;width:100%;height:100%'></canvas></div>\
         <script>window.__sent=[];window.__inbox=[];window.__waiter=null;\
         window.dioxus={{send:(m)=>window.__sent.push(m),recv:()=>new Promise((r)=>{{if(window.__inbox.length)r(window.__inbox.shift());else window.__waiter=r;}})}};\
         window.__push=(m)=>{{if(window.__waiter){{const w=window.__waiter;window.__waiter=null;w(m);}}else window.__inbox.push(m);}};</script>"
    );
    let mut context = WebContext::new(None);
    let ready = Rc::new(Cell::new(false));
    let finished = ready.clone();
    let view = WebViewBuilder::new_with_web_context(&mut context)
        .with_html(html)
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, dioxus::desktop::wry::PageLoadEvent::Finished) {
                finished.set(true);
            }
        })
        .build_gtk(&child.widget)
        .unwrap();
    window.show_all();
    let start = Instant::now();
    while !ready.get() {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "page did not load"
        );
        pump();
    }
    let start = Instant::now();
    while value(
        &view,
        "document.getElementById('browser-markup-base-7')?.complete === true",
    ) != true
    {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "still did not load"
        );
        pump();
    }
    let script = include_str!("../../assets/js/browser-markup.js");
    let _ = view.evaluate_script(script);
    let _ = view.evaluate_script("window.__push({window:'7'})");
    for _ in 0..50 {
        pump();
    }
    let event = |kind: &str, x: u32, y: u32| {
        format!(
            "document.getElementById('browser-markup-canvas-7').dispatchEvent(new PointerEvent('{kind}',{{clientX:{x},clientY:{y},button:0,pointerId:1,bubbles:true}}))"
        )
    };
    let _ = view.evaluate_script("window.__push({color:'#ff0000',width:4})");
    for js in [
        event("pointerdown", 50, 50),
        event("pointermove", 150, 120),
        event("pointerup", 150, 120),
    ] {
        let _ = view.evaluate_script(&js);
        for _ in 0..20 {
            pump();
        }
    }
    let _ = view.evaluate_script("window.__push({tool:'rect'})");
    for _ in 0..20 {
        pump();
    }
    for js in [
        event("pointerdown", 50, 50),
        event("pointermove", 150, 120),
        event("pointerup", 150, 120),
    ] {
        let _ = view.evaluate_script(&js);
        for _ in 0..20 {
            pump();
        }
    }
    let _ = view.evaluate_script("window.__push({export:true})");
    let start = Instant::now();
    let exported = loop {
        let sent = value(
            &view,
            "JSON.stringify(window.__sent.filter((m)=>m.png).map((m)=>m.png))",
        );
        if let Some(text) = sent.as_str()
            && let Ok(list) = serde_json::from_str::<Vec<String>>(text)
            && let Some(png) = list.first()
        {
            break png.clone();
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "no image exported"
        );
        pump();
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(exported)
        .unwrap();
    let out = image::load_from_memory(&bytes).unwrap().to_rgba8();
    // The still is twice the size of the box it is shown in.
    assert_eq!((out.width(), out.height()), (800, 600));
    let left_edge = out.get_pixel(100, 170);
    assert!(
        left_edge[0] > 200 && left_edge[1] < 80 && left_edge[2] < 80,
        "the rectangle's left edge should be red, not {left_edge:?}"
    );
    assert_eq!(out.get_pixel(700, 500).0, [255, 255, 255, 255]);
    drop(view);
    // SAFETY: this test owns the GTK window and every child on this thread.
    unsafe {
        window.destroy();
    }
}

#[test]
#[ignore = "requires a GTK display and WebKitGTK"]
fn an_agent_reads_a_page_by_references_and_acts_on_them_without_reading_a_password() {
    gtk::init().unwrap();
    let site = Site::start();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(800, 500);
    let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.add(&host);
    let ready = Rc::new(Cell::new(false));
    let loaded_once = ready.clone();
    let view = WebViewBuilder::new()
        .with_url(format!("{}/one", site.url))
        .with_initialization_script(include_str!("../../assets/js/browser-agent.js"))
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, dioxus::desktop::wry::PageLoadEvent::Finished) {
                loaded_once.set(true);
            }
        })
        .build_gtk(&host)
        .unwrap();
    window.show_all();
    let start = Instant::now();
    while !ready.get() {
        pump();
        assert!(start.elapsed() < Duration::from_secs(15), "no load");
    }
    // A script answers with the JSON text of its object.
    let said = |js: &str| -> serde_json::Value {
        let text = value(&view, js);
        serde_json::from_str(text.as_str().unwrap()).unwrap()
    };
    let read = said("window.tsumugiAgent.snapshot({})");
    let tree = read["snapshot"].as_str().unwrap();
    assert!(tree.contains(r#"textbox [ref=@e1]"#), "{tree}");
    assert!(tree.contains("value=(hidden)"), "{tree}");
    assert!(!tree.contains("private-secret"), "{tree}");
    assert!(tree.contains(r#"button "Save""#), "{tree}");
    assert_eq!(
        said(r#"window.tsumugiAgent.fill("@e1", "typed")"#)["filled"],
        "@e1"
    );
    assert_eq!(
        value(&view, "document.getElementById('field').value"),
        "typed"
    );
    assert!(
        said(r#"window.tsumugiAgent.click("@e9")"#)["error"]
            .as_str()
            .unwrap()
            .contains("stale")
    );
    let link = tree
        .lines()
        .find(|line| line.contains(r#"link "Next""#))
        .and_then(|line| line.split("[ref=").nth(1))
        .and_then(|rest| rest.split(']').next())
        .unwrap()
        .to_string();
    said(&format!(r#"window.tsumugiAgent.click("{link}")"#));
    loaded(&view, "/two");
    drop(view);
    // SAFETY: this test owns the GTK window and every child on this thread.
    unsafe {
        window.destroy();
    }
}

/// Run `future` while the GTK main loop goes on, as the window's own does.
fn block<T>(future: impl std::future::Future<Output = T>) -> T {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut future = Box::pin(future);
        loop {
            match tokio::time::timeout(Duration::from_millis(5), &mut future).await {
                Ok(out) => return out,
                Err(_) => pump(),
            }
        }
    })
}

#[test]
#[ignore = "requires a GTK display and WebKitGTK"]
fn an_agent_waits_for_a_page_reads_what_it_logged_and_photographs_it_though_it_is_not_shown() {
    gtk::init().unwrap();
    let site = Site::start();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(800, 500);
    let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.add(&host);
    let ready = Rc::new(Cell::new(false));
    let loaded_once = ready.clone();
    let view = WebViewBuilder::new()
        .with_url(format!("{}/one", site.url))
        .with_initialization_script(include_str!("../../assets/js/browser-agent.js"))
        .with_on_page_load_handler(move |event, _| {
            if matches!(event, dioxus::desktop::wry::PageLoadEvent::Finished) {
                loaded_once.set(true);
            }
        })
        .build_gtk(&host)
        .unwrap();
    window.show_all();
    let start = Instant::now();
    while !ready.get() {
        pump();
        assert!(start.elapsed() < Duration::from_secs(15), "no load");
    }
    let said = |js: &str| -> serde_json::Value {
        let text = value(&view, js);
        serde_json::from_str(text.as_str().unwrap()).unwrap()
    };

    // What the page logs is kept, in order, with its level.
    value(
        &view,
        "console.log('hello', {a: 1}); console.warn('careful'); console.error(new Error('boom').message); 1",
    );
    let logged = said("window.tsumugiAgent.console(10)");
    let entries = logged["entries"].as_array().unwrap();
    assert_eq!(logged["total"], 3);
    assert_eq!(
        entries[0],
        serde_json::json!({"level": "log", "text": "hello {\"a\":1}"})
    );
    assert_eq!(entries[1]["level"], "warning");
    assert_eq!(entries[2]["text"], "boom");
    // Only the latest entries are given.
    assert_eq!(
        said("window.tsumugiAgent.console(1)")["entries"][0]["text"],
        "boom"
    );
    assert_eq!(said("window.tsumugiAgent.console(1)")["total"], 3);

    // The page's requests are kept with their status and kind, and no body.
    value(
        &view,
        "fetch('/two').then(() => { window.fetched = true; }); 1",
    );
    let start = Instant::now();
    while value(&view, "window.fetched === true") != true {
        pump();
        assert!(start.elapsed() < Duration::from_secs(15), "no fetch");
    }
    let requests = said("window.tsumugiAgent.network(50)");
    let entries = requests["entries"].as_array().unwrap();
    let document = &entries[0];
    assert_eq!(document["method"], "GET");
    assert!(
        document["url"].as_str().unwrap().ends_with("/one"),
        "{document}"
    );
    let fetched = entries
        .iter()
        .find(|entry| entry["url"].as_str().unwrap().ends_with("/two"))
        .unwrap_or_else(|| panic!("no /two in {requests}"));
    assert_eq!(fetched["status"], 200);
    assert_eq!(fetched["mimeType"], "text/html");

    // A stopped capture keeps what it has; a started one begins afresh.
    assert_eq!(
        said("window.tsumugiAgent.capture(false)")["capturing"],
        false
    );
    value(&view, "console.log('after stop'); 1");
    assert_eq!(said("window.tsumugiAgent.console(10)")["total"], 3);
    assert_eq!(said("window.tsumugiAgent.capture(true)")["capturing"], true);
    assert_eq!(said("window.tsumugiAgent.console(10)")["total"], 0);

    // A wait looks at what the page shows, and never at what a field holds.
    let check = |wanted: serde_json::Value| {
        said(&format!("window.tsumugiAgent.check({wanted})"))["met"].clone()
    };
    assert_eq!(check(serde_json::json!({"text": "BROWSER_FIXTURE"})), true);
    assert_eq!(check(serde_json::json!({"text": "private-secret"})), false);
    assert_eq!(check(serde_json::json!({"text": "Save"})), true);
    assert_eq!(check(serde_json::json!({"selector": "#save"})), true);
    assert_eq!(
        check(serde_json::json!({"selector": "#save", "state": "hidden"})),
        false
    );
    assert_eq!(check(serde_json::json!({"selector": "#absent"})), false);
    assert_eq!(
        check(serde_json::json!({"selector": "#absent", "state": "hidden"})),
        true
    );
    assert_eq!(check(serde_json::json!({"load": "load"})), true);
    assert_eq!(check(serde_json::json!({"load": "domcontentloaded"})), true);
    assert!(said("window.tsumugiAgent.check({selector: '!!'})")["error"].is_string());
    // A page that has made no request for half a second is idle.
    let start = Instant::now();
    while check(serde_json::json!({"load": "networkidle"})) != true {
        pump();
        assert!(start.elapsed() < Duration::from_secs(15), "never idle");
    }

    // The whole page is taller than the part in view.
    value(&view, "document.body.style.height = '3000px'; 1");
    let size = said("window.tsumugiAgent.size()");
    assert!(size["height"].as_f64().unwrap() >= 3000.0, "{size}");
    let url = view.url().unwrap();
    let shown = block(capture::page(&view, &url)).unwrap().0;
    let whole = block(capture::whole(
        &view,
        &url,
        [
            size["width"].as_f64().unwrap(),
            size["height"].as_f64().unwrap(),
        ],
    ))
    .unwrap();
    assert!(
        whole.height() > shown.height() * 2,
        "{} {}",
        whole.height(),
        shown.height()
    );
    let jpeg = capture::encode_as(&whole, tsumugi_protocol::ImageFormat::Jpeg).unwrap();
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);

    // The page is a PDF.
    let pdf = block(capture::pdf(&view)).unwrap();
    assert_eq!(&pdf[..5], b"%PDF-", "{:?}", &pdf[..pdf.len().min(16)]);

    // A page that is not shown can still be photographed and exported.
    let _ = view.set_visible(false);
    host.hide();
    pump();
    let hidden = block(capture::page(&view, &url));
    let hidden_whole = block(capture::whole(&view, &url, [800.0, 3000.0]));
    let hidden_pdf = block(capture::pdf(&view));
    assert!(hidden.is_ok(), "hidden screenshot: {:?}", hidden.err());
    assert!(
        hidden_whole.is_ok(),
        "hidden full: {:?}",
        hidden_whole.err()
    );
    assert!(hidden_pdf.is_ok(), "hidden pdf: {:?}", hidden_pdf.err());
    let hidden = hidden.unwrap().0;
    assert!(
        hidden
            .to_rgba8()
            .pixels()
            .any(|p| p.0 != [0, 0, 0, 0] && p.0 != hidden.to_rgba8().get_pixel(0, 0).0),
        "the hidden page's picture is blank"
    );
    drop(view);
    // SAFETY: this test owns the GTK window and every child on this thread.
    unsafe {
        window.destroy();
    }
}
