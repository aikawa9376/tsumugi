use super::annotations::Element;
use dioxus::desktop::wry::WebView;

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
    if view.url().ok().as_deref() != Some(url) {
        return Err("The page changed before its image was captured".into());
    }
    let image = tokio::time::timeout(std::time::Duration::from_secs(5), snapshot(view))
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
async fn snapshot(view: &WebView) -> Result<image::DynamicImage, String> {
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
            0,
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
async fn snapshot(view: &WebView) -> Result<image::DynamicImage, String> {
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
    unsafe {
        let _: () = objc2::msg_send![&*view.webview(), takeSnapshotWithConfiguration: std::ptr::null::<objc2::runtime::AnyObject>(), completionHandler: &*done];
    }
    rx.await
        .map_err(|_| "The browser image was cancelled".to_string())?
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn snapshot(_view: &WebView) -> Result<image::DynamicImage, String> {
    Err("Element images are not available on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

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
