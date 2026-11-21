# Install skills as stubs that load their guide from the tsumugi that runs

Status: Accepted

## Context

Tsumugi has guidance for agents, such as how to open a session beside the
user's. An agent reads a skill from files in its own skills folder, so a
skill written there carries the commands and flags of the tsumugi that wrote
it. Updating tsumugi leaves those files behind, and the agent follows
commands that are no longer right.

## Decision

A skill is installed as a short stub that says when to use it and tells the
agent to run `tsumugi skills get` to read the guide. The guide is part of the
tsumugi binary and is printed by the build that runs.

## Alternatives considered

- Write the whole guide into each agent's skills folder. It is correct only
  until tsumugi is updated, and every update would have to find and rewrite
  every copy, including the ones the user has since changed.
- Write the guide into the files again each time a session starts. Tsumugi
  would write into a folder it does not own on every start, and an agent
  already running keeps the text it read.
- Leave the guide out of the skills folder and tell sessions of it through a
  plugin only. That reaches the sessions tsumugi starts, not an agent the
  user runs in a terminal of their own.

## Consequences

- A guide changes with the binary, with nothing written to the user's
  folders.
- A stub changes rarely, so an installed copy rarely falls behind.
- An agent needs a `tsumugi` it can run to read a guide.
