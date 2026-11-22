# Clear away only the worktrees that have nothing to lose and ask nothing

Status: Accepted

## Context

Worktrees outlive the work they were made for, and each holds a build folder.
Deleting a worktree from the tree asks the user first and tells what it would
lose, which is right for one chosen by hand but too much to answer for every
worktree that is plainly done with.

## Decision

Cleaning up removes a worktree only when deleting it loses nothing: no changes,
no commit that neither a remote nor the main checkout has, no session or
terminal in it, not the main checkout and not locked. Every other worktree is
left and listed with what keeps it. Nothing is forced, so a change made after
the look is still kept by git. Deleting a worktree that would lose something
stays the user's decision, made where the loss is told.

## Alternatives considered

- Delete when the session of a worktree ends or its branch is merged. A
  session ending says nothing of what its worktree holds, and a merged branch
  can still have changes in its worktree.
- Delete with force after the user confirms a list. A list of many worktrees
  hides the one with the loss in it.

## Consequences

- Cleaning up needs no question and cannot lose work.
- A worktree that was made and never used is cleared away too, as it loses
  nothing and is made again by one ask.
- A branch goes with its worktree only by the user's setting, and only when
  merged.
