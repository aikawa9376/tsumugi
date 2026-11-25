# Capture a part of the desktop for a project's resources on a still of it

Status: Accepted

## Context

A screenshot could be kept among a project's resources only of the browser
page shown. What the user wants to keep is often elsewhere: a terminal, an
editor, a dialog, another application beside Tsumugi. They also need a moment
before the picture is taken, to bring forward what it should show or open a
menu.

## Decision

Tsumugi captures what the screen of the window's machine shows, Tsumugi and
other applications alike, through the operating system's screen capture,
after a delay the user chooses. The part to keep is chosen by dragging over a
still of that capture shown in Tsumugi's window, and is kept as a file on the
work machine with a resource pointing at it.

## Alternatives considered

- Capture only Tsumugi's window by drawing its views together. It needs no
  permission, but leaves out the other applications the user wants to keep.
- Leave choosing the part to the system's own picker. Each desktop has a
  different one or none, and the screen keeps changing under it while the
  user chooses.
- Keep the capture of the browser page only. Terminals, editors and other
  applications could not be kept.

## Consequences

- The window's machine must allow screen capture: macOS asks for the screen
  recording permission, and a Wayland compositor must offer a way to capture
  its screens.
- The picture is taken where the window runs and sent to the work host, which
  keeps it as the screenshots of the browser were.
- Tsumugi's window comes forward to have the part chosen, whatever the delay
  brought to the front.
