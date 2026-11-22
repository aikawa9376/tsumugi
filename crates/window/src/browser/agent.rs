//! The browser pages an agent drives. A command arrives from the work host
//! for the worktree its session works in, acts on a page of that worktree and
//! of no other, and goes back to the host as its answer. What a page says in
//! the answer is the page's own words: the script that reads it runs in the
//! page and answers through the result of its evaluation, never through the
//! channel the page itself can call.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use dioxus::desktop::wry::WebView;
use dioxus::prelude::*;
use serde_json::{Value, json};
use tsumugi_core::content::{BrowserId, Tab};
use tsumugi_core::msg::Msg;
use tsumugi_model::EnvironmentId;
use tsumugi_protocol::{BrowserCall, BrowserCommand, BrowserResult, ImageFormat};

use super::{Browsers, Page, capture, name_of, shown_address, target, viewport};
use crate::bridge::AppCtx;

mod wait;

/// How long a script that reads or acts on a page may take.
const SCRIPT: Duration = Duration::from_secs(10);
/// How long a command that points a page at an address waits for it to load.
const LOAD: Duration = Duration::from_secs(20);

thread_local! {
    /// The commands the work host handed this window, until the loop of the
    /// pages takes them.
    static QUEUE: RefCell<Vec<(EnvironmentId, BrowserCall)>> = const { RefCell::new(Vec::new()) };
    /// The page commands act on in each worktree, once `tab create` or `tab
    /// switch` chose one.
    static CHOSEN: RefCell<HashMap<(EnvironmentId, PathBuf), BrowserId>> = RefCell::new(HashMap::new());
}

/// A command of `environment`'s work host, to carry out.
pub(crate) fn enqueue(environment: EnvironmentId, call: BrowserCall) {
    QUEUE.with(|queue| queue.borrow_mut().push((environment, call)));
}

pub(super) fn take() -> Vec<(EnvironmentId, BrowserCall)> {
    QUEUE.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

/// The page commands act on: the one chosen, while it is still open; else the
/// one shown; else the first.
fn current(
    pages: &[BrowserId],
    chosen: Option<BrowserId>,
    shown: impl Fn(BrowserId) -> bool,
) -> Option<BrowserId> {
    chosen
        .filter(|chosen| pages.contains(chosen))
        .or_else(|| pages.iter().copied().find(|page| shown(*page)))
        .or_else(|| pages.first().copied())
}

/// What a page that is blank or still loading is told.
const NOT_READY: &str =
    "This page cannot be read yet: it is blank or still loading. Use goto, then try again";

/// Whether `failure` is a page that is not ready yet, which a wait goes on
/// through.
fn not_ready(failure: &str) -> bool {
    failure == NOT_READY
}

/// The width and height, in CSS pixels, a page gave for its document, when
/// they are a size that can be pictured.
fn document_size(said: &Value) -> Result<[f64; 2], String> {
    const MOST: f64 = 16_384.0;
    let side = |key: &str| {
        said[key]
            .as_f64()
            .filter(|side| side.is_finite() && *side >= 1.0 && *side <= MOST)
    };
    match (side("width"), side("height")) {
        (Some(width), Some(height)) => Ok([width, height]),
        _ => Err(format!(
            "The page's size cannot be pictured: it must be at most {MOST} pixels each way"
        )),
    }
}

/// What a script that answered `raw`, the JSON of its result, says: its
/// answer, or why it could not give one.
fn answer_of(raw: &str) -> Result<Value, String> {
    // A script that answers nothing, as the one that is not there yet does.
    let raw = match raw.trim() {
        "" | "undefined" => "null",
        raw => raw,
    };
    let outer: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let value = match &outer {
        Value::String(inner) => serde_json::from_str(inner).map_err(|e| e.to_string())?,
        other => other.clone(),
    };
    match value {
        Value::Null => Err(NOT_READY.into()),
        Value::Object(map) if map.contains_key("error") => Err(map["error"]
            .as_str()
            .unwrap_or("the page refused")
            .to_string()),
        value => Ok(value),
    }
}

/// `element` as a reference a snapshot gives.
fn reference(element: &str) -> Result<&str, String> {
    let digits = element.strip_prefix("@e").unwrap_or_default();
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        Ok(element)
    } else {
        Err(format!(
            "{element:?} is not an element reference: use one a snapshot gave, such as @e3"
        ))
    }
}

/// The script that carries out `command` on a page that is read or acted on
/// in place; none for the commands that open, point or photograph a page.
fn script(command: &BrowserCommand) -> Result<Option<String>, String> {
    let call = |name: &str, arguments: Vec<Value>| {
        let arguments = arguments
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Some(format!("window.tsumugiAgent?.{name}({arguments})"))
    };
    Ok(match command {
        BrowserCommand::Snapshot {
            interactive,
            delta,
            selector,
            depth,
        } => call(
            "snapshot",
            vec![
                json!({"interactive": interactive, "delta": delta, "selector": selector, "depth": depth}),
            ],
        ),
        BrowserCommand::Console { limit } => call("console", vec![json!(limit)]),
        BrowserCommand::Network { limit } => call("network", vec![json!(limit)]),
        BrowserCommand::CaptureStart => call("capture", vec![json!(true)]),
        BrowserCommand::CaptureStop => call("capture", vec![json!(false)]),
        BrowserCommand::Click { element } => call("click", vec![json!(reference(element)?)]),
        BrowserCommand::Fill { element, value } => {
            call("fill", vec![json!(reference(element)?), json!(value)])
        }
        _ => None,
    })
}

async fn evaluate(view: &WebView, script: &str) -> Result<Value, String> {
    let (done, answered) = tokio::sync::oneshot::channel();
    let done = Mutex::new(Some(done));
    view.evaluate_script_with_callback(script, move |raw| {
        if let Some(done) = done.lock().expect("script result lock").take() {
            let _ = done.send(raw);
        }
    })
    .map_err(|e| e.to_string())?;
    let raw = tokio::time::timeout(SCRIPT, answered)
        .await
        .map_err(|_| "The page did not answer; it may be busy or gone".to_string())?
        .map_err(|_| "The page went away".to_string())?;
    answer_of(&raw)
}

/// What the page has for a title.
async fn title_of(view: &WebView) -> String {
    let title = tokio::time::timeout(
        Duration::from_secs(2),
        evaluate(view, "JSON.stringify(document.title)"),
    )
    .await;
    match title {
        Ok(Ok(Value::String(title))) if !title.is_empty() => title,
        _ => name_of(&view.url().unwrap_or_default()),
    }
}

/// Carry out `call`, which `environment`'s host handed the window, and tell
/// the host what came of it.
pub(super) async fn run(
    app: AppCtx,
    browsers: Browsers,
    environment: EnvironmentId,
    call: BrowserCall,
) {
    let number = call.call;
    let result = match execute(app, &browsers, &environment, call).await {
        Ok(result) => result,
        Err(text) => BrowserResult::Failed { text },
    };
    let answer = app
        .ask_in(
            &environment,
            tsumugi_protocol::Ask::BrowserResult {
                call: number,
                result,
            },
        )
        .await;
    if !matches!(answer, tsumugi_protocol::Answer::Done) {
        tracing::warn!(?answer, "the work host took no browser result");
    }
}

fn page_of(
    app: AppCtx,
    browsers: &Browsers,
    environment: &EnvironmentId,
    id: BrowserId,
) -> Result<std::rc::Rc<Page>, String> {
    browsers.0.borrow_mut().page(id, environment, app)
}

/// The address `page` shows, when it shows a page that can be photographed
/// or exported.
fn page_address(page: &Page) -> Result<String, String> {
    let url = page.view.url().unwrap_or_default();
    if url.is_empty() || url == "about:blank" {
        return Err("The page is blank; use goto first".into());
    }
    Ok(url)
}

/// A picture of the page, as the host is handed it.
fn picture(bytes: Vec<u8>, format: ImageFormat) -> BrowserResult {
    use base64::Engine;
    BrowserResult::Image {
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        mime: format.mime().into(),
    }
}

async fn execute(
    app: AppCtx,
    browsers: &Browsers,
    environment: &EnvironmentId,
    call: BrowserCall,
) -> Result<BrowserResult, String> {
    call.command.check()?;
    // Only the pages of the worktree the session works in are reached: no
    // command names a page by anything that could lead elsewhere.
    let reach = app.reach(environment, &call.dir)?;
    let key = (environment.clone(), reach.worktree.clone());
    let chosen = CHOSEN.with(|chosen| chosen.borrow().get(&key).copied());
    let shown = |page| app.content.peek().window_of_page(page).is_some();
    let now = current(&reach.pages, chosen, shown);
    let done = |value: Value| Ok(BrowserResult::Done { value });
    let open = |url: Option<String>| -> Result<BrowserId, String> {
        let url = url
            .map(|url| target(&url, |port| app.forward_port(environment, port)))
            .transpose()?;
        let tab = Tab::browser(environment.clone(), reach.worktree.clone());
        let Tab::Browser { id, .. } = &tab else {
            unreachable!("a new page is a browser tab")
        };
        let id = *id;
        // A page an agent opens goes on as the user the page it works on is.
        if let Some(now) = now {
            super::profiles::inherit(now, id);
        }
        app.dispatch(Msg::OpenBehind(tab));
        let page = page_of(app, browsers, environment, id)?;
        if let Some(url) = url {
            page.view.load_url(&url).map_err(|e| e.to_string())?;
        }
        CHOSEN.with(|chosen| chosen.borrow_mut().insert(key.clone(), id));
        Ok(id)
    };
    match call.command {
        BrowserCommand::TabList => {
            let mut tabs = Vec::new();
            for (index, id) in reach.pages.iter().enumerate() {
                // A page freed to save memory is listed as it was kept, not
                // loaded again for the sake of a list.
                if let Some(kept) = super::retention::saved(*id) {
                    tabs.push(json!({
                        "index": index,
                        "url": kept.address,
                        "title": name_of(&kept.address),
                        "active": Some(*id) == now,
                        "shown": shown(*id),
                    }));
                    continue;
                }
                let page = page_of(app, browsers, environment, *id)?;
                let url = page.view.url().unwrap_or_default();
                tabs.push(json!({
                    "index": index,
                    "url": shown_address(&url, |local| app.work_port(environment, local)),
                    "title": title_of(&page.view).await,
                    "active": Some(*id) == now,
                    "shown": shown(*id),
                }));
            }
            done(json!({"tabs": tabs}))
        }
        BrowserCommand::TabCreate { url } => {
            let id = open(url)?;
            let index = reach.pages.len();
            done(json!({"index": index, "id": id.to_string(), "active": true}))
        }
        BrowserCommand::TabSwitch { index } => {
            let Some(id) = reach.pages.get(index).copied() else {
                return Err(format!(
                    "There is no page {index}: this worktree has {} (counted from 0); tab list shows them",
                    reach.pages.len()
                ));
            };
            CHOSEN.with(|chosen| chosen.borrow_mut().insert(key.clone(), id));
            let tab = Tab::Browser {
                environment: environment.clone(),
                worktree: reach.worktree.clone(),
                id,
            };
            // A page in the layout shown is brought up; one in a layout put
            // away is only the one commands act on.
            let shown_now = app.content.peek().windows.shows_anywhere(&tab);
            if shown_now {
                app.dispatch(Msg::TabReveal(tab));
            }
            done(json!({"index": index, "active": true, "shown": shown_now}))
        }
        BrowserCommand::Goto { url } => {
            let (id, fresh) = match now {
                Some(id) => (id, false),
                None => (open(Some(url.clone()))?, true),
            };
            let page = page_of(app, browsers, environment, id)?;
            let before = page.loads.get();
            if !fresh {
                let resolved = target(&url, |port| app.forward_port(environment, port))?;
                page.view.load_url(&resolved).map_err(|e| e.to_string())?;
            }
            let waited = tokio::time::timeout(LOAD, async {
                while page.loads.get() == before {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await;
            let at = page.view.url().unwrap_or_default();
            done(json!({
                "url": shown_address(&at, |local| app.work_port(environment, local)),
                "title": title_of(&page.view).await,
                "loaded": waited.is_ok(),
            }))
        }
        BrowserCommand::Screenshot => {
            let id = now.ok_or("This worktree has no browser page open; use goto first")?;
            let page = page_of(app, browsers, environment, id)?;
            let url = page_address(&page)?;
            let (_, png) = capture::page(&page.view, &url).await?;
            Ok(picture(png, ImageFormat::Png))
        }
        BrowserCommand::FullScreenshot { format } => {
            let id = now.ok_or("This worktree has no browser page open; use goto first")?;
            let page = page_of(app, browsers, environment, id)?;
            let url = page_address(&page)?;
            let size = document_size(&evaluate(&page.view, "window.tsumugiAgent?.size()").await?)?;
            let format = format.unwrap_or(ImageFormat::Png);
            let image = capture::whole(&page.view, &url, size).await?;
            Ok(picture(capture::encode_as(&image, format)?, format))
        }
        BrowserCommand::Pdf => {
            let id = now.ok_or("This worktree has no browser page open; use goto first")?;
            let page = page_of(app, browsers, environment, id)?;
            page_address(&page)?;
            use base64::Engine;
            Ok(BrowserResult::Pdf {
                data: base64::engine::general_purpose::STANDARD
                    .encode(capture::pdf(&page.view).await?),
            })
        }
        BrowserCommand::SetDevice { name } => {
            let id = now.ok_or("This worktree has no browser page open; use goto first")?;
            let device = viewport::device(&name)?;
            viewport::choose(id, device.id());
            Ok(BrowserResult::Done {
                value: match device {
                    viewport::Device::Preset(preset) => json!({
                        "device": preset.label.split(" — ").next().unwrap_or(preset.id),
                        "width": preset.width,
                        "height": preset.height,
                        "shown": shown(id),
                    }),
                    viewport::Device::Responsive => {
                        json!({"device": "Responsive", "shown": shown(id)})
                    }
                },
            })
        }
        command @ BrowserCommand::Wait { .. } => match wait::plan(&command) {
            Some(wait::Plan::Pause(pause)) => {
                tokio::time::sleep(pause).await;
                done(json!({"paused": pause.as_millis() as u64}))
            }
            Some(wait::Plan::Until {
                conditions,
                timeout,
            }) => {
                let id = now.ok_or("This worktree has no browser page open; use goto first")?;
                let page = page_of(app, browsers, environment, id)?;
                let script = conditions.script();
                let address = || {
                    shown_address(&page.view.url().unwrap_or_default(), |local| {
                        app.work_port(environment, local)
                    })
                };
                wait::until(&conditions, timeout, || async {
                    if !conditions.address_holds(&address()) {
                        return Ok(false);
                    }
                    if !conditions.asks_page() {
                        return Ok(true);
                    }
                    match evaluate(&page.view, &script).await {
                        Ok(said) => Ok(wait::held(&said)),
                        Err(failure) if not_ready(&failure) => Ok(false),
                        Err(failure) => Err(failure),
                    }
                })
                .await?;
                done(json!({"waited": true, "url": address()}))
            }
            None => unreachable!("a wait is planned"),
        },
        command => {
            let id = now.ok_or("This worktree has no browser page open; use goto first")?;
            let page = page_of(app, browsers, environment, id)?;
            let script = script(&command)?.expect("a command that reads or acts on the page");
            done(evaluate(&page.view, &script).await?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(ids: &[BrowserId]) -> Vec<BrowserId> {
        ids.to_vec()
    }

    #[test]
    fn commands_act_on_the_page_chosen_else_the_one_shown_else_the_first() {
        let (a, b, c) = (BrowserId::new(), BrowserId::new(), BrowserId::new());
        let all = pages(&[a, b, c]);
        assert_eq!(current(&all, Some(c), |_| false), Some(c));
        assert_eq!(current(&all, None, |page| page == b), Some(b));
        assert_eq!(current(&all, None, |_| false), Some(a));
        // A page chosen and since closed is forgotten.
        assert_eq!(current(&all, Some(BrowserId::new()), |_| false), Some(a));
        assert_eq!(current(&[], Some(a), |_| true), None);
    }

    #[test]
    fn what_a_script_says_is_read_through_its_json_and_a_refusal_is_an_error() {
        // A script answers with the JSON text of its object, which the view
        // gives back as a JSON string.
        let wrapped = serde_json::to_string(r#"{"url":"http://x/","snapshot":"- link"}"#).unwrap();
        assert_eq!(
            answer_of(&wrapped).unwrap(),
            json!({"url": "http://x/", "snapshot": "- link"})
        );
        let refused = serde_json::to_string(r#"{"error":"@e3 is stale"}"#).unwrap();
        assert_eq!(answer_of(&refused).unwrap_err(), "@e3 is stale");
        assert!(
            answer_of("null")
                .unwrap_err()
                .contains("blank or still loading")
        );
        assert!(answer_of("not json").is_err());
    }

    #[test]
    fn the_logs_and_the_capture_are_asked_of_the_page_by_scripts_with_json_arguments() {
        let ask = |command| script(&command).unwrap().unwrap();
        assert_eq!(
            ask(BrowserCommand::Console { limit: Some(20) }),
            "window.tsumugiAgent?.console(20)"
        );
        assert_eq!(
            ask(BrowserCommand::Network { limit: None }),
            "window.tsumugiAgent?.network(null)"
        );
        assert_eq!(
            ask(BrowserCommand::CaptureStart),
            "window.tsumugiAgent?.capture(true)"
        );
        assert_eq!(
            ask(BrowserCommand::CaptureStop),
            "window.tsumugiAgent?.capture(false)"
        );
        // A picture, a pause and a device are not read off the page by a script.
        for command in [
            BrowserCommand::Pdf,
            BrowserCommand::SetDevice { name: "x".into() },
            BrowserCommand::FullScreenshot { format: None },
        ] {
            assert_eq!(script(&command), Ok(None));
        }
    }

    #[test]
    fn the_size_a_page_gives_for_its_document_is_taken_only_within_what_can_be_pictured() {
        assert_eq!(
            document_size(&json!({"width": 800, "height": 2400.5})),
            Ok([800.0, 2400.5])
        );
        for bad in [
            json!({}),
            json!({"width": 0, "height": 100}),
            json!({"width": -5, "height": 100}),
            json!({"width": "wide", "height": 100}),
            json!({"width": 100, "height": 1e12}),
            json!({"width": 40000, "height": 100}),
        ] {
            assert!(document_size(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_page_that_is_still_loading_is_not_yet_as_a_wait_asks_and_other_failures_end_it() {
        assert!(not_ready(
            "This page cannot be read yet: it is blank or still loading. Use goto, then try again"
        ));
        assert!(!not_ready("The page went away"));
    }

    #[test]
    fn only_a_reference_a_snapshot_gives_names_an_element() {
        for good in ["@e1", "@e42"] {
            assert_eq!(reference(good), Ok(good));
        }
        for bad in [
            "",
            "e3",
            "@e",
            "@e3 ",
            "#save",
            "@e3'); alert(1); ('",
            "button",
        ] {
            assert!(reference(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_script_carries_its_arguments_as_json_so_a_value_cannot_become_code() {
        let fill = BrowserCommand::Fill {
            element: "@e1".into(),
            value: "'); fetch('http://evil'); ('".into(),
        };
        let script = script(&fill).unwrap().unwrap();
        assert_eq!(
            script,
            r#"window.tsumugiAgent?.fill("@e1","'); fetch('http://evil'); ('")"#
        );
        let snapshot = script_for_snapshot();
        assert!(snapshot.starts_with("window.tsumugiAgent?.snapshot({"));
        assert!(snapshot.contains(r#""selector":"main > form""#));
        assert!(
            self::script(&BrowserCommand::Click {
                element: "x".into()
            })
            .is_err()
        );
        assert_eq!(self::script(&BrowserCommand::TabList), Ok(None));
    }

    fn script_for_snapshot() -> String {
        script(&BrowserCommand::Snapshot {
            interactive: true,
            delta: false,
            selector: Some("main > form".into()),
            depth: Some(2),
        })
        .unwrap()
        .unwrap()
    }
}
