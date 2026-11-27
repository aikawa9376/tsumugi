# Mark the lines an agent wrote from the file as it was before and after each write, and carry them by the file's diff

Status: Accepted

## Context

A reviewer reading a change cannot tell the lines an agent wrote from the
lines a person wrote or that were there before. Orca marks every line it saw
an agent write in the diff's gutter, turns a line back to the person's once a
person edits it, and keeps that knowledge to itself rather than in Git.

Tsumugi already loads hooks or a plugin of its own into the sessions it
starts, and an agent runs them just before and just after each of its tools,
naming the file a writing tool writes. What a tool hands them differs from
agent to agent and from release to release; the file itself reads the same
whoever wrote it.

## Decision

The work host reads a file just before and just after a session's agent
writes it with one of its own tools, told by the hooks or the plugin Tsumugi
loads into the agent around that tool. The
lines the write changed or added are that session's; every other line keeps
whoever it was before. The work host keeps, per worktree, each such file's
lines as it last read them, with who wrote each, on the work host and never
in the repository.

Whenever the marks are read, the lines kept are carried to the file as it is
now by a line diff of the two: a line still there keeps its author wherever
it moved, and a line changed or added since is the person's. A diff marks the
lines of its new side so, when that side is the worktree, and names the
session that wrote each.

## Alternatives considered

- Take the lines from what the tool hands its hooks: its patch or the text it
  wrote. Each agent says it in its own words, and a release that changes them
  loses the marks; a whole file written anew would mark lines it left as they
  were.
- Match lines by their text alone. A line the person types that reads as one
  the agent wrote would be marked as the agent's.
- Compare the worktree before and after each turn. Whatever the person
  changed during the turn would be taken as the agent's.
- Keep the marks in Git, as notes or trailers. They would be pushed and
  committed with the change, which Orca also leaves out.

## Consequences

- A line is marked only when the agent's own writing tool wrote it, in a
  session whose agent runs the hooks or the plugin Tsumugi loads into it.
  An agent that runs hooks only once the user has trusted them marks nothing
  until the user does; Tsumugi does not trust them on the user's behalf. A shell command, a formatter or another
  program the agent runs leaves the lines it changes the person's.
- What changed between the two reads of a write is taken as the write's.
- The marks follow the worktree's files as they are, committed or not; a diff
  of commits only shows none.
- A person's edit undone brings the agent's mark back, as the line reads as
  the agent left it.
- The file is read inside the agent's turn, so the work host answers at
  once, and a hook or plugin that cannot reach it loses only that write's
  marks.
