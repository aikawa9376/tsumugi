# Start any other command-line agent from a profile that names its command

Status: Accepted

## Context

Terminal agents multiply faster than Tsumugi can write an adapter for each:
one that starts it, asks about its login and reads its conversations. Most of
them need none of that to be useful in a session: they start with a command,
take a first prompt in one of a few ways and draw a screen. Orca lists about
forty such agents in one launch picker. Tsumugi ran five.

## Decision

Any command-line agent runs as a session of one more kind, which is started
by the command its profile names and nothing else. Tsumugi reads how it
stands from the status it reports or from its screen, keeps no login for it,
reads none of its conversations, and starts no sessions of its own through
it. A catalog of well-known agents gives each its command and how it takes
its first prompt, so one starts by name without a profile being written.

A session of this kind fails to start, saying why, when its profile names no
command or when the program is not found where it starts.

## Alternatives considered

- Write an adapter for each agent in the catalog. Each adapter claims
  knowledge of the agent's conversations, models and hooks that Tsumugi
  cannot check, and adding one agent becomes a release.
- Leave them to a shell tab. Such a session would have no state, no place in
  the tree or the board, and no first prompt.

## Consequences

- A command agent has no model, permission profile or read-only mode, since
  Tsumugi knows nothing of what its program can do.
- Its state is only as good as the screen rules or the status it reports; the
  user may replace the rules.
- A profile may replace a catalog entry by taking its id.
