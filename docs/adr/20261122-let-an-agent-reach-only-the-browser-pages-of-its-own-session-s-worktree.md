# Let an agent reach only the browser pages of its own session's worktree

Status: Accepted

## Context

Browser pages hang under worktrees, and several sessions work in a
repository at once, each answering to the user. A page may hold a signed-in
site or the application under development of another line of work.

## Decision

A browser command acts on the pages of the worktree holding the directory the
calling session works in, in the environment it works in, and on no other. No
command names a worktree or a page of anyone else's: pages are counted within
the caller's own worktree.

## Alternatives considered

- Let a command name any worktree, as Orca's does. One session could then
  read or act on a page another session depends on, which the user never
  allowed it.
- Give each session pages of its own. Sessions in one worktree work on the
  same application and are meant to look at the same page; the user watches
  pages by worktree.

## Consequences

- Sessions working in the same worktree share its pages and can disturb each
  other's.
- A session working outside every worktree the window lists has no pages to
  reach.
