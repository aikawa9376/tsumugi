# Limits on sessions and builds, and clearing worktrees

Sessions that open sessions, each building in a worktree of its own, can load
a work machine past what it carries. The `[governor]` table of the
[settings](settings.md) ([reference](reference/settings.md)) holds the limits
you may set. All are off until you set them: nothing changes for settings
that say nothing.

## Sessions

`max_sessions` is the most agent sessions that run at once on a work machine.
A session another session opens waits for room under it, for `wait_seconds`;
once that is up, the start is refused, telling the opener how many run, and it
leaves nothing behind. Sessions you open yourself are never held, though they
count among those that run.

## Builds

Sessions another opens are started with the build limits set in their
environment:

- `build_jobs` sets `CARGO_BUILD_JOBS`.
- `build_cache` says how worktrees of a repository share what they build:
  `own`, each in its own `target/`, as by hand; `shared`, in one folder
  Tsumugi keeps for the repository (`CARGO_TARGET_DIR`), where builds go one at
  a time; or `sccache`, each in its own, with `sccache` as the compiler
  wrapper. The gate ([the central gate](gate.md)) builds as this says too.

## Size

`worktree_warn_gib` is the size, in GiB, of a repository's worktrees above
which listing them for a cleanup carries a warning, naming the largest. It
only warns.

## Clearing away worktrees

A worktree is done with when deleting it would lose nothing: it has no
changes, no commit that neither a remote nor the main checkout has, no
session or terminal in it, and is neither the main checkout nor locked.
`tsumugi cleanup` lists the worktrees of the repository with the ones done
with and why each other stays, and the `cleanup_worktrees` tool does the
same for a session. With `--remove`, or `remove` for the tool, it deletes the
ones done with, and no other: nothing is asked and nothing is forced, so a
change made after the list was drawn keeps its worktree. A branch is deleted
with its worktree only when `cleanup_branches` is set, and only when it is
merged.

Deleting a worktree that has something to lose stays your decision, made from
the tree where the loss is told
([worktrees](tree.md#making-and-deleting-worktrees)).
