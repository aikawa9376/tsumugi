//! The stills of the screens captured, over the window, to choose the part
//! to keep. It only draws and passes on what the mouse does; the view state
//! holds what is chosen.

use dioxus::prelude::*;
use tsumugi_core::capture::Stage;
use tsumugi_core::msg::Msg;

use super::ui::Ui;

/// The stills while a part is chosen, or nothing.
#[component]
pub(crate) fn Capture() -> Element {
    let ui = use_context::<Ui>();
    let stage = ui
        .app
        .overlays
        .read()
        .capture
        .as_ref()
        .map(|c| c.stage.clone());
    let Some(Stage::Choosing {
        screens,
        shown,
        part,
    }) = stage
    else {
        return rsx! {};
    };
    rsx! {
        Choosing { size: screens[shown], screen: shown, count: screens.len(), part }
    }
}

/// Where a drag started, in pixels of the screen, and where the still was
/// drawn when it did.
#[derive(Clone, Copy, PartialEq)]
struct Drag {
    from: [f64; 2],
    still: Drawn,
}

/// Where the still of a screen of `size` is drawn in the window, and at what
/// scale.
#[derive(Clone, Copy, PartialEq)]
struct Drawn {
    left: f64,
    top: f64,
    scale: f64,
}

impl Drawn {
    /// The pixel of the screen under the point `x`, `y` of the window.
    fn pixel(&self, x: f64, y: f64) -> [f64; 2] {
        [(x - self.left) / self.scale, (y - self.top) / self.scale]
    }
}

#[component]
fn Choosing(size: [u32; 2], screen: usize, count: usize, part: Option<[u32; 4]>) -> Element {
    let ui = use_context::<Ui>();
    let app = ui.app;
    let mut still = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut drag = use_signal(|| None::<Drag>);
    let Some(src) = crate::screen_capture::shown(screen) else {
        return rsx! {};
    };
    let [width, height] = size.map(f64::from);
    // The part is drawn in shares of the still, which scales with the window.
    let marked = part.map(|[l, t, w, h]| {
        format!(
            "left: {}%; top: {}%; width: {}%; height: {}%;",
            f64::from(l) / width * 100.0,
            f64::from(t) / height * 100.0,
            f64::from(w) / width * 100.0,
            f64::from(h) / height * 100.0,
        )
    });
    let counter = format!("Screen {} of {count}", screen + 1);
    let send = move |e: &Event<MouseData>, done: bool| {
        if let Some(d) = drag() {
            let at = e.client_coordinates();
            let to = d.still.pixel(at.x, at.y);
            app.dispatch(Msg::CaptureDrag {
                from: d.from,
                to,
                done,
            });
        }
    };
    rsx! {
        div { class: "capture",
            onmousemove: move |e: Event<MouseData>| send(&e, false),
            onmouseup: move |e: Event<MouseData>| {
                send(&e, true);
                drag.set(None);
            },
            div { class: "capture-bar",
                span { "Drag over the part to keep" }
                span { class: "capture-keys",
                    kbd { "Enter" } " the whole screen  "
                    if count > 1 {
                        kbd { "Tab" } " the next screen  "
                    }
                    kbd { "Esc" } " keep nothing"
                }
                if count > 1 {
                    span { class: "capture-count", "{counter}" }
                }
                button { class: "capture-button",
                    onclick: move |_| app.dispatch(Msg::CaptureCancel),
                    "Cancel"
                }
            }
            div { class: "capture-stage",
                div { class: "capture-still",
                    onmounted: move |e| still.set(Some(e.data())),
                    onmousedown: move |e: Event<MouseData>| {
                        e.prevent_default();
                        let at = e.client_coordinates();
                        spawn(async move {
                            let Some(m) = still() else {
                                return;
                            };
                            if let Ok(r) = m.get_client_rect().await
                                && r.width() > 0.0
                            {
                                let drawn = Drawn {
                                    left: r.origin.x,
                                    top: r.origin.y,
                                    scale: r.width() / width,
                                };
                                drag.set(Some(Drag { from: drawn.pixel(at.x, at.y), still: drawn }));
                            }
                        });
                    },
                    img { src: "{src}", draggable: false }
                    if let Some(marked) = marked {
                        div { class: "capture-part", style: "{marked}" }
                    }
                }
            }
        }
    }
}
