# Keep the floating terminal's shells on the work host as terminals the tree does not list

Status: Accepted

## Context

A user wants a shell at hand over whatever worktree is shown, that opens and
closes with one key without ending what runs in it, and without taking a place
in a worktree's list of sessions and terminals. Every shell Tsumugi runs is
held by the terminal host of its instance and listed by the work host, so a
window that closes or restarts loses nothing; a shell kept only by the window
would lose its program with it.

## Decision

The floating terminal's shells are terminals of the work host like any other,
marked as floating when started. The tree leaves floating terminals out; the
window reads the panel's tabs off the same listing as every terminal, and
holds only whether the panel is open and has the keys. Each environment's work
host holds its own floating shells.

## Alternatives considered

- Hold the shells in the window and start them as it needs them. Closing or
  restarting the window would end their programs, and a work machine's
  shells could not be reached from a second window.
- List them in the tree under a worktree, shown in a panel. They would belong
  to the worktree shown when they started, and every worktree's row would
  change as the user moved about.
- Start them without the terminal host. They would not survive a restart of
  the window or the work host, which every other terminal does.

## Consequences

- Hiding the panel, switching worktrees and restarting the window leave the
  shells and their output as they are.
- Anything that lists terminals without the tree, such as the command line,
  sees floating shells; anything that lists the tree does not.
