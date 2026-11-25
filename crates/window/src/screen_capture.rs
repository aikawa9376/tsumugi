//! Capturing the screens of the window's machine for a project's resources:
//! the stills taken, shown to choose a part of, and the part kept.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

use tsumugi_core::msg::Msg;
use tsumugi_model::EnvironmentId;

use crate::bridge::AppCtx;

mod system;

/// Waited at least before capturing, so the menu that asked for the capture
/// is drawn away first.
const SETTLE: Duration = Duration::from_millis(300);

/// What the stills are drawn with while a part is chosen: small enough to
/// hand the page quickly.
const SHOWN_QUALITY: u8 = 80;

/// A screen captured, whole, and as the page shows it.
struct Still {
    image: image::RgbaImage,
    shown: String,
}

thread_local! {
    static STILLS: RefCell<Vec<Still>> = const { RefCell::new(Vec::new()) };
}

/// The address the page draws the still of screen `screen` from, none when
/// there is no such still.
pub(crate) fn shown(screen: usize) -> Option<String> {
    STILLS.with(|stills| stills.borrow().get(screen).map(|s| s.shown.clone()))
}

/// Capture every screen once `delay` has gone by, and tell the view state
/// what was taken.
pub(crate) fn take(app: AppCtx, delay: Duration) {
    dioxus::prelude::spawn(async move {
        tokio::time::sleep(delay.max(SETTLE)).await;
        let taken = match system::screens().await {
            Ok(images) => {
                tokio::task::spawn_blocking(move || images.into_iter().map(still).collect())
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|stills: Result<Vec<Still>, String>| stills)
            }
            Err(text) => Err(text),
        };
        match taken {
            Ok(stills) => {
                let sizes = stills
                    .iter()
                    .map(|s| [s.image.width(), s.image.height()])
                    .collect();
                STILLS.with(|kept| *kept.borrow_mut() = stills);
                app.dispatch(Msg::CaptureTaken(sizes));
                // What the delay brought forward stays behind the stills.
                dioxus::desktop::window().set_focus();
            }
            Err(text) => app.dispatch(Msg::CaptureFailed(text)),
        }
    });
}

fn still(image: image::RgbaImage) -> Result<Still, String> {
    use base64::Engine;
    let mut jpeg = Vec::new();
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).into_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, SHOWN_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    let data = base64::engine::general_purpose::STANDARD.encode(jpeg);
    Ok(Still {
        image,
        shown: format!("data:image/jpeg;base64,{data}"),
    })
}

/// Let the stills go.
pub(crate) fn drop_stills() {
    STILLS.with(|stills| stills.borrow_mut().clear());
}

/// Keep `part` of the still of screen `screen` among the resources of the
/// project holding `dir`, and let the stills go.
pub(crate) fn keep(
    app: AppCtx,
    environment: EnvironmentId,
    dir: PathBuf,
    screen: usize,
    part: [u32; 4],
) {
    let image = STILLS.with(|stills| {
        let mut stills = stills.borrow_mut();
        let image = (screen < stills.len()).then(|| stills.swap_remove(screen).image);
        stills.clear();
        image
    });
    let Some(image) = image else {
        app.notice("The capture of the screen is gone".into());
        return;
    };
    dioxus::prelude::spawn(async move {
        let png = tokio::task::spawn_blocking(move || crop(&image, part))
            .await
            .map_err(|e| e.to_string())
            .and_then(|png| png);
        let png = match png {
            Ok(png) => png,
            Err(text) => {
                app.notice(text);
                return;
            }
        };
        use base64::Engine;
        let answer = app
            .ask_in(
                &environment,
                tsumugi_protocol::Ask::KeepScreenshot {
                    dir,
                    data: base64::engine::general_purpose::STANDARD.encode(png),
                    label: Some("Capture of the screen".into()),
                    note: None,
                },
            )
            .await;
        match answer {
            tsumugi_protocol::Answer::Done => app.notice("Kept the capture in resources".into()),
            tsumugi_protocol::Answer::Failed { text }
            | tsumugi_protocol::Answer::Unconfirmed { text } => app.notice(text),
            answer => app.notice(format!("the work host answered {answer:?}")),
        }
    });
}

/// `part` of `image`, its left, top, width and height, as a PNG; what lies
/// outside the image is left out.
fn crop(image: &image::RgbaImage, part: [u32; 4]) -> Result<Vec<u8>, String> {
    let [left, top, width, height] = part;
    let left = left.min(image.width());
    let top = top.min(image.height());
    let width = width.min(image.width() - left);
    let height = height.min(image.height() - top);
    if width == 0 || height == 0 {
        return Err("The part chosen holds nothing of the screen".into());
    }
    let part = image::imageops::crop_imm(image, left, top, width, height).to_image();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(part)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(png: &[u8]) -> image::RgbaImage {
        image::load_from_memory(png).unwrap().into_rgba8()
    }

    fn screen() -> image::RgbaImage {
        image::RgbaImage::from_fn(40, 30, |x, y| image::Rgba([x as u8, y as u8, 0, 255]))
    }

    #[test]
    fn the_part_kept_is_cut_from_the_screen_where_it_was_chosen() {
        let part = decoded(&crop(&screen(), [10, 5, 20, 15]).unwrap());

        assert_eq!(part.dimensions(), (20, 15));
        assert_eq!(part.get_pixel(0, 0).0, [10, 5, 0, 255]);
    }

    #[test]
    fn a_part_past_the_screen_keeps_only_what_lies_on_it() {
        let part = decoded(&crop(&screen(), [30, 20, 50, 50]).unwrap());

        assert_eq!(part.dimensions(), (10, 10));
    }

    #[test]
    fn a_part_off_the_screen_keeps_nothing() {
        assert!(crop(&screen(), [40, 0, 5, 5]).is_err());
    }

    #[test]
    fn a_still_is_shown_as_an_image_the_page_can_draw() {
        let shown = still(screen()).unwrap().shown;

        assert!(shown.starts_with("data:image/jpeg;base64,"));
    }
}
