# Attach file editor GUIs to persistent Neovim terminals

Status: Accepted

## Context

Users want their normal Neovim configuration rendered as a GUI inside a file
tab. Editors and unsaved buffers must survive closing the desktop window.

## Decision

Attach a window-owned GUI to the existing host-owned Neovim terminal. Keep
process lifetime and configuration with the terminal, and retain terminal
rendering whenever a local GUI cannot attach.

## Alternatives considered

- Start a new editor in each window: closing a window would lose its unsaved work.
- Open an external Neovim GUI: file editing would leave the workspace tabs.
- Replace the terminal host with an editor host: duplicates its process lifetime
  and recovery responsibilities and changes remote execution unnecessarily.

## Consequences

- Windows can reconnect to the same editor without restarting its plugins.
- The terminal remains usable by the CLI and remote clients.
- GUI input, rendering and resizing share the editor with its existing terminal UI.
