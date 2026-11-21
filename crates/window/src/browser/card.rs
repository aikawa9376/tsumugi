//! The card an element's instruction is written on: it stands by the element
//! on a still of the page, which stands in for the page while it is open,
//! since the page itself is a native view that nothing of the workspace can
//! be drawn over.

use std::rc::Rc;

use dioxus::prelude::*;

use super::annotations::{Element as Picked, Intent};
use super::feedback::IntentChoice;
use crate::icons::{Icon, IconView};

/// An element picked, waiting for its instruction.
pub(super) struct Draft {
    pub key: u64,
    pub element: Picked,
    /// The element on its own, for the prompt.
    pub image: Option<Vec<u8>>,
    pub image_error: Option<String>,
    /// The page as it was when the element was picked, as a PNG in base64.
    pub still: Option<String>,
}

impl PartialEq for Draft {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// How the element lies on the page, as fractions of it, for the stylesheet
/// to place the frame and the card by.
fn placement(element: &Picked) -> String {
    let [x, y, w, h] = element.rect;
    let [vw, vh] = element.viewport;
    let (x, y, w, h) = (x / vw, y / vh, w / vw, h / vh);
    format!("--x:{x:.5};--y:{y:.5};--w:{w:.5};--h:{h:.5}")
}

/// Whether the card goes below the element, which it does while the element
/// is in the upper part of the page.
fn below(element: &Picked) -> bool {
    (element.rect[1] + element.rect[3] / 2.0) / element.viewport[1] < 0.55
}

#[component]
pub(super) fn Card(
    draft: Rc<Draft>,
    on_add: EventHandler<(String, Intent)>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut comment = use_signal(String::new);
    let mut intent = use_signal(Intent::default);
    let element = &draft.element;
    let label = element
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let label = if label.is_empty() {
        element.selector.clone()
    } else {
        label.chars().take(80).collect()
    };
    let side = if below(element) { "below" } else { "above" };
    let style = placement(element);
    let blank = draft.still.is_none();
    let src = draft
        .still
        .as_ref()
        .map(|png| format!("data:image/png;base64,{png}"))
        .unwrap_or_default();
    let can_add = !comment.read().trim().is_empty();
    let add = move || {
        let text = comment.peek().trim().to_string();
        if !text.is_empty() {
            on_add.call((text, *intent.peek()));
        }
    };
    rsx! {
        div { class: if blank { "browser-frame blank" } else { "browser-frame" }, style: "{style}",
            if !blank { img { class: "browser-frame-still", src: "{src}", alt: "", draggable: "false" } }
            div { class: "browser-frame-mark" }
            div { class: "browser-card {side}", role: "dialog", aria_label: "Add browser annotation",
                div { class: "card-target", title: "{element.selector}",
                    span { class: "card-tag", "{element.tag}" }
                    span { class: "card-label", "{label}" }
                }
                textarea {
                    class: "card-comment", aria_label: "Annotation comment", rows: "4", autofocus: true,
                    placeholder: "Describe what the agent should change here…", value: "{comment}",
                    oninput: move |e| comment.set(e.value()),
                    onkeydown: {
                        move |event: KeyboardEvent| {
                            event.stop_propagation();
                            let modifiers = event.modifiers();
                            match event.key() {
                                Key::Escape => { event.prevent_default(); on_cancel.call(()); }
                                Key::Enter if modifiers.ctrl() || modifiers.meta() => { event.prevent_default(); add(); }
                                _ => {}
                            }
                        }
                    },
                }
                IntentChoice { intent: intent(), onchange: move |i| intent.set(i) }
                div { class: "card-actions",
                    button { class: "button", r#type: "button", onclick: move |_| on_cancel.call(()), "Cancel" }
                    button { class: "button primary", r#type: "button", disabled: !can_add, onclick: move |_| add(),
                        IconView { icon: Icon::MessagePlus, size: 13 }
                        " Add "
                        span { class: "card-keys", "Ctrl+Enter" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rect: [f64; 4]) -> Picked {
        serde_json::from_value(serde_json::json!({
            "url":"http://localhost:3000/", "title":"", "selector":"#a", "tag":"div",
            "text":"", "html":"", "nearby":"", "styles":{}, "rect":rect,
            "viewport":[800.0,600.0], "source":null
        }))
        .unwrap()
    }

    #[test]
    fn the_card_stands_below_an_element_in_the_upper_part_of_the_page_and_above_otherwise() {
        assert!(below(&at([10.0, 20.0, 100.0, 30.0])));
        assert!(!below(&at([10.0, 500.0, 100.0, 30.0])));
    }

    #[test]
    fn the_element_is_placed_by_fractions_of_the_page() {
        let style = placement(&at([200.0, 150.0, 400.0, 300.0]));
        assert_eq!(style, "--x:0.25000;--y:0.25000;--w:0.50000;--h:0.50000");
    }
}
