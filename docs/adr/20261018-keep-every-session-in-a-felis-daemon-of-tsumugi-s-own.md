# Keep every session in a felis daemon of Tsumugi's own

Status: Accepted

## Context

Sessions must outlive the window, and each must show its program's own
terminal interface: Claude Code's now, other agents' and plain shells' as
well. Keeping a program alive past the window takes a process that holds its
pseudo-terminal, and showing it takes a terminal emulator that follows the
protocols rich interfaces use, Kitty graphics and the Kitty keyboard among
them. felis is a terminal whose daemon does both: it owns each session's
pseudo-terminal and emulator, keeps them while no window is attached, and
serves the screen, images and input to clients over a socket, with a client
library that needs no window or GPU.

## Decision

Every session, an agent's or a shell's, runs in a felis daemon. Tsumugi starts
that daemon itself, from the felis it is built with, on a socket of its own,
and is a felis client: it lists, starts, tags and ends sessions through the
daemon, and draws each one it shows from the screen and images the daemon
sends it.

## Alternatives considered

- Keep sessions alive with each agent's own mechanism, such as Claude Code's
  background sessions. Every agent would need its own way, or none would be
  available, and shells would still die with the window.
- Run programs on pseudo-terminals Tsumugi owns and emulate them in-process.
  Sessions would die with the window, and Tsumugi would carry the emulator, the
  key encoding and the image handling the daemon already has.
- Use the felis daemon the user may already run. Its version is not Tsumugi's,
  and Tsumugi would depend on felis being installed.
- A multiplexer such as tmux. It re-encodes the program's output for an outer
  terminal and drops the protocols felis keeps.

## Consequences

- Closing Tsumugi detaches; the sessions and their scrollback stay in the
  daemon until they end.
- Tsumugi has no emulator, pseudo-terminal or key encoder of its own; how a
  terminal behaves is felis's.
- felis is not published and promises no API, so it is pinned to a git
  revision, and a new revision may need Tsumugi changed.
- A daemon left running by an older Tsumugi may speak an older protocol than a
  newer Tsumugi.
- What felis lacks for Tsumugi is asked of felis, with a stopgap in Tsumugi
  until it lands.
