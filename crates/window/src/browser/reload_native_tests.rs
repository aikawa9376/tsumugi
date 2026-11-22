use super::native_tests::pump;
use super::*;
use dioxus::desktop::wry::WebViewBuilderExtUnix;
use gtk::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

/// A page whose stylesheet the cache holds for an hour: a reload leaves the
/// stylesheet where it is, a hard reload asks the server for it again.
#[test]
#[ignore = "requires a GTK display and WebKitGTK"]
fn a_hard_reload_asks_the_network_for_what_the_cache_holds() {
    gtk::init().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let asked = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let server = {
        let (asked, stop) = (asked.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                let asked = asked.clone();
                std::thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut request = [0; 4096];
                    let n = stream.read(&mut request).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let first = String::from_utf8_lossy(&request[..n]);
                    let (kind, body) = if first.starts_with("GET /asset.css") {
                        asked.fetch_add(1, Ordering::Relaxed);
                        ("text/css", "p { color: red }")
                    } else {
                        (
                            "text/html",
                            "<!doctype html><p>BROWSER_FIXTURE</p><link rel=stylesheet href=/asset.css>",
                        )
                    };
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nCache-Control: max-age=3600\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(reply.as_bytes());
                });
            }
        })
    };
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(400, 300);
    let parent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let page = gtk::DrawingArea::new();
    parent.pack_start(&page, true, true, 0);
    window.add(&parent);
    let layer = crate::native_layer::Layer::wrap(&page).unwrap();
    let child = layer.interactive_child();
    child.set_bounds([0.0, 0.0, 400.0, 300.0]);
    let data = tempfile::tempdir().unwrap();
    let mut context = WebContext::new(Some(data.path().to_path_buf()));
    let view = WebViewBuilder::new_with_web_context(&mut context)
        .with_url(&url)
        .build_gtk(&child.widget)
        .unwrap();
    window.show_all();
    let wait_for = |count: usize| {
        let start = Instant::now();
        while asked.load(Ordering::Relaxed) < count {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the stylesheet was not asked for"
            );
            pump();
        }
    };
    wait_for(1);

    view.reload().unwrap();
    for _ in 0..300 {
        pump();
    }
    assert_eq!(asked.load(Ordering::Relaxed), 1, "a reload used the cache");

    reload::hard(&view);
    wait_for(2);

    drop(view);
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    // SAFETY: this test owns the GTK window and every child on this thread.
    unsafe {
        window.destroy();
    }
}
