# Put a finished session to sleep by stopping its program and wake it through the restart path

Status: Accepted

## Context

Every session holds an agent process and a model conversation in memory for as
long as it is listed, however long it has been done. Someone who keeps many
sessions open pays for all of them, and ending them to save the machine loses
their rows and tabs.

## Decision

A session that is done and has been left alone for a while is put to sleep: its
program is stopped, and its record, row and tabs stay. Waking it, by focus or
by a message sent to it, brings the conversation back through the same path
that restarts a session whose program exited, so what stands for it takes its
tabs, code and opener.

## Alternatives considered

- Leave the program running and ask the agent to trim itself. Agents have no
  such way, and the memory is the point.
- Close the session and list its conversation only in Resume Conversation. It
  loses the row and tabs, and a supervisor would take it for a failed worker.
- Wake it on focus only. A message to a sleeping session would then be refused
  or lost, so sleeping would change who can be talked to.

## Consequences

- A sleeping session is neither running nor ended: what counts running
  sessions, tells a worker's failure or refuses a message must tell it from both.
- A message waits for the woken session, not for the record that slept.
