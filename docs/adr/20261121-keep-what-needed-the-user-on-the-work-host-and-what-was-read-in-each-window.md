# Keep what needed the user on the work host and what was read in each window

Status: Accepted

## Context

A session's state tells how it stands now. The user who is away while several
sessions finish, fail or ask something finds only their latest states, and
nothing to say which of them needed attention, in what order or whether it was
already looked at. The window only knows what it saw while it was open, so
what happened between two windows, or while no window was open, cannot be
rebuilt from it.

## Decision

The work host records each time a session begins waiting on the user, fails or
finishes a turn, as a notice that stands by itself, and keeps a bounded number
of the newest ones across its own restarts. Which notices the user has read is
kept by each window, not by the host. Telling the user outside the window is
decided by the window, from the notices its host sends, as the window's
settings say.

## Alternatives considered

- Keep read marks on the host, shared by every window. One window's reading
  would hide the notices from another, and the user could not leave one
  window's list untouched. A window cannot tell whether another user of the
  same host has read.
- Build the history in each window from the states it sees. Each window would
  hold a different history, one opened late holds none, and a state seen twice
  after a reconnection would be told twice.
- Keep no history and only mark the state of the sessions. A state is replaced
  by the next, so what needed the user once is lost.

## Consequences

- A window that opens late, or reconnects, lists what happened before it
  looked, and is not told of it again outside the window.
- A notice says what happened then; whether its session still stands as the
  notice says is judged against the sessions as they are listed now, so a past
  wait never looks like a current one.
- Read marks and the host's notices meet only by a notice's number, so a
  window forgets the marks of notices its host has dropped.
