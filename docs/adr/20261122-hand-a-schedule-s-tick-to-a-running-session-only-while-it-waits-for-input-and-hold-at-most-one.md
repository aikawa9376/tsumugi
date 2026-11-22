# Hand a schedule's tick to a running session only while it waits for input and hold at most one

Status: Accepted

## Context

A supervisor that opened workers is woken by their reports, but a report that
never comes — a worker stuck without noticing, a supervisor that missed one —
leaves the work looking stopped. The supervisor's own agent had a timer for
this inside its conversation; it vanished with the session, did not fire during
a turn, and left duplicate ticks behind. A schedule already gives the work host
a timetable that outlives windows and restarts.

## Decision

A schedule can name a running session instead of the queue, and the work host
hands it the schedule's prompt as a tick. A tick is given to the session only
while it waits for input. Times that come due while it works are held as one
tick, and while a tick has not reached the session no other is added. The
work host keeps the held tick, so nothing is lost or given twice.

## Alternatives considered

- Keep the timer in the supervisor's agent. It ends with the session, is blind
  to the host restarting, and cannot tell a turn from a pause.
- Deliver every time that comes due. A long turn would be followed by a pile of
  ticks the supervisor has to answer one by one.
- Skip a time that comes due while the session works. A supervisor that works
  for longer than the interval would then be ticked only by chance, at the next
  time it happens to wait.
- Poll the workers' consoles from the host and wake the supervisor on change.
  That is what the workers' own reports already do; the tick is for what they
  do not say.

## Consequences

- A tick costs the supervisor one turn, never more than one at a time, however
  long it worked.
- A tick can be late by as long as the supervisor's turn, and is never early.
