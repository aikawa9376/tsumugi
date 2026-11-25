//! Drawing on a still of the page, as a pen, a highlighter, arrows, boxes,
//! ellipses and text; the drawing and the still leave as one image.

use std::rc::Rc;

use base64::Engine;
use dioxus::prelude::*;
use serde_json::json;
use tsumugi_core::content::WindowId;

use crate::icons::Icon;

/// A still of the page, as a PNG in base64. Two stills are never alike: one
/// is made each time the user starts to draw.
pub(super) struct Still {
    pub png: String,
    /// The page the still is of.
    pub url: String,
}

impl PartialEq for Still {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

const SCRIPT: &str = include_str!("../../assets/js/browser-markup.js");

const COLOURS: [&str; 7] = [
    "#ef4444", "#f97316", "#eab308", "#22c55e", "#3b82f6", "#111827", "#ffffff",
];
const WIDTHS: [u32; 3] = [2, 4, 8];
const SIZES: [u32; 5] = [14, 18, 24, 32, 48];

const TOOLS: [(&str, &str, Icon); 7] = [
    ("pen", "Pen", Icon::Pencil),
    ("highlight", "Highlighter", Icon::Highlight),
    ("arrow", "Arrow", Icon::ArrowUpRight),
    ("rect", "Rectangle", Icon::Square),
    ("ellipse", "Ellipse", Icon::Circle),
    ("text", "Text", Icon::Typography),
    ("eraser", "Eraser", Icon::Eraser),
];

/// Where the drawing ends up, which the caller says.
#[component]
pub(super) fn Markup(
    window: WindowId,
    still: Rc<Still>,
    busy: bool,
    on_done: EventHandler<Vec<u8>>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut tool = use_signal(|| "pen");
    let mut colour = use_signal(|| COLOURS[0]);
    let mut width = use_signal(|| WIDTHS[1]);
    let mut size = use_signal(|| SIZES[1]);
    let mut can_undo = use_signal(|| false);
    let mut can_redo = use_signal(|| false);
    let eval = use_hook(|| {
        let eval = document::eval(SCRIPT);
        let _ = eval.send(json!({ "window": window.to_string() }));
        let mut replies = eval;
        spawn(async move {
            while let Ok(reply) = replies.recv::<serde_json::Value>().await {
                if let Some(png) = reply.get("png").and_then(|v| v.as_str()) {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(png) {
                        on_done.call(bytes);
                    }
                } else {
                    can_undo.set(reply["undo"].as_bool().unwrap_or(false));
                    can_redo.set(reply["redo"].as_bool().unwrap_or(false));
                }
            }
        });
        eval
    });
    use_drop(move || {
        let _ = eval.send(json!({ "stop": true }));
    });
    let command = move |message: serde_json::Value| {
        let _ = eval.send(message);
    };
    let src = format!("data:image/png;base64,{}", still.png);
    rsx! {
        div { class: "browser-markup",
            img { id: "browser-markup-base-{window}", class: "browser-markup-base", src: "{src}", alt: "", draggable: "false" }
            canvas { id: "browser-markup-canvas-{window}", class: if busy { "browser-markup-canvas busy" } else { "browser-markup-canvas" } }
            div { class: "browser-markup-controls",
                div { class: "browser-markup-bar",
                    for (kind, label, icon) in TOOLS {
                        button {
                            key: "{kind}", r#type: "button", title: "{label}", aria_label: "{label}",
                            class: if tool() == kind { "markup-tool active" } else { "markup-tool" },
                            aria_pressed: tool() == kind,
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| { tool.set(kind); command(json!({ "tool": kind })); },
                            crate::icons::IconView { icon, size: 15 }
                        }
                    }
                    span { class: "markup-divider" }
                    for swatch in COLOURS {
                        button {
                            key: "{swatch}", r#type: "button", title: "{swatch}", aria_label: "Colour {swatch}",
                            class: if colour() == swatch { "markup-colour active" } else { "markup-colour" },
                            style: "background:{swatch}",
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| { colour.set(swatch); command(json!({ "color": swatch })); },
                        }
                    }
                    span { class: "markup-divider" }
                    if tool() == "text" {
                        for option in SIZES {
                            button {
                                key: "{option}", r#type: "button", title: "{option} px", aria_label: "Font size {option}",
                                class: if size() == option { "markup-size active" } else { "markup-size" },
                                onmousedown: move |e| e.prevent_default(),
                                onclick: move |_| { size.set(option); command(json!({ "font": option })); },
                                "{option}"
                            }
                        }
                    } else {
                        for option in WIDTHS {
                            button {
                                key: "{option}", r#type: "button", title: "{option} px", aria_label: "Thickness {option}",
                                class: if width() == option { "markup-width active" } else { "markup-width" },
                                onmousedown: move |e| e.prevent_default(),
                                onclick: move |_| { width.set(option); command(json!({ "width": option })); },
                                span { style: "width:{option + 2}px;height:{option + 2}px" }
                            }
                        }
                    }
                    span { class: "markup-divider" }
                    button { r#type: "button", class: "markup-tool", title: "Undo", aria_label: "Undo", disabled: !can_undo(),
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| command(json!({ "undo": true })),
                        crate::icons::IconView { icon: Icon::ArrowBackUp, size: 15 }
                    }
                    button { r#type: "button", class: "markup-tool", title: "Redo", aria_label: "Redo", disabled: !can_redo(),
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| command(json!({ "redo": true })),
                        crate::icons::IconView { icon: Icon::ArrowForwardUp, size: 15 }
                    }
                    button { r#type: "button", class: "markup-tool", title: "Clear all", aria_label: "Clear all", disabled: !can_undo() && !can_redo(),
                        onmousedown: move |e| e.prevent_default(),
                        onclick: move |_| command(json!({ "clear": true })),
                        crate::icons::IconView { icon: Icon::Trash, size: 15 }
                    }
                }
                div { class: "browser-markup-bar",
                    span { class: "markup-hint", "Draw on the page, then add the drawing to the prompt." }
                    button { r#type: "button", class: "button", disabled: busy, onclick: move |_| on_cancel.call(()),
                        crate::icons::IconView { icon: Icon::X, size: 13 }
                        " Cancel"
                    }
                    button { r#type: "button", class: "button primary", disabled: busy,
                        onclick: move |_| command(json!({ "export": true })),
                        crate::icons::IconView { icon: Icon::Check, size: 13 }
                        " Add to prompt"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SCRIPT;

    // Dioxus closes an eval's channel as soon as the script's body returns,
    // and drops what it is sent from then on; a script that only starts an
    // async function never hears its first message.
    #[test]
    fn the_drawing_script_awaits_its_body_so_it_hears_what_it_is_sent() {
        let body = SCRIPT
            .lines()
            .find(|line| !line.trim_start().starts_with("//"))
            .unwrap_or_default();
        assert!(body.starts_with("await (async () => {"), "{body}");
    }
}
