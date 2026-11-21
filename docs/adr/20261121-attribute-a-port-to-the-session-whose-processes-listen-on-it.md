# Attribute a port to the session whose processes listen on it

Status: Accepted

## Context

Several worktrees run dev servers on one work machine, often on neighbouring
ports. A list of every listening port, or of those whose process runs inside a
worktree's directory, mixes servers of other worktrees into the one shown, and
a server started from outside the directory is missed.

## Decision

A port belongs to a session when a process in that session's process tree
holds the listening socket. The work machine reads the sockets and the process
tree itself; a client never guesses ownership from the port number or from a
directory.

## Alternatives considered

- Attribute a port to the worktree its process's working directory is under.
  A nested worktree's servers appear under its parent, and a server started
  elsewhere is not listed at all.
- List every listening port of the machine. It shows other users' and other
  worktrees' services as if they were the session's, and offers to kill them.

## Consequences

- A server whose parent exited and was adopted by the init process leaves the
  session's tree and its port is no longer listed.
- Killing a port kills a process the session already lists, so the same
  verification as for any process of the session applies.
