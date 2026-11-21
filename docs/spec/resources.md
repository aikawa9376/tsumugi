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
