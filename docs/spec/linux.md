# Linux desktop

Tsumugi provides a desktop workspace on Linux. Settings, state, data, service
registrations and discovery use its own product namespace. Explicit advanced
routing overrides still select the endpoints they name.

Copy uses the active desktop session's clipboard. Files and URLs open through
the desktop's default handlers; revealing a file opens its enclosing directory.
Notifications use the desktop notification service. PDF pages use the system's
Ghostscript renderer. These operations require their respective system helpers.

The Linux installer provides a desktop launcher, icon, executable and licenses
under the chosen prefix. Background service registration is a separate action.

The main workspace uses one themed header in place of Linux's native title and
menu bars. The Tsumugi button opens the retained application menu. The header
supports moving, minimizing, maximizing/restoring and closing the window;
its edges and corners support resizing. macOS keeps its native decorations.
Embedded Neovims find the runtime belonging to their executable, independently
of an editor from which Tsumugi was launched.

## Display scaling

The workspace and Settings window automatically adapt to their current display.
System display scaling takes precedence; unscaled high-resolution displays get
an additional application scale. Moving or resizing a window does not overwrite
the user's zoom preference or compound a previously selected automatic scale.

Automatic scaling is enabled for existing configurations as well as new ones.
Settings → Window can disable it or adjust Zoom relative to the automatic size.
Turning it off uses the user's Zoom alone. Display preferences belong to this
computer, independently of a remote work machine's settings.

## Shell and file editing

Shell tabs use the account's login shell and inherit the environment it normally
uses to find startup files, history and plugins. Explicit terminal launch
settings override that environment. A named application instance separates
workspace state without changing the shell's configuration directories.

Local Neovim file tabs draw the editor's GUI in the tab and read the user's
normal Neovim configuration. The editor stays in its terminal host after a
window closes, including unsaved buffers. A remote editor or one whose GUI
cannot yet attach keeps its terminal display and input available. The prompt
and console reader retain their separate configurations.
