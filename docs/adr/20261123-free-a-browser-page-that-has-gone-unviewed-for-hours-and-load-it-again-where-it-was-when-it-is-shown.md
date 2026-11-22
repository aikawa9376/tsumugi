# Free a browser page that has gone unviewed for hours and load it again where it was when it is shown

Status: Accepted

## Context

A page stays open until its window closes, whichever worktree is shown
(`20261122-keep-a-browser-page-open-until-its-window-closes-whichever-worktree-is-shown.md`),
so every worktree visited leaves a page with a live view behind it. Orca keeps
such pages the same way and frees the views of those left unviewed for a set
number of hours, enabled unless the user turns it off.

## Decision

A browser page that is not shown and has gone unviewed for a set number of
hours is freed: its view and the memory behind it are released while the page
stays open and listed. It keeps its address, its scroll position and the
address typed into its bar, and loads again at them when it is next shown. A
page an agent command is using, or one with unsent input or notes, is never
freed.

## Alternatives considered

- Keep every page loaded until it closes. Memory grows with each worktree
  visited, and nothing gives it back.
- Cap the number of loaded hidden pages and free the least recently used past
  it. A busy day frees pages that were just left, and an idle week frees
  nothing; how long a page has been left is what says it is not wanted.
- Free a page when its worktree's layout is put away. Switching worktrees back
  and forth would reload pages each time.

## Consequences

- What a reload cannot bring back is lost in a freed page, which is why a page
  with unsent input is kept; a development server may answer differently the
  second time.
- A command of an agent on a freed page loads it again first.
- Pages are told apart by their own identity, so freeing one never frees
  another worktree's.
