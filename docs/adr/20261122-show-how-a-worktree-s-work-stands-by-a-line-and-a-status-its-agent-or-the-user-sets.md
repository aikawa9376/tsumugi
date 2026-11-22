# Show how a worktree's work stands by a line and a status its agent or the user sets

Status: Accepted

## Context

A tree of many worktrees says which sessions run in each, not how the work
stands: whether it waits on review, is blocked, or is done. The agent that
works there knows, and Orca lets it say so on the worktree's card in a
sentence and a status.

## Decision

A worktree keeps one line of text and one status of a fixed few, which the
agent working in it, the user or a command sets, and which the work host
keeps with the worktree rather than with a session. While no line is set, the
latest note an agent wrote on its timeline in the worktree stands in for it.
Every window shows the line beside the worktree's name and can narrow the tree
to a status.

## Alternatives considered

- Derive the line from the session's timeline alone. A timeline is the plan of
  one session; its latest note is a step, not where the work stands, and a
  worktree outlives its sessions. It is only what shows while nothing better
  is set.
- Keep it with the session. The status would be lost with the session that
  set it, and two sessions in one worktree would disagree.
- Make the status the list of states of the issue that started the work.
  Not every worktree has an issue, and an issue's state is its own.

## Consequences

- What a worktree says stays after its sessions end, until it is cleared.
- A status is a fact someone set, not one derived, so it can be stale.
