//! Native children anchored to rectangles in the desktop page.

use dioxus::desktop::wry::WebViewExtUnix;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

pub(crate) struct Child {
    pub(crate) widget: gtk::Box,
    bounds: Rc<Cell<[i32; 4]>>,
    overlay: gtk::Overlay,
    handler: Option<gtk::glib::SignalHandlerId>,
}

impl Child {
    pub(crate) fn set_bounds(&self, [x, y, w, h]: [f64; 4]) {
        self.bounds.set([
            x.round() as i32,
            y.round() as i32,
            w.floor().max(1.0) as i32,
            h.floor().max(1.0) as i32,
        ]);
        self.overlay.queue_resize();
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        if self.widget.parent().as_ref() == Some(self.overlay.upcast_ref()) {
            if let Some(handler) = self.handler.take() {
                self.overlay.disconnect(handler);
            }
            self.overlay.remove(&self.widget);
        }
    }
}

pub(crate) struct Layer {
    overlay: gtk::Overlay,
    pub(crate) fixed: gtk::Fixed,
}

impl Layer {
    pub(crate) fn install(desktop: &dioxus::desktop::DesktopContext) -> Result<Self, String> {
        Self::wrap(&desktop.webview.webview())
    }

    pub(crate) fn interactive_child(&self) -> Child {
        let child = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let bounds = Rc::new(Cell::new([0, 0, 1, 1]));
        let position = bounds.clone();
        let weak = child.downgrade();
        let handler = self.overlay.connect_get_child_position(move |_, widget| {
            let child = weak.upgrade()?;
            if widget != child.upcast_ref::<gtk::Widget>() {
                return None;
            }
            let [x, y, w, h] = position.get();
            Some(gtk::Allocation::new(x, y, w, h))
        });
        self.overlay.add_overlay(&child);
        self.overlay.set_overlay_pass_through(&child, false);
        Child {
            widget: child,
            bounds,
            overlay: self.overlay.clone(),
            handler: Some(handler),
        }
    }

    pub(crate) fn wrap(webview: &impl IsA<gtk::Widget>) -> Result<Self, String> {
        let parent = webview
            .parent()
            .and_then(|p| p.downcast::<gtk::Container>().ok())
            .ok_or("The desktop webview has no GTK container")?;
        let overlay = gtk::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        let fixed = gtk::Fixed::new();
        fixed.set_hexpand(true);
        fixed.set_vexpand(true);
        parent.remove(webview);
        overlay.add(webview);
        if let Some(parent) = parent.downcast_ref::<gtk::Box>() {
            parent.pack_start(&overlay, true, true, 0);
        } else {
            parent.add(&overlay);
        }
        overlay.add_overlay(&fixed);
        // Displays start as paint only; interactive children opt into input.
        overlay.set_overlay_pass_through(&fixed, true);
        overlay.show_all();
        Ok(Self { overlay, fixed })
    }

    #[cfg(test)]
    pub(crate) fn passes_input(&self) -> bool {
        self.overlay.is_overlay_pass_through(&self.fixed)
    }
}
