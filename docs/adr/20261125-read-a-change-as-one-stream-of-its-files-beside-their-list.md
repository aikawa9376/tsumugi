# Read a change as one stream of its files beside their list

Status: Accepted

## Context

Reviewing what an agent did on a branch means reading every file it changed,
against what the branch will be merged into. The diff tab showed one file at
a time, so the reviewer lost their place each time they moved to another file
and never saw the change as a whole. Orca's combined diff and hunk's review
stream show the whole change in one scroll, and a move to the next hunk
crosses into the next file.

## Decision

A diff tab shows a change, every file of it one after another in one scroll,
beside the list of those files. The change is chosen as a pair of sides: the
worktree against its branch's base by default, or one commit, or any two
revisions. Each file in the stream is still read and drawn on its own, so
comments, notes, viewed marks and staging stay pinned to one file's line
diff. How a worktree's diffs are drawn is kept for the worktree, not for the
tab, while the window runs.

## Alternatives considered

- One file at a time beside the list. The reviewer loses their place on every
  move and cannot read the change through.
- One patch for the whole change, drawn as a single text. A picture, a binary
  file or difftastic's view of one file could not be drawn in its place, and
  nothing could be pinned to a file.
- Keep the choices in each tab. Opening the change in another tab would lose
  them.

## Consequences

- Each file of a large change is cut to the lines a diff may show.
- The file list marks where the stream is, and opening a file moves the
  stream to it.
- Choices about drawing are lost when the window closes.
