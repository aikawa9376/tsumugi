# Search the picker's sources in scopes the user mixes and enters by prefix

Status: Accepted

## Context

The picker went to a session or a directory. Finding a project's file, a
resource, a worktree or an action meant leaving it for another surface, and
the user wanted one fuzzy finder over what belongs to the projects the window
shows, switchable between sources, mixing them and shaped by the user.
Orca, which Tsumugi follows, splits this over a jump palette for worktrees and
tabs and a quick open for files.

## Decision

The picker has one list that searches **scopes**: a scope is a named mix of
sources, and the rows of a scope's sources are listed in the order it names
them. The user steps through the scopes with a key, or enters one by typing
its prefix first, and writes their own scopes in the settings, replacing
those Tsumugi starts with. The sources are what the window already knows or
can have the work host list for the project shown: sessions, directories,
worktrees, resources, files and actions. The window ranks every source's rows
by the same fuzzy match; the work host only lists the files.

## Alternatives considered

- A separate palette for each source, as Orca has. It cannot mix sources, and
  every source would cost a key of its own.
- A fixed set of mixes. What to mix is a matter of habit, and the user asked
  to shape it.
- A search on the work host for every source. The rows are small and the
  window already holds all but the files, so a second ranking on the host
  would only have to agree with the first.

## Consequences

- A new source is one more name for scopes to list, with its rows ranked in
  the window.
- A source that needs the work host to read something asks it only while a
  scope that searches it is in use.
- What a prefix enters is the user's to rename, so no prefix can be relied on
  to mean the same on another machine.
