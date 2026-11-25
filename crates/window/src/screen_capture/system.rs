//! The operating system's ways to capture what its screens show.

/// Every screen of the machine, as the system captures it: one image of all
/// of them where the system captures them together.
#[cfg(target_os = "linux")]
pub(super) async fn screens() -> Result<Vec<image::RgbaImage>, String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return x11().map(|image| vec![image]);
    }
    // The desktop portal is how a Wayland compositor lets applications
    // capture its screens; wlroots compositors without one offer grim.
    if let Some(image) = portal().await? {
        return Ok(vec![image]);
    }
    if let Some(image) = grim().await? {
        return Ok(vec![image]);
    }
    Err(
        "This desktop lets no application capture its screens: install a desktop portal with \
         screenshots for it, or grim"
            .into(),
    )
}

#[cfg(target_os = "macos")]
pub(super) async fn screens() -> Result<Vec<image::RgbaImage>, String> {
    // `screencapture` writes one file for each screen, as many as it is given.
    const MOST: usize = 16;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let files: Vec<_> = (0..MOST)
        .map(|n| dir.path().join(format!("{n}.png")))
        .collect();
    let status = tokio::process::Command::new("/usr/sbin/screencapture")
        .arg("-x")
        .arg("-tpng")
        .args(&files)
        .status()
        .await
        .map_err(|e| format!("screencapture: {e}"))?;
    if !status.success() {
        return Err(format!("screencapture: {status}"));
    }
    let images: Vec<_> = files
        .iter()
        .filter(|file| file.exists())
        .map(|file| {
            image::open(file)
                .map(|i| i.into_rgba8())
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    if images.is_empty() {
        // Without the screen recording permission nothing is written.
        return Err(
            "Tsumugi may not record the screen: allow it in System Settings, Privacy & Security, \
             Screen & System Audio Recording"
                .into(),
        );
    }
    Ok(images)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) async fn screens() -> Result<Vec<image::RgbaImage>, String> {
    Err("Capturing the screen is not available on this system".into())
}

/// The screens as the X server's root window shows them.
#[cfg(target_os = "linux")]
fn x11() -> Result<image::RgbaImage, String> {
    use gtk::gdk;
    use gtk::gdk::prelude::*;
    let screen = gdk::Screen::default().ok_or("There is no screen to capture")?;
    let root = screen
        .root_window()
        .ok_or("There is no screen to capture")?;
    let (width, height) = (root.width(), root.height());
    let pixbuf = root
        .pixbuf(0, 0, width, height)
        .ok_or("The screen could not be captured")?;
    let png = pixbuf
        .save_to_bufferv("png", &[])
        .map_err(|e| e.to_string())?;
    image::load_from_memory(&png)
        .map(|i| i.into_rgba8())
        .map_err(|e| e.to_string())
}

/// The screens through the desktop portal, none where the desktop has no
/// portal that takes screenshots.
#[cfg(target_os = "linux")]
async fn portal() -> Result<Option<image::RgbaImage>, String> {
    use gtk::gio;
    use gtk::gio::prelude::*;
    use gtk::glib;
    const DESKTOP: &str = "org.freedesktop.portal.Desktop";
    let bus = gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|e| e.to_string())?;
    let Some(sender) = bus.unique_name() else {
        return Ok(None);
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let token = format!("tsumugi{}_{nanos}", std::process::id());
    // The portal answers on a request named by the caller and the token: it
    // is listened to before asking, so the answer cannot come first.
    let request = format!(
        "/org/freedesktop/portal/desktop/request/{}/{token}",
        sender.trim_start_matches(':').replace('.', "_")
    );
    let (tx, rx) = tokio::sync::oneshot::channel::<glib::Variant>();
    let tx = std::cell::RefCell::new(Some(tx));
    let listening = bus.signal_subscribe(
        Some(DESKTOP),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(&request),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, answer| {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(answer.clone());
            }
        },
    );
    let options = glib::VariantDict::new(None);
    options.insert("handle_token", token.as_str());
    options.insert("interactive", false);
    let args = ("", options.end()).to_variant();
    let asked = bus
        .call_future(
            Some(DESKTOP),
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Screenshot",
            "Screenshot",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            5_000,
        )
        .await;
    if let Err(error) = asked {
        bus.signal_unsubscribe(listening);
        // A remote error's message starts with its D-Bus name.
        let message = error.message();
        let absent = [
            "org.freedesktop.DBus.Error.ServiceUnknown",
            "org.freedesktop.DBus.Error.UnknownMethod",
            "org.freedesktop.DBus.Error.UnknownInterface",
            "org.freedesktop.DBus.Error.UnknownObject",
        ]
        .iter()
        .any(|name| message.contains(name));
        return if absent {
            Ok(None)
        } else {
            Err(message.to_string())
        };
    }
    // The desktop may ask the user first.
    let answer = tokio::time::timeout(std::time::Duration::from_secs(120), rx).await;
    bus.signal_unsubscribe(listening);
    let answer = answer
        .map_err(|_| "The desktop did not answer the capture".to_string())?
        .map_err(|_| "The desktop did not answer the capture".to_string())?;
    let Some((response, results)) = answer.get::<(u32, glib::VariantDict)>() else {
        return Err("The desktop's answer to the capture is not understood".into());
    };
    match response {
        0 => {}
        1 => return Err("The capture of the screen was cancelled".into()),
        _ => return Err("The desktop refused to capture the screen".into()),
    }
    let uri = results
        .lookup::<String>("uri")
        .ok()
        .flatten()
        .ok_or("The desktop's capture names no file")?;
    let path = gio::File::for_uri(&uri)
        .path()
        .ok_or("The desktop's capture is not a file")?;
    image::open(&path)
        .map(|i| Some(i.into_rgba8()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The screens through grim, none where it is not installed.
#[cfg(target_os = "linux")]
async fn grim() -> Result<Option<image::RgbaImage>, String> {
    let out = match tokio::process::Command::new("grim")
        .args(["-t", "png", "-l", "0", "-"])
        .output()
        .await
    {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("grim: {e}")),
    };
    if !out.status.success() {
        return Err(format!(
            "grim: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    image::load_from_memory_with_format(&out.stdout, image::ImageFormat::Png)
        .map(|i| Some(i.into_rgba8()))
        .map_err(|e| format!("grim: {e}"))
}
