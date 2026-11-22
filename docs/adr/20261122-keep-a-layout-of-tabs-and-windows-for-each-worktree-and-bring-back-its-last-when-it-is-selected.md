# Keep a layout of tabs and windows for each worktree and bring back its last when it is selected

Status: Accepted

## Context

The content held one layout for the whole window: a tab took whatever was
opened in it, from any worktree. Moving between worktrees therefore left every
worktree's tabs in the way of the others, and a worktree's splits were lost as
soon as the user worked elsewhere. Orca keeps a layout of tabs and splits for
each worktree and swaps the whole tree of panes when the worktree changes.

## Decision

Each worktree has a layout of its own, its tabs and the windows they are split
into. Selecting a worktree puts away the layout shown, with the splits and
sizes it had, and shows the last layout that worktree had, or an empty one when
it had none. What is opened from a worktree opens in that worktree's layout.

Putting a layout away only hides it. What its windows showed keeps running and
stays attached to its program, exactly as when a window or a tab is closed, and
what a restored layout shows that has ended meanwhile is left out of it.

## Alternatives considered

- Keep the one layout and let a tab hold anything from any worktree. It leaves
  each worktree's splits at the mercy of the others, which is what this
  replaces.
- Keep one layout and hide the tabs of the worktrees not selected. A split can
  hold windows of several worktrees, so hiding some of its windows cannot give
  a worktree back the layout it left.

## Consequences

- A tab, and the windows in it, belong to one worktree's layout; the same thing
  opened from two worktrees is a tab in each.
- Things running are counted as shown while any layout, shown or put away,
  has a window on them, so a Neovim started on a file is not asked to quit
  because its worktree is not the one selected.
