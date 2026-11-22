//! Showing a page at the size of a phone, a tablet or a laptop: the native
//! view is given that size, centred in the room the page has, so the page
//! sees the width in `window.innerWidth` and its media queries.

use std::cell::RefCell;
use std::collections::HashMap;

use tsumugi_core::content::BrowserId;

/// A size a page can be shown at, as Chrome's device toolbar names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub width: u32,
    pub height: u32,
}

pub(super) const PRESETS: [Preset; 7] = [
    Preset {
        id: "mobile-s",
        label: "Mobile S — 320 × 568",
        width: 320,
        height: 568,
    },
    Preset {
        id: "mobile-m",
        label: "Mobile M — 375 × 667",
        width: 375,
        height: 667,
    },
    Preset {
        id: "mobile-l",
        label: "Mobile L — 425 × 812",
        width: 425,
        height: 812,
    },
    Preset {
        id: "tablet",
        label: "Tablet — 768 × 1024",
        width: 768,
        height: 1024,
    },
    Preset {
        id: "laptop",
        label: "Laptop — 1024 × 768",
        width: 1024,
        height: 768,
    },
    Preset {
        id: "laptop-l",
        label: "Laptop L — 1440 × 900",
        width: 1440,
        height: 900,
    },
    Preset {
        id: "desktop",
        label: "Desktop — 1920 × 1080",
        width: 1920,
        height: 1080,
    },
];

thread_local! {
    /// The preset each page is shown at; a page left out fills its room. The
    /// page outlives the window that shows it, so its size does too.
    static CHOSEN: RefCell<HashMap<BrowserId, &'static str>> = RefCell::new(HashMap::new());
}

/// The preset by this id.
pub(super) fn preset(id: &str) -> Option<Preset> {
    PRESETS.iter().copied().find(|p| p.id == id)
}

/// What an agent's device name means: a preset, or the page's room again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Device {
    Preset(Preset),
    Responsive,
}

impl Device {
    /// The id `choose` takes.
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Preset(preset) => preset.id,
            Self::Responsive => "responsive",
        }
    }
}

/// The device `name` names, by a preset's id (`mobile-m`) or by its name
/// without its size (`Mobile M`), in any case; or `Responsive`.
pub(super) fn device(name: &str) -> Result<Device, String> {
    let wanted = name.trim().to_ascii_lowercase();
    let same = |other: &str| {
        let squash = |text: &str| {
            text.to_ascii_lowercase()
                .split(|c: char| c == '-' || c.is_whitespace())
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        squash(other) == squash(&wanted)
    };
    if same("responsive") {
        return Ok(Device::Responsive);
    }
    PRESETS
        .iter()
        .find(|preset| same(preset.id) || same(preset.label.split(" — ").next().unwrap_or("")))
        .map(|preset| Device::Preset(*preset))
        .ok_or_else(|| {
            let names = PRESETS
                .iter()
                .map(|preset| preset.label.split(" — ").next().unwrap_or(preset.id))
                .chain(["Responsive"])
                .collect::<Vec<_>>()
                .join(", ");
            format!("There is no device {name:?}: the sizes are {names}")
        })
}

/// The id of the preset the page `page` is shown at, or none for its room.
pub(super) fn chosen(page: BrowserId) -> Option<&'static str> {
    CHOSEN.with(|chosen| chosen.borrow().get(&page).copied())
}

/// Show `page` at the preset by this id, or at the size of its room when none
/// is.
pub(super) fn choose(page: BrowserId, id: &str) {
    CHOSEN.with(|chosen| {
        let mut chosen = chosen.borrow_mut();
        match preset(id) {
            Some(preset) => chosen.insert(page, preset.id),
            None => chosen.remove(&page),
        };
    });
}

/// Where the page is drawn in `room`, as `[x, y, width, height]`: at the
/// size `preset` says, as far as the room holds it, centred across and at
/// the top.
pub(super) fn fit(room: [f64; 4], preset: Option<Preset>) -> [f64; 4] {
    let Some(preset) = preset else {
        return room;
    };
    let [x, y, width, height] = room;
    let (w, h) = (
        f64::from(preset.width).min(width),
        f64::from(preset.height).min(height),
    );
    [x + (width - w) / 2.0, y, w, h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_without_a_preset_fills_its_room() {
        assert_eq!(
            fit([10.0, 20.0, 800.0, 600.0], None),
            [10.0, 20.0, 800.0, 600.0]
        );
    }

    #[test]
    fn a_preset_smaller_than_the_room_is_centred_across_and_at_the_top() {
        let phone = preset("mobile-m");
        assert_eq!(
            fit([10.0, 20.0, 800.0, 900.0], phone),
            [10.0 + (800.0 - 375.0) / 2.0, 20.0, 375.0, 667.0]
        );
    }

    #[test]
    fn a_preset_larger_than_the_room_is_cut_to_the_room() {
        let desktop = preset("desktop");
        assert_eq!(
            fit([0.0, 0.0, 800.0, 600.0], desktop),
            [0.0, 0.0, 800.0, 600.0]
        );
        assert_eq!(
            fit([0.0, 0.0, 800.0, 400.0], preset("tablet")),
            [16.0, 0.0, 768.0, 400.0]
        );
    }

    #[test]
    fn each_page_keeps_the_preset_chosen_for_it() {
        let (one, two) = (BrowserId::new(), BrowserId::new());
        assert_eq!(chosen(one), None);
        choose(one, "tablet");
        choose(two, "laptop");
        assert_eq!(chosen(one), Some("tablet"));
        assert_eq!(chosen(two), Some("laptop"));
        choose(one, "responsive");
        assert_eq!(chosen(one), None);
        assert_eq!(chosen(two), Some("laptop"));
    }

    #[test]
    fn a_device_is_named_by_a_presets_id_or_its_name_in_any_case_or_is_responsive() {
        for name in ["mobile-m", "Mobile M", "  mobile m ", "MOBILE-M"] {
            assert_eq!(device(name).map(Device::id), Ok("mobile-m"), "{name:?}");
        }
        assert_eq!(device("Laptop L").map(Device::id), Ok("laptop-l"));
        assert_eq!(device("laptop").map(Device::id), Ok("laptop"));
        assert_eq!(device("Tablet").map(Device::id), Ok("tablet"));
        assert_eq!(device("Responsive"), Ok(Device::Responsive));
        assert_eq!(Device::Responsive.id(), "responsive");
    }

    #[test]
    fn an_unknown_device_is_refused_with_the_names_there_are() {
        let failed = device("iPhone 12").unwrap_err();
        assert!(failed.contains("iPhone 12"), "{failed}");
        for preset in ["Mobile S", "Tablet", "Desktop", "Responsive"] {
            assert!(failed.contains(preset), "{failed}");
        }
        assert!(device("").is_err());
        assert!(device("mobile").is_err());
    }

    #[test]
    fn the_presets_are_told_apart_by_their_ids() {
        for (at, preset) in PRESETS.iter().enumerate() {
            assert_eq!(PRESETS.iter().position(|p| p.id == preset.id), Some(at));
        }
    }
}
