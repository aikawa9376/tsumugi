# Try a local issue as several attempts, each in a worktree of its own, and keep the one the user merges

Status: Accepted

## Context

Agents differ in how well they do a given task, and the same agent does it
differently from one run to the next. Today an issue is worked by one session,
so trying another agent means waiting for the first, setting its work aside by
hand and starting again. Tsumugi already makes a worktree per session, reads a
worktree's change against its base, merges a worktree into the main checkout
and deletes a worktree with its loss told.

## Decision

A local issue can be started as several attempts at once. Each attempt is a
session started for the issue, with a profile of its own, in a new worktree
made from the same commit, so every attempt begins from the same code and none
sees another's work. The issue keeps its attempts and stands by them together.
The user compares them in the diff tabs that already read a worktree's change,
and chooses one by merging its worktree, which closes the issue; the others are
then offered for discarding, their loss told, and are never discarded without
that answer.

## Alternatives considered

- Run the attempts one after another in the issue's directory. Each attempt
  sees what the one before it left, and nothing can be compared until the last
  ends.
- A comparison view of its own that lays every attempt's change side by side.
  It repeats the diff tab, its keys, comments and notes, for a gain that two
  diff tabs and a diff of two attempts already give.
- Let an agent judge the attempts and merge the best. Choosing what goes into
  the main checkout stays the user's; a leader session can still be asked to
  read the attempts and say which it would take.

## Consequences

- An issue is linked to several sessions, not one, and its state is drawn
  from all of them.
- Attempts work only in Git, since each needs a worktree.
- Attempts run beside each other and beside other work in the directory, so
  they count against the limits on the sessions a work machine carries, not
  against the queue's one request per directory.
- The attempts not chosen stay, with their worktrees and branches, until the
  user discards them or cleaning up finds they have nothing to lose.
