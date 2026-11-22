# List a session whose program exited from its record and restart it into its place

Status: Accepted

## Context

A session whose agent exited vanished from the tree and its tabs closed; the
way back was Resume Conversation, a separate list. A user who typed `/exit`
by mistake, or whose agent crashed, had to find the conversation again.

## Decision

A session whose program exits on its own stays listed. The work host keeps
that fact in the session's record, lists the session from the record while the
terminal host holds nothing for it, and restarts it through the path that
brings back what the terminal host took, so the new session takes its tabs.

## Alternatives considered

- Run every agent under a wrapper that keeps the terminal after the program
  ends, as command terminals are. It changes the process every agent runs
  under, and the terminal host lets go of such a session after a restart of
  the host anyway; the record outlives both.
- Keep exited sessions only in the Resume Conversation list. It leaves the
  tabs and the row to close, which is what made a mistaken exit costly.

## Consequences

- A listed session need not have a terminal in the terminal host; what reads
  the terminal host's sessions cannot assume it has.
- A session ended from Tsumugi is told from one that exited by whether the
  terminal host still listed its program as exited, so it is listed no more.
