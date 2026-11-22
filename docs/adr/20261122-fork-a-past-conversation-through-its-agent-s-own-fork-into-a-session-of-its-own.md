# Fork a past conversation through its agent's own fork into a session of its own

Status: Accepted

## Context

A past conversation could only be resumed, which continues it where it
stopped. To try another way from the same point, the user had to resume it and
let both histories grow in one conversation, or copy its text by hand into a
new one.

## Decision

Forking a past conversation runs the agent's own fork in a new session, which
continues a copy of the conversation. The conversation it copies is never
written to, and the copy is a session of its own, with a code and a
conversation that are not the original's. An agent with no fork of its own is
refused, naming why, and is not forked by another means.

## Alternatives considered

- Build a prompt from the transcript and start a fresh conversation with it.
  The agent sees a summary rather than its own history, and what the agent
  keeps in a conversation beyond its text is lost.
- Resume the conversation and ask the agent to branch inside it. The original
  conversation is written to, and two sessions would hold it.

## Consequences

- Every agent that forks does so its own way, so the set that can fork follows
  what the agents offer.
- A fork never takes over the session code of the conversation it copies.
