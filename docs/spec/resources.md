# Resources

A project's resources are the paths and addresses you keep for it: a
configuration file, a design document, a site. Only the reference is kept,
never a copy of what it points at, and each is shown by a title: the label
you gave it, else the file's name or the address.

They belong to the project, the workspace holding the directory, so every
worktree of a repository, every window and every session of the project sees
the same ones. Tsumugi's work host keeps them, and they outlive windows and
sessions.

The details panel's Resources tab lists them under Pinned
([details](details.md#components)).

## Keeping and opening them

You keep a resource from the details panel as well as by asking an agent:

- **Add resource…** (`resource.add`, from the panel's right-click menu, the
  menu bar or its key) opens a sheet for a path or an address, a label and a
  note. A relative path is taken from the worktree shown. Keeping a target
  the project keeps already keeps one resource.
- **Keep as resource** (`resource.keep`) on a link, a file or an image file
  the session came across keeps that. It says so when the project keeps it
  already.
- **Capture the screen** (`resource.capture`) keeps a picture of a part of
  what the screen shows, Tsumugi's window and other applications alike.
  **Capture the screen later** (`resource.capture_later`) waits a few seconds
  first, to bring forward what the picture should show
  ([reference](reference/settings.md#details)). Once it is taken, Tsumugi's
  window comes forward with a still of the screen: drag over the part to keep
  and let go, press Enter to keep the whole screen, or Escape to keep nothing.
  Where the system captures each screen apart, Tab shows the next one. The
  picture is a file on the work machine, which nothing removes, and the
  resource points at it. It needs the system to let Tsumugi capture the
  screen: on macOS, the screen recording permission; on Wayland, a desktop
  portal that takes screenshots, or grim.
- **Edit label and note…** and **Remove from resources** are on a pinned row's
  right-click menu. Removing leaves what the resource points at as it is.

Opening a pinned row shows it in the content, where it opens in the tab shown,
a new tab or a split as the keys say. A path opens as a row of the file tree
does: in its viewer, or in Neovim. A web address opens a page of the browser
under the worktree shown. Any other address is left to the system to open.

## What agents do with them

Every session is told to look the resources up when you point at material
loosely, as in "check the conf link", instead of guessing where it is. Two
tools of the `tsumugi` MCP server ([sessions](opening-sessions.md#what-a-session-can-do))
serve that:

- **`resources`** lists the project's resources, each with its title, target,
  whether it is a path or an address, its note, and whether a path still
  exists. A `query` of words narrows it to those whose title, target or note
  hold every word, in any case. It can name another project by one of its
  directories.
- **`remove_resource`** stops keeping one, by the id `resources` lists, and
  a session does it only when you ask it to.
- **`add_resource`** keeps a path or an address, a relative path taken from
  the caller's directory. A session does it only when you ask it to. Keeping
  a target the project keeps already does not make another: it keeps the
  label and note you did not give again.

A path that no longer exists stays listed and is reported missing.
