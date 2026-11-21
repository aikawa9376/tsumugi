# Make and delete worktrees from the tree, in the background and only with the loss told

Status: Accepted

## Context

A worktree could be made only by a session starting in a new one, and ended
only by closing what ran in it. Making one to work in, or clearing away one
that was done with, took git on a shell. Orca, which Tsumugi follows in how the
tree is worked, makes and deletes them from its sidebar.

## Decision

The tree makes and deletes worktrees. A new worktree is asked for in a dialog
and made in the background, its row in the tree showing how far it has come,
and a session's draft opens in it once it is made. Deleting a worktree first
tells the person what it loses, and a worktree with changes is never deleted
unless they answer to discard them; a branch holding commits nothing else has
is kept unless they choose to delete it.

Git runs on the work host that owns the repository, so the same asks work for
a work machine as for this computer. Making a worktree is a sequence of steps
of which a failure takes back what the earlier ones made.

## Alternatives considered

- Hold the dialog open until the worktree is made. The window waits on git,
  a fetch of a remote branch included, and the person cannot do anything else
  meanwhile.
- Run git in the window. A worktree of a work machine's repository could not
  be made.
- Delete with force whenever the person confirms. A confirmation that does not
  say what is lost is not one, and a race that makes new changes after the
  question would be lost too.

## Consequences

- A worktree made from the tree is listed only once something runs in it, as
  every worktree is.
- A step can be added to the sequence, such as a repository's own setup,
  without changing how the dialog, the row or the deletion work.
- Deleting asks even where nothing is at risk.
