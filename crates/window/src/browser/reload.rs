//! A reload that goes round the cache, for a developer working on a page's
//! own assets.

use dioxus::desktop::wry::WebView;

/// Load `view`'s page again, asking the network for the page and what it
/// draws on instead of trusting what the cache holds. Wry has no call for
/// it, so each engine is asked its own way.
pub(super) fn hard(view: &WebView) {
    #[cfg(target_os = "linux")]
    {
        use dioxus::desktop::wry::WebViewExtUnix;
        use gtk::glib::object::ObjectType;
        unsafe extern "C" {
            fn webkit_web_view_reload_bypass_cache(view: *mut gtk::glib::gobject_ffi::GObject);
        }
        let webview = view.webview();
        // SAFETY: the view is alive, and the call only asks it to load again.
        unsafe { webkit_web_view_reload_bypass_cache(webview.as_ptr().cast()) };
    }
    #[cfg(target_os = "macos")]
    {
        use dioxus::desktop::wry::WebViewExtMacOS;
        // SAFETY: `reloadFromOrigin` takes no argument and returns the
        // navigation it starts, which is let go.
        let _: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
            unsafe { objc2::msg_send![&*view.webview(), reloadFromOrigin] };
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        // The page itself is fetched again past the cache before it is
        // loaded; what it draws on is not, as the engine offers no way here.
        let _ = view.evaluate_script(
            "fetch(location.href,{cache:'reload'}).catch(()=>{}).finally(()=>location.reload())",
        );
    }
}
