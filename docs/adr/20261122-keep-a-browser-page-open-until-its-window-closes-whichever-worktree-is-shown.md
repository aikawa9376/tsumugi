# Keep a browser page open until its window closes, whichever worktree is shown

Status: Accepted

## Context

A page holds what a reload cannot bring back: its scroll position, unfinished
input, the in-memory state of the application under development and the notes
taken on it. Each worktree has a layout of its own, put away when another
worktree is selected, and Orca restores a worktree's browser tabs as the user
left them.

## Decision

A browser page stays open, hidden while its worktree's layout is put away,
until its own window closes. Pages are told apart by their own identity, never
by a window number, which the layouts of different worktrees each count from
the start.

## Alternatives considered

- Close a page with the layout it is in and load its last address when the
  worktree comes back. The state above is lost, and a development server may
  answer differently the second time.
- Keep one view for each environment and point it at the address of the
  worktree shown. Two worktrees would share one history and one set of
  unfinished input, and could not show different pages at once.

## Consequences

- Memory grows with the pages open in all worktrees; only closing a page
  releases it.
- A page kept hidden still runs its scripts, so what a page asks of the
  workspace is answered only while its window is the one shown.
