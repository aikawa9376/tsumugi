//! Searching the text of the page shown: the bar under the toolbar, and the
//! script that finds and shows each match.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::desktop::wry::WebView;
use dioxus::prelude::*;
use serde::Deserialize;
use tsumugi_core::content::WindowId;

use super::Page;
use crate::icons::{Icon, IconView};

pub(super) const SCRIPT: &str = include_str!("../../assets/js/browser-find.js");

thread_local! {
    /// The windows whose page was asked to open its search bar, until their
    /// loop takes the asking.
    static ASKED: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
}

/// The page of the window `window` is asked to open its search bar.
pub(crate) fn ask(window: WindowId) {
    ASKED.with(|asked| asked.borrow_mut().push(window));
}

pub(super) fn take(window: WindowId) -> bool {
    ASKED.with(|asked| {
        let mut asked = asked.borrow_mut();
        let was = asked.contains(&window);
        asked.retain(|w| *w != window);
        was
    })
}

/// What a search found.
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub(super) struct Outcome {
    found: bool,
    total: usize,
    ordinal: usize,
}

impl Outcome {
    /// What the bar says of it.
    pub(super) fn label(&self) -> String {
        match (self.found, self.total, self.ordinal) {
            (_, 0, _) | (false, ..) => "No matches".into(),
            (true, total, 0) => format!("{total} matches"),
            (true, total, at) => format!("{at} of {total}"),
        }
    }
}

/// Show the next match of `query` in the page `view` shows, or the previous
/// when `backwards`, from the top of the page when `fresh`.
pub(super) async fn search(
    view: &WebView,
    query: &str,
    backwards: bool,
    fresh: bool,
) -> Result<Outcome, String> {
    let query = serde_json::to_string(query).map_err(|e| e.to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::sync::Mutex::new(Some(tx));
    view.evaluate_script_with_callback(
        &format!("window.tsumugiFind.run({query},{backwards},{fresh})"),
        move |answer| {
            if let Some(tx) = tx.lock().expect("search answer lock").take() {
                let _ = tx.send(answer);
            }
        },
    )
    .map_err(|e| e.to_string())?;
    let answer = rx
        .await
        .map_err(|_| "The page did not answer".to_string())?;
    serde_json::from_str(&answer).map_err(|_| "This page cannot be searched".to_string())
}

/// Let go of the match shown.
pub(super) fn stop(view: &WebView) {
    let _ = view.evaluate_script("window.tsumugiFind?.stop()");
}

/// What the search bar holds, kept with the page's window so that closing
/// the bar and opening it again finds the same words.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Find {
    pub open: Signal<bool>,
    query: Signal<String>,
    status: Signal<String>,
    /// Counts the askings, so that asking again takes the keys back.
    pub asked: Signal<u32>,
    /// The search the status answers, so that a slow one does not overwrite
    /// a newer.
    latest: Signal<u32>,
}

pub(super) fn use_find() -> Find {
    Find {
        open: use_signal(|| false),
        query: use_signal(String::new),
        status: use_signal(String::new),
        asked: use_signal(|| 0),
        latest: use_signal(|| 0),
    }
}

/// The page a bar searches.
#[derive(Clone)]
pub(super) struct Searched(pub Rc<Page>);

impl PartialEq for Searched {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[component]
pub(super) fn Bar(window: WindowId, page: Searched, find: Find) -> Element {
    let Find {
        mut open,
        mut query,
        mut status,
        asked,
        mut latest,
    } = find;
    let field = format!("browser-find-{window}");
    let mut run = {
        let page = page.0.clone();
        move |backwards: bool, fresh: bool| {
            let text = query.peek().clone();
            let this = *latest.peek() + 1;
            latest.set(this);
            let page = page.clone();
            spawn(async move {
                let said = match search(&page.view, &text, backwards, fresh).await {
                    Ok(_) if text.is_empty() => String::new(),
                    Ok(outcome) => outcome.label(),
                    Err(message) => message,
                };
                if *latest.peek() == this {
                    status.set(said);
                }
            });
        }
    };
    use_effect({
        let field = field.clone();
        let mut run = run.clone();
        move || {
            // Asking again, with the bar open already, takes the keys back.
            let _ = asked();
            let _ = document::eval(&format!(
                "const f=document.getElementById('{field}');f?.focus();f?.select();"
            ));
            if !query.peek().is_empty() {
                run(false, true);
            }
        }
    });
    let close = {
        let page = page.0.clone();
        move || {
            stop(&page.view);
            status.set(String::new());
            open.set(false);
        }
    };
    let (mut close_key, mut close_button) = (close.clone(), close);
    let (mut previous, mut next) = (run.clone(), run.clone());
    rsx! {
        div { class: "browser-find", role: "search",
            input {
                id: "{field}", class: "find-field", r#type: "text",
                aria_label: "Find in the page", placeholder: "Find in the page",
                autocomplete: "off", spellcheck: "false",
                value: query(),
                oninput: {
                    let mut run = run.clone();
                    move |e| { query.set(e.value()); run(false, true); }
                },
                onkeydown: move |event| {
                    let shift = event.modifiers().shift();
                    match event.key() {
                        Key::Escape => { event.prevent_default(); close_key(); }
                        Key::Enter => { event.prevent_default(); run(shift, false); }
                        _ => {}
                    }
                },
            }
            span { class: "find-status", "{status}" }
            button { class: "find-step", r#type: "button", title: "Previous match (Shift+Enter)",
                onmousedown: move |e| e.prevent_default(),
                onclick: move |_| previous(true, false),
                IconView { icon: Icon::ChevronUp, size: 14 }
            }
            button { class: "find-step", r#type: "button", title: "Next match (Enter)",
                onmousedown: move |e| e.prevent_default(),
                onclick: move |_| next(false, false),
                IconView { icon: Icon::ChevronDown, size: 14 }
            }
            button { class: "find-step", r#type: "button", title: "Close (Escape)",
                onmousedown: move |e| e.prevent_default(),
                onclick: move |_| close_button(),
                IconView { icon: Icon::X, size: 14 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(found: bool, total: usize, ordinal: usize) -> Outcome {
        Outcome {
            found,
            total,
            ordinal,
        }
    }

    #[test]
    fn the_bar_says_how_far_through_the_matches_the_one_shown_is() {
        assert_eq!(outcome(true, 12, 3).label(), "3 of 12");
        assert_eq!(outcome(true, 12, 0).label(), "12 matches");
        assert_eq!(outcome(false, 0, 0).label(), "No matches");
        assert_eq!(outcome(false, 4, 0).label(), "No matches");
    }

    #[test]
    fn what_the_script_answers_is_read() {
        let answer = r#"{"found":true,"total":5,"ordinal":2}"#;
        assert_eq!(
            serde_json::from_str::<Outcome>(answer).unwrap(),
            outcome(true, 5, 2)
        );
    }

    #[test]
    fn a_search_is_taken_once_by_the_window_it_was_asked_of() {
        let window = 7;
        assert!(!take(window));
        ask(window);
        assert!(take(window));
        assert!(!take(window));
    }
}
