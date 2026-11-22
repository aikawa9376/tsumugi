# Run an agent's browser commands in the window that renders the page, through the work host

Status: Accepted

## Context

A browser page renders on the computer showing Tsumugi, also for an
environment on a work machine. The agent that should drive it runs in a
session on the work machine and reaches Tsumugi only through the work host.

## Decision

A browser command from an agent goes to the work host, which hands it to a
window of its environment; the window carries it out on its own page and
answers the host, which answers the agent. With no window open, the command
fails at once and says that a window is needed, and one left unanswered
because its window closed fails the same way.

## Alternatives considered

- Run a headless browser on the work machine for agents, as Orca does for a
  workspace on a server. The agent would drive a different page than the one
  the user watches and shares state with, and every work machine would need a
  second browser runtime, which the decision to browse on the displaying
  computer avoided.
- Keep the command until a window opens. An agent would wait without end on a
  window nobody is going to open, with nothing to say why.

## Consequences

- An agent's browser works only while the user has Tsumugi open, and it drives
  the very pages the user sees.
- The work host holds no page state; a command in flight is lost with its
  window, which the agent is told.
