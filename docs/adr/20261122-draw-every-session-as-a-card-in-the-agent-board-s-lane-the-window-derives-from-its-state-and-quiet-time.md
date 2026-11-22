# Draw every session as a card in the agent board's lane the window derives from its state and quiet time

Status: Accepted

## Context

The Agents panel lists sessions grouped by who opened them, as a column too
narrow to tell what each one last said or to see at a glance which need the
user. Orca's agent dashboard shows the same sessions as a board of lanes for
what needs the user, what works, what is done and what has gone quiet, and
Tsumugi is moving toward Orca's design while keeping its own keys.

A session's reported state has no lane for "quiet": a finished session stays
finished however long ago it was.

## Decision

The agent board is a window of the content, not a section of the sidebar. The
window derives each session's lane itself from the state the host reports,
whether the user has seen it finish, and how long it has stood so; a finished
session the user has seen goes to the idle lane once it has been quiet for half
an hour. The cursor holds a session, not a place, so a card that changes lane
keeps it.

## Alternatives considered

- Draw lanes in the Agents panel. The panel's width cannot hold a message
  preview, and a board is read as a whole rather than beside the work.
- Have the work host say when a session went quiet. Every window and the
  phone would share one answer, but the host would have to keep a clock for
  every session and every client would have to learn a new state, for a
  distinction only the board draws.
- Hold the cursor by its place in a lane. A card that moves lane would then
  take the cursor to a different session, and the user would be answering the
  wrong one.

## Consequences

- Idle is the window's own reading: it counts from the last change the window
  saw, or from the session's last input or start where it saw none, so a
  window opened late does not make every old session look new.
- The cards of a shown board make the host read every listed session's
  conversation, which a card quotes from.
