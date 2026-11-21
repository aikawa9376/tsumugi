# Run Cursor CLI as an agent known from its screen alone

Status: Accepted

## Context

Cursor CLI is a terminal agent like the four Tsumugi runs. Its documentation
tells how it starts, resumes a conversation and picks a model, and that it
runs hooks. It does not tell where it writes its conversations or how they
are laid out, nor whether it reports its status to the terminal. Tsumugi's
other integrations each read such records or load a plugin of their own, and
Cursor's cannot be written against what is not documented.

## Decision

Cursor CLI runs through an adapter that starts it, asks whether its login
stands, and reads its state from its screen by rules Tsumugi ships. Tsumugi
reads nothing of its conversations, lists no models or commands of its own
for it, and loads no integration into it, until those are documented or
shown by a session that runs.

## Alternatives considered

- Wait until a Cursor session can be observed. Cursor can be started and read
  from its screen now, which is all the other agents had when they were added.
- Read its conversations from the files it keeps. Their location and format
  are undocumented and may change without notice.
- Tie it to Tsumugi through its hooks. Which of them run in its interface is
  not documented, and a hook that does not run would leave the session
  looking idle while it works.

## Consequences

- A Cursor session shows no links, files or conversation in the details
  panel, cannot be resumed from Tsumugi's list of past conversations, and
  starts no sessions of its own.
- The screen rules are the only way Tsumugi knows how a Cursor session
  stands, so a release that changes its screen leaves the state unknown until
  the rules follow.
