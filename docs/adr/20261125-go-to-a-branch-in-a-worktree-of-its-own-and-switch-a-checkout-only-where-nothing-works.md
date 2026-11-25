# Go to a branch in a worktree of its own, and switch a checkout only where nothing works

Status: Accepted

## Context

Tsumugi gives each line of work a worktree, and the tree lists a repository's
worktrees under it. A branch that already existed, a teammate's or one made
outside Tsumugi, could be reached only by making a worktree with a new branch
started from it, or by running `git switch` in a terminal under the sessions
that work in that checkout. Switching a checkout changes the files an agent is
reading and writing, and leaves its session believing it is on the branch it
began on.

## Decision

Choosing a branch goes to the worktree that has it checked out, and where none
has, makes a worktree that checks that branch out, as it is, without making
another. Switching the checkout a worktree is on to another branch is offered
beside this, and is refused while an agent works in that worktree, while it has
uncommitted changes, or while Git has an operation in it unfinished.

## Alternatives considered

- Only switch the checkout, as a branch menu does in an editor. Every session
  of the worktree would find its files replaced under it.
- Only go to worktrees. A checkout used by hand, with no agent in it, could not
  follow a branch without a terminal.
- Make a new branch from the one chosen, as making a worktree did. The work
  would land on a branch nobody asked for, and the chosen branch would never
  move.

## Consequences

- One branch is checked out in at most one worktree, as Git requires, so a
  branch is reached by one row of the tree.
- A worktree made for a branch that existed keeps that branch when the
  worktree is deleted.
