# Ask an agent to fix a pull request in a draft the user sends

Status: Accepted

## Context

A pull request's tab shows its failed checks and review threads, but asking an
agent to fix them meant copying the check names, links, logs and comments into
a prompt by hand. Orca builds such a request from the failed checks and the
selected review threads and hands it to an agent. Tsumugi holds no GitHub token
of its own, and a request is only as good as what it was made from: a head that
has moved since the tab was read, or a log that cannot be read, would
otherwise be sent as if it were current.

## Decision

A request to fix a pull request's failed checks or unresolved review threads
is prepared as the text of a new session's draft, built from what the work
host reads afresh through `gh`, and is sent only when the user sends the
prompt. Preparing and sending it write nothing on GitHub.

## Alternatives considered

- Start the agent at once, as Orca does. The user could not see or correct
  what is sent, and a stale head or a missing log would go unnoticed.
- Build the request from what the tab last read. The head may have moved and
  the logs were never read, so the request could name failures that are gone.
- Have the agent read the checks and logs itself from the pull request's link.
  Its login may not be `gh`'s, it may not be able to read them, and what it
  read would not be shown to the user before it acts.

## Consequences

- A log or a head that cannot be read is told in the request, which still
  names every failed check and its link.
- Remote machines behave as local ones, since the work host reads and the
  work host's `gh` is the login.
