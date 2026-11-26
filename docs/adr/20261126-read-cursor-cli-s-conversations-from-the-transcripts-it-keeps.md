# Read Cursor CLI's conversations from the transcripts it keeps

Status: Accepted

## Context

Cursor CLI is a terminal agent like the others Tsumugi runs, and a session's
conversation is read in Tsumugi for every one of them. Cursor documents how
it starts, resumes a conversation and picks a model, but not where it keeps
its conversations or how they are laid out, and it names its conversation
neither in its terminal nor through a plugin of Tsumugi's. It does keep a
transcript of each conversation, with the user's words, its replies and the
calls it made, but not what the calls returned or when anything was said.

## Decision

Cursor CLI runs through an adapter that starts it, asks whether its login
stands, reads how it stands from its screen by rules Tsumugi ships, and reads
its conversations from the transcripts it keeps. A session's conversation is
the one it began in its directory, when only one did. Tsumugi loads no
integration into Cursor and lists no models or commands of its own for it.

## Alternatives considered

- Read nothing of its conversations until Cursor documents them. A Cursor
  session would be the one whose conversation cannot be read in Tsumugi.
- Create the conversation before starting Cursor, with its own command for
  that, and start Cursor on it. Every session would wait on a second program
  before it starts, and a session resumed or begun in Cursor's own interface
  would still have to be found.
- Tie it to Tsumugi through its hooks. Which of them run in its interface is
  not documented, and a hook that does not run would leave the session
  looking idle while it works.

## Consequences

- Cursor's calls show without what they returned or when they were made.
- Two Cursor sessions begun in one directory at once leave their
  conversations unread until one is told apart.
- A release that changes the transcripts' place or shape, or the screen,
  leaves the conversation or the state unknown until the reader or the rules
  follow.
