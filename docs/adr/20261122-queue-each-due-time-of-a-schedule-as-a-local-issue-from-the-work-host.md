# Queue each due time of a schedule as a local issue from the work host

Status: Accepted

## Context

Triage, upkeep and summaries are asked of an agent again and again, and each
time a person had to save the request and start it. Local issues already give
the work host a durable queue that runs one request at a time in each
directory and never starts what the user did not authorize.

## Decision

The work host keeps schedules and, each time one is due, queues its request as
an ordinary local issue that names the schedule. A schedule is made paused,
and enabling it is the user's authorization of its times from then on and of
none before; a saved issue never gains a schedule by itself. A time at which a
schedule was due is settled once, together with the issue it queues, and a
schedule never has more than one request unfinished in the queue.

## Alternatives considered

- Start a session directly at each time. It would bypass the order kept in
  each directory and the recovery of the queue, which would then be written a
  second time.
- Leave the timing to the operating system's scheduler. It does not follow the
  work host's environments, is not seen from the windows, and cannot tell
  whether the previous request is still working.
- Let a window hold the timer. Closing the window would stop the schedule, and
  two windows would run it twice.
- Run every missed time, or keep adding requests while one waits. A host that
  was off, or an agent waiting on an answer, would then leave a queue that
  grows without bound.

## Consequences

- Ordering, per-directory exclusion, restart recovery and permissions are those
  of local issues, with nothing added for scheduled work.
- A time that passes while the host is off runs once if it is recent enough,
  and is recorded as skipped if not.
- What a due time produces is separate from knowing that it is due, so a
  schedule can hand its prompt to something other than the queue.
