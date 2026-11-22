# Never begin a merge into the main checkout that would conflict and resolve it in the branch's worktree instead

Status: Accepted

## Context

A finished branch is merged into the main checkout, where the user and other
sessions have work of their own: uncommitted changes, and a branch other
commits land on. A merge begun there that stops on a conflict leaves that
checkout in a state nobody chose, and whoever merges has to be there to untangle
it. Orca merges through the hosting provider and has no merge of its own to
copy.

## Decision

Tsumugi finds out whether a merge would conflict before it touches the main
checkout, and begins it there only when it would not. A merge that would
conflict leaves the main checkout alone and is resolved in the branch's own
worktree, by bringing the main checkout's branch into the branch, where it is
resolved, continued or aborted like any merge in progress. The main checkout's
uncommitted changes are never stashed, committed or overwritten unless the
person chose it.

## Alternatives considered

- Begin the merge in the main checkout and abort it on a conflict. The checkout
  is in a merge for as long as it takes to notice, and an abort that fails or
  never comes leaves it there.
- Stash the main checkout's changes by itself and pop them afterwards. A pop
  that conflicts leaves their changes mixed with the merge's.
- Rebase the branch onto the main checkout's, or fast-forward only. It
  rewrites commits another session may hold and leaves no merge commit to show
  where the work came in.

## Consequences

- The conflict is resolved by the session or person that knows the branch, in
  the worktree they are in, and the main checkout keeps moving meanwhile.
- A merge is made twice over when it conflicts: once into the branch, once
  into the main checkout.
