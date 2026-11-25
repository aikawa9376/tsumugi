# Open the Linux window through X11 when kitty draws its consoles

Status: Accepted

## Context

kitty draws a console in a window of its own, which Tsumugi embeds in its
window through X11. A Wayland window cannot hold another program's window, so
kitty can draw only in a window opened through X11 or XWayland. Wayland
desktops commonly ask GTK programs to prefer Wayland, and a window started
from them opened on Wayland, could not embed kitty and fell back to the
built-in display without the user learning why.

## Decision

When kitty is the chosen console display and an X server is reachable, the
Linux window opens through X11 whatever backend the desktop prefers. Only
the window does: the programs it starts keep the desktop's preference.

## Alternatives considered

- Leave the backend to the user's environment. Choosing kitty then does
  nothing on most Wayland desktops unless the user also changes how Tsumugi
  is launched, which nothing in Tsumugi tells them.
- Open every Linux window through X11. Users who do not draw with kitty
  would lose native Wayland scaling and input for nothing.
- Embed kitty on Wayland. Wayland has no way for one client to place another
  client's surface inside its own window.

## Consequences

- A window drawing with kitty runs under XWayland, with XWayland's scaling
  and input.
- Choosing kitty in a window already open on Wayland takes effect only when
  Tsumugi starts again.
- Without an X server, the window opens as before and keeps the built-in
  display.
