# Run local issues on the work host

Status: Accepted

## Context

People need a place to leave requests for their agents without using a
hosting service. Their work should keep going when they turn their attention
elsewhere or close the workspace window.

## Decision

Run local issues on the work host, which owns the queue and the sessions
processing it. Windows register and present the work rather than owning its
execution.

## Alternatives considered

- Let each window process its own list: closing the window stops progress,
  and two windows can start the same request.

## Consequences

- Connected windows share one account of the queued work.
- Issue execution uses the existing session lifetime and agent permissions.
- Recovering the queue must distinguish a request never started from work
  whose session may already have run.
