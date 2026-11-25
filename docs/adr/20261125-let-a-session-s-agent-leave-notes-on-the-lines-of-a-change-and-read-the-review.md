# Let a session's agent leave notes on the lines of a change and read the review

Status: Accepted

## Context

An agent knows why it changed each part of a file. The reviewer can only
find that out from the conversation, away from the code. hunk lets the agent
write notes beside the hunks the reviewer reads, and lets it read the
reviewer's comments. Orca marks which lines an agent wrote but takes no
notes from it.

## Decision

A session's agent can leave notes on lines of a change in its worktree, and
read that worktree's review: the comments, the notes and their state. It does
this through Tsumugi's tools. A note is kept like a review comment, on the
work host, per worktree, and pinned to a side and lines of a file's line diff.
It is marked as the agent's, with the session that wrote it. Notes are drawn
in the diff beside the comments and never move the reviewer's view. Only the
reviewer resolves or deletes a note.

## Alternatives considered

- Notes in a file of the repository. They would be committed with the change,
  or have to be kept out of every commit by hand.
- Notes as comments in the code. They change the code under review.
- Let the agent move the reviewer's view to its note, as hunk can. The agent
  would take the view away from what the person is reading.

## Consequences

- A note goes outdated, as a comment does, once its lines no longer read as
  they did.
- Comments and notes share one store and one freshness check.
- An agent can check its fix against the comments without the person pasting
  them in.
