# Pin diff review comments to lines of the line diff and send the open ones as one prompt

Status: Accepted

## Context

Reviewing what an agent wrote meant copying the path, the lines of the diff
and the remark into the prompt by hand. Orca lets the reviewer comment on
lines of the diff and sends every comment together to an agent, so the agent
is not pulled back and forth by remarks arriving one at a time.

The details panel and the diff tab read a file's diff as difftastic's text
when it is installed. That text is for the eye: it has no line identity Git
or Tsumugi defines, so nothing can be pinned to it.

## Decision

A review comment is pinned to a side (before or after the change) and the
lines of a file's line diff, in the revision the diff was read from, and
keeps the lines' text as they were commented. Comments are kept on the work
host, per worktree, until a person deletes them. They are sent only when the
person asks: every comment not resolved goes in one prompt, to a session the
person chooses or to a new one. A comment whose lines no longer read as they
were commented is shown as outdated; it is never attached to other lines. Only
a person resolves a comment, and sending resolves none.

## Alternatives considered

- Pin comments to the structural diff. Its text has no line numbers that are
  ours to rely on, and a comment could not be checked against it later.
- Follow a comment to wherever its lines moved. A wrong guess silently points
  the agent at unrelated code; showing it outdated costs the person one look.
- Send each comment as it is written. The agent starts on a half review and
  answers one remark at a time.
- Keep comments in the window. They would not follow the worktree to another
  window or client, and a remote work machine's reviews would live on the
  wrong computer.

## Consequences

- The review view reads the line diff, and a structural diff offers the line
  diff to comment on.
- The work host judges each comment against the file's diff as it is now, so
  every client sees the same freshness.
- A comment sent stays until resolved, so the next batch repeats it, as Orca's
  does, and the agent's fix can be checked against it.
