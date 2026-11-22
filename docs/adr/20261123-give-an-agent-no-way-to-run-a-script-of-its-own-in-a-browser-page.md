# Give an agent no way to run a script of its own in a browser page

Status: Accepted

## Context

The window reads a page for an agent with scripts of its own, which are written
to leave out what a page keeps from agents, such as what a password field
holds. Orca also lets an agent hand the browser a script to evaluate, as its
`eval` command and the `--fn` condition of `wait` do.

## Decision

No browser command takes a script from an agent. Each command asks the page a
fixed question the window wrote, and an agent chooses only its arguments.

## Alternatives considered

- Offer `eval` and a script condition for `wait`, as Orca does. A script of the
  agent's own reads anything the page holds, including what the fixed scripts
  withhold, so the promise that password fields are never read to an agent
  would depend on the agent's behaviour.
- Take scripts but filter their results. What a script may read cannot be told
  from its text, and a filter of its answer would have to guess which of the
  page's values are secret.

## Consequences

- An agent can wait only on what the commands name: an element, text, an
  address, a load state.
- Anything the commands do not ask of a page is out of an agent's reach until a
  command is made for it.
