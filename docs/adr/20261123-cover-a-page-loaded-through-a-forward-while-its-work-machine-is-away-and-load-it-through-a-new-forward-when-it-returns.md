# Cover a page loaded through a forward while its work machine is away and load it through a new forward when it returns

Status: Accepted

## Context

A page of a remote environment that shows a development server reaches it
through a forward that lasts only as long as the connection to the work
machine
(`20261122-reach-a-work-machine-s-loopback-from-a-page-only-through-a-forward-for-the-page-s-environment.md`).
When the connection drops, the page stays on screen, looking alive, and
anything it does fails against a closed port. Orca's browser marks such a page
with the host it goes through and covers it when that host is not connected.

## Decision

While the work machine of a page loaded through a forward is not connected,
the page is covered by a notice that says so and offers a retry, and its
toolbar shows the work machine it goes through. The address stays what the
work machine writes, whatever the forward was. When the connection returns, or
on a retry, the page is loaded at that address through a forward made again.

## Alternatives considered

- Leave the page showing what it last drew. It looks usable and every action
  in it fails without saying why.
- Keep the forward's local port reserved across a reconnection so the page
  could simply reload. Nothing guarantees the port is still free, and a page
  that reached whatever took it would be worse than one that waits.

## Consequences

- The page's address field is not rewritten by a disconnection, so what was
  typed into it and what it shows survive until the work machine returns.
- A page of this computer, or of an address the work machine's loopback is not
  needed for, is never covered.
