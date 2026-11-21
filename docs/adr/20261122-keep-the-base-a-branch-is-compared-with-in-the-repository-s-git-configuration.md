# Keep the base a branch is compared with in the repository's Git configuration

Status: Accepted

## Context

The Changes list compares a branch with the branch it will be merged into, and
the user can choose that base for each branch. Worktrees of one repository are
made and read from different windows and environments, and the work host reads
them through Git alone.

## Decision

A base chosen for a branch is written to the repository's own Git
configuration, under that branch, and nowhere in Tsumugi's state.

## Alternatives considered

- Keep it in Tsumugi's own state, by worktree. A worktree made again, or read
  from another environment, would forget it, and the state would have a second
  copy of what the repository can hold.
- Take the base from the branch's upstream. A pushed feature branch's upstream
  is itself, which says nothing about what it will be merged into.

## Consequences

- The choice follows the branch to every worktree of the repository and
  survives Tsumugi being reinstalled.
- Git tools and scripts can read and set it.
- Without a choice, the base is found from the repository: the remote's
  default branch, else the usual names.
