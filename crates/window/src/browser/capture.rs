use super::annotations::Element;
use dioxus::desktop::wry::WebView;
use tsumugi_protocol::ImageFormat;

/// How much of a page a picture takes in.
#[derive(Clone, Copy)]
enum Region {
    /// What is in view.
    Visible,
    /// The whole document, scrolled or not, `[width, height]` in CSS pixels.
    #[cfg_attr(
        target_os = "linux",
        allow(dead_code, reason = "only macOS is told the size to take in")
    )]
    Whole([f64; 2]),
}

fn crop_rect(rect: [f64; 4], viewport: [f64; 2], pixels: [u32; 2]) -> Result<[u32; 4], String> {
    if !rect.iter().chain(viewport.iter()).all(|v| v.is_finite())
        || viewport.iter().any(|v| *v <= 0.0)
        || rect[2] <= 0.0
        || rect[3] <= 0.0
        || pixels.contains(&0)
    {
        return Err("The selected element has no visible image".into());
    }
    let sx = f64::from(pixels[0]) / viewport[0];
    let sy = f64::from(pixels[1]) / viewport[1];
    let x = (rect[0].max(0.0) * sx).floor().min(f64::from(pixels[0])) as u32;
    let y = (rect[1].max(0.0) * sy).floor().min(f64::from(pixels[1])) as u32;
    let right = ((rect[0] + rect[2]).max(0.0) * sx)
        .ceil()
        .min(f64::from(pixels[0])) as u32;
    let bottom = ((rect[1] + rect[3]).max(0.0) * sy)
        .ceil()
        .min(f64::from(pixels[1])) as u32;
    if right <= x || bottom <= y {
        return Err("The selected element is outside the viewport".into());
    }
    Ok([x, y, right - x, bottom - y])
}

fn encode(image: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(bytes.into_inner())
}

/// `image` encoded as `format`.
pub(super) fn encode_as(
    image: &image::DynamicImage,
    format: ImageFormat,
) -> Result<Vec<u8>, String> {
    match format {
        ImageFormat::Png => encode(image),
        ImageFormat::Jpeg => {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image.to_rgb8())
                .write_to(&mut bytes, image::ImageFormat::Jpeg)
                .map_err(|e| e.to_string())?;
            Ok(bytes.into_inner())
        }
    }
}

/// The part of `image`, a snapshot of the page, that `element` covers.
pub(super) fn crop(image: &image::DynamicImage, element: &Element) -> Result<Vec<u8>, String> {
    let [x, y, w, h] = crop_rect(
        element.rect,
        element.viewport,
        [image.width(), image.height()],
    )?;
    encode(&image.crop_imm(x, y, w, h))
}

/// The visible part of the page as a PNG, with the image it was made from.
/// `url` is the page it must still be showing when the image is taken.
pub(super) async fn page(
    view: &WebView,
    url: &str,
) -> Result<(image::DynamicImage, Vec<u8>), String> {
    shot(view, url, Region::Visible).await
}

/// The whole document of the page, as a picture, scrolled or not. `size` is
/// the document's width and height in CSS pixels.
pub(super) async fn whole(
    view: &WebView,
    url: &str,
    size: [f64; 2],
) -> Result<image::DynamicImage, String> {
    shot(view, url, Region::Whole(size))
        .await
        .map(|(image, _)| image)
}

async fn shot(
    view: &WebView,
    url: &str,
    region: Region,
) -> Result<(image::DynamicImage, Vec<u8>), String> {
    if view.url().ok().as_deref() != Some(url) {
        return Err("The page changed before its image was captured".into());
    }
    let image = tokio::time::timeout(std::time::Duration::from_secs(10), snapshot(view, region))
        .await
        .map_err(|_| "The browser image timed out".to_string())??;
    if view.url().ok().as_deref() != Some(url) {
        return Err("The page changed while its image was captured".into());
    }
    let png = encode(&image)?;
    Ok((image, png))
}

#[cfg(test)]
pub(super) async fn element(view: &WebView, element: &Element) -> Result<Vec<u8>, String> {
    let (image, _) = page(view, &element.url).await?;
    crop(&image, element)
}

#[cfg(target_os = "linux")]
async fn snapshot(view: &WebView, region: Region) -> Result<image::DynamicImage, String> {
    use dioxus::desktop::wry::WebViewExtUnix;
    use glib::object::ObjectType;
    use gtk::{cairo, gio, glib};
    type Sender = tokio::sync::oneshot::Sender<Result<image::DynamicImage, String>>;
    // The WebKit dependency is owned by Wry. These stable C entry points avoid
    // introducing a second, incompatible GTK binding just for snapshots.
    unsafe extern "C" {
        fn webkit_web_view_get_snapshot(
            view: *mut glib::gobject_ffi::GObject,
            region: i32,
            options: u32,
            cancel: *mut gio::ffi::GCancellable,
            callback: Option<
                unsafe extern "C" fn(
                    *mut glib::gobject_ffi::GObject,
                    *mut gio::ffi::GAsyncResult,
                    *mut std::ffi::c_void,
                ),
            >,
            data: *mut std::ffi::c_void,
        );
        fn webkit_web_view_get_snapshot_finish(
            view: *mut glib::gobject_ffi::GObject,
            result: *mut gio::ffi::GAsyncResult,
            error: *mut *mut glib::ffi::GError,
        ) -> *mut cairo::ffi::cairo_surface_t;
    }
    unsafe extern "C" fn ready(
        view: *mut glib::gobject_ffi::GObject,
        result: *mut gio::ffi::GAsyncResult,
        data: *mut std::ffi::c_void,
    ) {
        let tx = unsafe { Box::from_raw(data.cast::<Sender>()) };
        let mut error = std::ptr::null_mut();
        let raw = unsafe { webkit_web_view_get_snapshot_finish(view, result, &mut error) };
        let result = if !error.is_null() {
            let error: glib::Error = unsafe { glib::translate::from_glib_full(error) };
            Err(error.to_string())
        } else if raw.is_null() {
            Err("The browser returned no image".into())
        } else {
            (|| {
                let surface =
                    unsafe { cairo::Surface::from_raw_full(raw) }.map_err(|e| e.to_string())?;
                let source = cairo::ImageSurface::try_from(surface)
                    .map_err(|_| "The browser returned an unsupported image".to_string())?;
                let (w, h) = (source.width(), source.height());
                let mut copy = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h)
                    .map_err(|e| e.to_string())?;
                {
                    let ctx = cairo::Context::new(&copy).map_err(|e| e.to_string())?;
                    ctx.set_source_surface(&source, 0.0, 0.0)
                        .map_err(|e| e.to_string())?;
                    ctx.paint().map_err(|e| e.to_string())?;
                }
                let stride = copy.stride() as usize;
                let data = copy.data().map_err(|e| e.to_string())?;
                let image = image::RgbaImage::from_fn(w as u32, h as u32, |x, y| {
                    let at = y as usize * stride + x as usize * 4;
                    let argb = u32::from_ne_bytes(data[at..at + 4].try_into().expect("ARGB pixel"));
                    let a = argb >> 24;
                    let channel = |shift: u32| {
                        (((argb >> shift) & 255) * 255 + a / 2)
                            .checked_div(a)
                            .unwrap_or(0)
                            .min(255) as u8
                    };
                    image::Rgba([channel(16), channel(8), channel(0), a as u8])
                });
                Ok(image::DynamicImage::ImageRgba8(image))
            })()
        };
        let _ = tx.send(result);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    unsafe {
        webkit_web_view_get_snapshot(
            view.webview().as_ptr().cast(),
            // WEBKIT_SNAPSHOT_REGION_VISIBLE, or _FULL_DOCUMENT, which takes
            // in what the page scrolls over.
            match region {
                Region::Visible => 0,
                Region::Whole(_) => 1,
            },
            0,
            std::ptr::null_mut(),
            Some(ready),
            Box::into_raw(Box::new(tx)).cast(),
        );
    }
    rx.await
        .map_err(|_| "The browser image was cancelled".to_string())?
}

#[cfg(target_os = "macos")]
async fn snapshot(view: &WebView, region: Region) -> Result<image::DynamicImage, String> {
    use dioxus::desktop::wry::WebViewExtMacOS;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::cell::RefCell::new(Some(tx));
    let done = block2::RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
        let result = (|| {
            if !error.is_null() {
                return Err("The browser could not capture its image".to_string());
            }
            let image = unsafe { image.as_ref() }.ok_or("The browser returned no image")?;
            let tiff = image
                .TIFFRepresentation()
                .ok_or("Cannot encode the browser image")?;
            let rep =
                NSBitmapImageRep::imageRepWithData(&tiff).ok_or("Cannot read the browser image")?;
            let png = unsafe {
                rep.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }
            .ok_or("Cannot encode the browser image")?;
            image::load_from_memory(&png.to_vec()).map_err(|e| e.to_string())
        })();
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(result);
        }
    });
    // The configuration, which a picture of the whole document needs for the
    // size to take in.
    let configuration: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> = match region {
        Region::Visible => None,
        Region::Whole([width, height]) => {
            let class = objc2::runtime::AnyClass::get(c"WKSnapshotConfiguration")
                .ok_or("The browser cannot take a picture of the whole page")?;
            unsafe {
                let configuration: objc2::rc::Retained<objc2::runtime::AnyObject> =
                    objc2::msg_send![class, new];
                let rect = objc2_foundation::NSRect::new(
                    objc2_foundation::NSPoint::new(0.0, 0.0),
                    objc2_foundation::NSSize::new(width, height),
                );
                let _: () = objc2::msg_send![&*configuration, setRect: rect];
                Some(configuration)
            }
        }
    };
    let given: *const objc2::runtime::AnyObject = configuration
        .as_ref()
        .map_or(std::ptr::null(), |configuration| &raw const **configuration);
    unsafe {
        let _: () = objc2::msg_send![&*view.webview(), takeSnapshotWithConfiguration: given, completionHandler: &*done];
    }
    rx.await
        .map_err(|_| "The browser image was cancelled".to_string())?
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn snapshot(_view: &WebView, _region: Region) -> Result<image::DynamicImage, String> {
    Err("Element images are not available on this platform".into())
}

/// The page as a PDF, laid out as for print.
pub(super) async fn pdf(view: &WebView) -> Result<Vec<u8>, String> {
    tokio::time::timeout(std::time::Duration::from_secs(30), export(view))
        .await
        .map_err(|_| "The browser did not finish the PDF in time".to_string())?
}

#[cfg(target_os = "linux")]
async fn export(view: &WebView) -> Result<Vec<u8>, String> {
    use dioxus::desktop::wry::WebViewExtUnix;
    use glib::object::ObjectType;
    use gtk::prelude::*;
    use gtk::{gio, glib};
    type Sender = tokio::sync::oneshot::Sender<Result<(), String>>;
    unsafe extern "C" {
        fn webkit_print_operation_new(
            view: *mut glib::gobject_ffi::GObject,
        ) -> *mut glib::gobject_ffi::GObject;
        fn webkit_print_operation_set_print_settings(
            operation: *mut glib::gobject_ffi::GObject,
            settings: *mut glib::gobject_ffi::GObject,
        );
        fn webkit_print_operation_print(operation: *mut glib::gobject_ffi::GObject);
    }
    unsafe extern "C" fn finished(
        _operation: *mut glib::gobject_ffi::GObject,
        data: *mut std::ffi::c_void,
    ) {
        let tx = unsafe { Box::from_raw(data.cast::<Sender>()) };
        let _ = tx.send(Ok(()));
    }
    unsafe extern "C" fn failed(
        _operation: *mut glib::gobject_ffi::GObject,
        error: *mut glib::ffi::GError,
        data: *mut std::ffi::c_void,
    ) {
        let tx = unsafe { Box::from_raw(data.cast::<Sender>()) };
        let text = if error.is_null() {
            "The browser could not print the page".to_string()
        } else {
            let error: glib::Error = unsafe { glib::translate::from_glib_none(error) };
            error.to_string()
        };
        let _ = tx.send(Err(text));
    }
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = directory.path().join("page.pdf");
    // Printing to a file, with no dialog, is a print operation whose settings
    // name the file and say it is a PDF.
    let settings = gtk::PrintSettings::new();
    settings.set("printer", Some("Print to File"));
    settings.set("output-file-format", Some("pdf"));
    settings.set("output-uri", Some(&gio::File::for_path(&file).uri()));
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    // SAFETY: the view is alive for the whole call, the operation is owned
    // here and released after printing, and exactly one of the two signals
    // ends it, taking the sender its data names.
    unsafe {
        let operation = webkit_print_operation_new(view.webview().as_ptr().cast());
        webkit_print_operation_set_print_settings(operation, settings.as_ptr().cast());
        let data = Box::into_raw(Box::new(tx)).cast::<std::ffi::c_void>();
        let connect = |name: &std::ffi::CStr, handler: unsafe extern "C" fn()| {
            glib::gobject_ffi::g_signal_connect_data(
                operation,
                name.as_ptr(),
                Some(handler),
                data,
                None,
                0,
            );
        };
        connect(
            c"finished",
            std::mem::transmute::<
                unsafe extern "C" fn(*mut glib::gobject_ffi::GObject, *mut std::ffi::c_void),
                unsafe extern "C" fn(),
            >(finished),
        );
        connect(
            c"failed",
            std::mem::transmute::<
                unsafe extern "C" fn(
                    *mut glib::gobject_ffi::GObject,
                    *mut glib::ffi::GError,
                    *mut std::ffi::c_void,
                ),
                unsafe extern "C" fn(),
            >(failed),
        );
        webkit_print_operation_print(operation);
        // The signals hold the operation until they fire.
        glib::gobject_ffi::g_object_unref(operation);
    }
    rx.await
        .map_err(|_| "The PDF was cancelled".to_string())??;
    std::fs::read(&file).map_err(|e| format!("The PDF could not be read back: {e}"))
}

#[cfg(target_os = "macos")]
async fn export(view: &WebView) -> Result<Vec<u8>, String> {
    use dioxus::desktop::wry::WebViewExtMacOS;
    use objc2_foundation::{NSData, NSError};
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::cell::RefCell::new(Some(tx));
    let done = block2::RcBlock::new(move |data: *mut NSData, error: *mut NSError| {
        let result = match unsafe { data.as_ref() } {
            Some(data) if error.is_null() => Ok(data.to_vec()),
            _ => Err("The browser could not export the page as a PDF".to_string()),
        };
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(result);
        }
    });
    unsafe {
        let _: () = objc2::msg_send![&*view.webview(), createPDFWithConfiguration: std::ptr::null::<objc2::runtime::AnyObject>(), completionHandler: &*done];
    }
    rx.await.map_err(|_| "The PDF was cancelled".to_string())?
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn export(_view: &WebView) -> Result<Vec<u8>, String> {
    Err("A PDF cannot be made on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_encoded_as_the_kind_the_agent_asked_for() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            4,
            3,
            image::Rgba([10, 200, 30, 255]),
        ));
        let png = encode_as(&image, ImageFormat::Png).unwrap();
        assert_eq!(&png[..4], b"\x89PNG");
        // A JPEG has no transparency, which the page's picture has.
        let jpeg = encode_as(&image, ImageFormat::Jpeg).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((back.width(), back.height()), (4, 3));
    }

    #[test]
    fn element_images_clip_to_the_visible_viewport_and_use_snapshot_pixels() {
        assert_eq!(
            crop_rect([-10.0, 20.0, 40.0, 30.0], [100.0, 100.0], [200, 200]).unwrap(),
            [0, 40, 60, 60]
        );
        assert_eq!(
            crop_rect([90.0, 95.0, 30.0, 20.0], [100.0, 100.0], [200, 200]).unwrap(),
            [180, 190, 20, 10]
        );
        assert!(crop_rect([110.0, 0.0, 20.0, 20.0], [100.0, 100.0], [200, 200]).is_err());
        assert!(crop_rect([0.0, 0.0, f64::NAN, 20.0], [100.0, 100.0], [200, 200]).is_err());
    }
}
