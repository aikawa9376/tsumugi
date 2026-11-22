# Reach a work machine's loopback from a page only through a forward for the page's environment

Status: Accepted

## Context

A page renders on the computer showing Tsumugi, where `localhost` is that
computer. A development server of a remote environment listens on the work
machine's loopback, and two environments may each have a server on the same
port number. A page that took `localhost:3000` at its word would show this
computer's server, or another environment's, as if it were the worktree's.

## Decision

Every address a page is pointed at, typed or given whole by a port or a
resource, has its loopback port replaced by a local port forwarded over the
paired connection to the same port of the page's own environment. An address
that no forward can make reach the work machine is refused, not opened here.

## Alternatives considered

- Open the address as given. It would reach this computer, which is a
  different machine from the one the worktree runs on.
- Send all of a page's traffic through a proxy that ends at the work machine,
  as Orca does for remote workspaces, so that links and sockets the page opens
  by itself reach the work machine too. WebViews can take a proxy on each
  platform, but whether they send loopback addresses and sockets to it was
  not shown, and an unproven proxy would turn a refusal into a silent leak.
- Make the page's loopback port equal to the work machine's by binding the
  same number here. The number may be in use on this computer, and then the
  page would reach whatever uses it.

## Consequences

- Links and scripts of the page that name `localhost` themselves are not
  rewritten and reach this computer; only the address the page was pointed at
  is carried to the work machine.
- A forward ends with the connection to the work machine, so a page loaded
  through it is left pointing at a closed port after a reconnection until it
  is pointed at the address again.
- Browsing needs no second transport or browser runtime on the work machine.
