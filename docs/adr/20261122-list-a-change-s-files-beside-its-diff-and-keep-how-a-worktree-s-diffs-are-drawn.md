# List a change's files beside its diff and keep how a worktree's diffs are drawn

Status: Accepted

## Context

Orca's diff viewer shows a tree of the files changed beside the diff, moves
between changes by key, and remembers what the user chose about it. Tsumugi's
diff tab showed one file at a time and forgot its choices with the tab, so a
reviewer going through many files had to go back to a list for each one.

## Decision

A diff tab on a file lists the files of its change beside the diff, and what
the user chose about how it is drawn is kept for the worktree, not for the
tab.

## Alternatives considered

- Draw every file of the change in one scrolling document, as Orca's combined
  view does. A review comment is pinned to one file's line diff and its
  cursor, and difftastic reads one file at a time, so one document would
  rebuild both for no gain a list does not give.
- Keep the choices in each tab. Opening a file in a new tab, or closing and
  reopening one, would lose them, which is the complaint.
- Write the choices to a file. They are preferences of a view, not state worth
  surviving a restart, and a file would bind them to the settings schema.

## Consequences

- Choices are lost when the window closes.
- Moving between files is moving in the list, one file's diff at a time.
