# Let a session end only the sessions it opened, and tell the user it did

Status: Accepted

## Context

A session that supervises sessions it opened could end only itself. Once a
worker was done, the user closed it by hand or asked the worker to close
itself, which the worker could rightly treat as a message not from the user.
Supervisors want what Orca's orchestration gives its coordinator: stopping and
releasing the workers it started.

## Decision

A session can end a session it opened, and no other. A session the user opened,
one another session opened and one that has ended are refused. Work in hand or
uncommitted changes in the session's directory refuse the end until the caller
insists. What the caller did stays visible to the user in the sessions
themselves.

## Alternatives considered

- **Any session may end any session.** Sessions are peers; one could cut off
  work another's user is steering.
- **Only the user ends sessions.** The supervisor keeps depending on a hand or
  on a message the worker may ignore.
- **Ask the user each time.** A supervisor opened its workers on the user's
  authority already; asking again brings back the wait this removes.

## Consequences

- Ending a worker is not a failure the supervisor is told of.
- A session's opener is the one session a message can come from with that
  standing, so messages say when they come from it.
