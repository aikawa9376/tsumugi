//! An optional native display for terminal tabs. The terminal host still owns
//! the screen and input modes; changing the display never starts a program.

use dioxus::prelude::*;
use tsumugi_core::content::Tab;
use tsumugi_term::Size;

#[derive(Clone, Copy)]
pub(crate) struct NativeSize(Signal<Option<(Tab, Size)>>);

/// Subscribes to the native size, independently of the page font's metrics.
pub(crate) fn size_for(tab: &Tab) -> Option<Size> {
    try_consume_context::<NativeSize>().and_then(|size| {
        size.0
            .read()
            .as_ref()
            .filter(|(shown, _)| shown == tab)
            .map(|(_, size)| *size)
    })
}

#[cfg(target_os = "linux")]
mod kitty;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use kitty::available as kitty_available;
#[cfg(target_os = "linux")]
pub(crate) use kitty::display as kitty_display;
#[cfg(target_os = "linux")]
pub(crate) use linux::{available, use_native_layer};

#[cfg(not(target_os = "linux"))]
pub(crate) fn available() -> Result<(), String> {
    Err("VTE is available on Linux only".into())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn kitty_available() -> Result<(), String> {
    Err("Embedded kitty is available on Linux with X11/XWayland only".into())
}

/// The GDK backends a window whose consoles `renderer` draws opens with, given
/// the X server `display` names: kitty's window is embedded through X11,
/// which a Wayland window cannot hold. Without an X server, opening through
/// X11 would leave no window at all.
#[cfg(any(target_os = "linux", test))]
fn backends(
    renderer: tsumugi_core::settings::TerminalRenderer,
    display: Option<&std::ffi::OsStr>,
) -> Option<&'static str> {
    let reachable = display.is_some_and(|d| !d.is_empty());
    (renderer == tsumugi_core::settings::TerminalRenderer::Kitty && reachable).then_some("x11")
}

/// Opens this window through X11 when kitty is to draw its consoles. GDK
/// reads this once, as the window opens, so it must come first.
#[cfg(target_os = "linux")]
pub(crate) fn choose_backend() {
    let Some(preferences) = crate::client_settings::chosen() else {
        return;
    };
    if let Some(chosen) = backends(
        preferences.console.renderer,
        std::env::var_os("DISPLAY").as_deref(),
    ) {
        // Only this process: the work it starts keeps the user's backend.
        gtk::gdk::set_allowed_backends(chosen);
    }
}

#[cfg(any(target_os = "linux", test))]
fn presents(renderer: tsumugi_core::settings::TerminalRenderer, tab: Option<&Tab>) -> bool {
    use tsumugi_core::settings::TerminalRenderer::{Kitty, Vte};
    matches!(
        (renderer, tab),
        (Kitty, Some(Tab::Session(..) | Tab::Terminal(..))) | (Vte, Some(Tab::Terminal(..)))
    )
}

#[cfg(any(target_os = "linux", test))]
fn frame(screen: &tsumugi_term::Screen, focused: bool) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("\x1b[?25l\x1b[0m");
    let bg = screen.background;
    let _ = write!(out, "\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b);
    for row in 0..screen.rows {
        let _ = write!(out, "\x1b[{};1H\x1b[2K", row + 1);
        let Some(line) = screen.lines.get(row) else {
            continue;
        };
        let mut col = 0;
        for run in &line.runs {
            let style = run.style;
            let bg = style.bg.unwrap_or(screen.background);
            let fg = style.fg;
            // Place runs at the host's columns despite font fallback widths.
            let _ = write!(
                out,
                "\x1b[{};{}H\x1b[0m\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m",
                row + 1,
                col + 1,
                fg.r,
                fg.g,
                fg.b,
                bg.r,
                bg.g,
                bg.b
            );
            for (on, code) in [
                (style.bold, 1),
                (style.italic, 3),
                (style.underline, 4),
                (style.strike, 9),
            ] {
                if on {
                    let _ = write!(out, "\x1b[{code}m");
                }
            }
            out.extend(
                run.text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c }),
            );
            col += run.width;
        }
        let _ = write!(out, "\x1b[0m\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b);
    }
    if let Some(cursor) = screen.cursor.filter(|_| focused) {
        let shape = match cursor.shape {
            tsumugi_term::CursorShape::Block => 2,
            tsumugi_term::CursorShape::Underline => 4,
            tsumugi_term::CursorShape::Beam => 6,
        } - u8::from(cursor.blink);
        let _ = write!(
            out,
            "\x1b[{};{}H\x1b[{shape} q\x1b[?25h",
            cursor.row + 1,
            cursor.col + 1
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_term::{Cursor, CursorShape, Line, Palette, Run, Screen, Style};

    #[test]
    fn a_window_drawing_with_kitty_opens_through_x11_when_an_x_server_is_there() {
        use std::ffi::OsStr;
        use tsumugi_core::settings::TerminalRenderer::{Builtin, Kitty, Vte};
        let display = Some(OsStr::new(":0"));
        assert_eq!(backends(Kitty, display), Some("x11"));
        assert_eq!(
            backends(Kitty, None),
            None,
            "without an X server it stays where GDK puts it"
        );
        assert_eq!(backends(Kitty, Some(OsStr::new(""))), None);
        assert_eq!(backends(Builtin, display), None);
        assert_eq!(
            backends(Vte, display),
            None,
            "VTE also draws in a Wayland window"
        );
    }

    #[test]
    fn kitty_presents_agent_sessions_and_terminals_without_covering_other_content() {
        use tsumugi_core::settings::TerminalRenderer::{Builtin, Kitty, Vte};
        let environment = tsumugi_model::EnvironmentId::unbound();
        let session = Tab::Session(environment.clone(), "agent".into());
        let terminal = Tab::Terminal(environment.clone(), 1);
        let draft = Tab::draft(environment, "/repo");
        assert!(presents(Kitty, Some(&session)));
        assert!(presents(Kitty, Some(&terminal)));
        assert!(!presents(Kitty, Some(&draft)));
        assert!(!presents(Kitty, None));
        assert!(!presents(Vte, Some(&session)));
        assert!(presents(Vte, Some(&terminal)));
        assert!(!presents(Builtin, Some(&session)));
        assert!(!presents(Builtin, Some(&terminal)));
    }

    fn sample() -> Screen {
        let palette = Palette::default();
        Screen {
            cols: 8,
            rows: 2,
            lines: vec![
                Line {
                    runs: vec![Run {
                        text: "日本\x1b".into(),
                        width: 5,
                        wide: true,
                        apart: true,
                        style: Style {
                            fg: palette.foreground,
                            bg: None,
                            bold: true,
                            italic: true,
                            underline: true,
                            strike: true,
                        },
                    }],
                },
                Line { runs: vec![] },
            ],
            cursor: Some(Cursor {
                row: 1,
                col: 2,
                shape: CursorShape::Beam,
                blink: false,
            }),
            background: palette.background,
        }
    }

    #[test]
    fn repaint_clears_old_text_preserves_styles_and_does_not_interpret_cells_as_controls() {
        let rendered = frame(&sample(), true);
        assert!(rendered.contains("日本 "));
        assert!(rendered.contains("\x1b[38;2;"));
        assert!(rendered.contains("\x1b[1m"));
        assert!(rendered.contains("\x1b[3m"));
        assert!(rendered.contains("\x1b[4m"));
        assert!(rendered.contains("\x1b[9m"));
        assert!(
            rendered.contains("\x1b[2;1H\x1b[2K"),
            "an empty row erases what it held"
        );
        assert!(rendered.contains("\x1b[2;3H\x1b[6 q\x1b[?25h"));
        assert!(
            !rendered.contains('\n'),
            "painting the last row must not scroll it"
        );
    }

    #[test]
    fn an_unfocused_display_hides_its_cursor_without_moving_the_programs_cursor() {
        let screen = sample();
        assert!(!frame(&screen, false).contains("\x1b[?25h"));
        assert_eq!(screen.cursor.unwrap().col, 2);
    }
}
