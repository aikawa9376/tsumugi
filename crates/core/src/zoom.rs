//! How far the window is zoomed, a level at a time.

/// The zoom levels, from the smallest to the largest; 1.0 is as designed.
pub const LEVELS: [f64; 9] = [0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75];

/// What a key asks of the zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    In,
    Out,
    Reset,
}

/// The zoom after `step` from `now`: the next level up or down, staying at
/// either end, or back to 1.0.
pub fn next(now: f64, step: Step) -> f64 {
    match step {
        Step::Reset => 1.0,
        Step::In => LEVELS
            .iter()
            .copied()
            .find(|l| *l > now + 1e-9)
            .unwrap_or(LEVELS[LEVELS.len() - 1]),
        Step::Out => LEVELS
            .iter()
            .rev()
            .copied()
            .find(|l| *l < now - 1e-9)
            .unwrap_or(LEVELS[0]),
    }
}

/// Extra application zoom for a high-resolution display whose desktop has
/// not already selected a display scale. Resolution is a fallback, not DPI.
pub fn automatic(width: u32, height: u32, system_scale: f64) -> f64 {
    if !system_scale.is_finite() || system_scale <= 0.0 || system_scale > 1.0 + 1e-6 {
        return 1.0;
    }
    match width.min(height) {
        4320.. => 3.0,
        2880.. => 2.0,
        2160.. => 1.5,
        1440.. => 1.25,
        _ => 1.0,
    }
}

/// Always derive from the user's adjustment, never a previously applied zoom.
pub fn for_display(user: f64, enabled: bool, width: u32, height: u32, system: f64) -> f64 {
    user * if enabled {
        automatic(width, height, system)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_through_the_levels_and_stops_at_either_end() {
        assert_eq!(next(1.0, Step::In), 1.1);
        assert_eq!(next(1.0, Step::Out), 0.9);
        assert_eq!(next(1.75, Step::In), 1.75);
        assert_eq!(next(0.67, Step::Out), 0.67);
        assert_eq!(next(1.5, Step::Reset), 1.0);
    }
    #[test]
    fn automatic_scaling_enlarges_unscaled_high_resolution_displays() {
        for (width, height, expected) in [
            (1920, 1080, 1.0),
            (2560, 1440, 1.25),
            (3840, 2160, 1.5),
            (2160, 3840, 1.5),
            (5120, 1440, 1.25),
            (5120, 2880, 2.0),
            (7680, 4320, 3.0),
        ] {
            assert_eq!(automatic(width, height, 1.0), expected, "{width}x{height}");
        }
    }

    #[test]
    fn system_scaling_takes_precedence_over_the_resolution_fallback() {
        for system in [1.25, 1.5, 2.0, 3.0] {
            assert_eq!(automatic(3840, 2160, system), 1.0);
        }
        for system in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(automatic(3840, 2160, system), 1.0);
        }
        assert_eq!(automatic(0, 0, 1.0), 1.0);
    }

    #[test]
    fn display_changes_recompute_from_the_user_adjustment_without_compounding() {
        assert_eq!(for_display(1.25, true, 3840, 2160, 1.0), 1.875);
        assert_eq!(for_display(1.25, true, 1920, 1080, 1.0), 1.25);
        assert_eq!(for_display(1.25, true, 3840, 2160, 1.0), 1.875);
        assert_eq!(for_display(1.25, false, 3840, 2160, 1.0), 1.25);
        assert_eq!(for_display(1.25, true, 3840, 2160, 2.0), 1.25);
    }
}
