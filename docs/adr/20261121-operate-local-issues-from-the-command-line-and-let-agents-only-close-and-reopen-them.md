# Operate local issues from the command line, and let agents only close, reopen and cancel them

Status: Accepted

## Context

Local issues could be changed only from the Issues window, and agents could
only create them. A supervising session could not close the issues it had seen
finished, and a person working in a terminal could not queue one without
leaving it. Saving an issue deliberately waits for the user, because starting
one authorizes an agent to work with its normal permissions.

## Decision

`tsumugi issue <verb>` operates local issues, as `tsumugi session` and
`tsumugi terminal` operate sessions and terminals: a noun, then a verb, with
`--json` for scripts and `--instance` to act in another instance, so the
environment is chosen as for the commands beside it. Each verb is an ask to the
running work host that owns the queue; nothing writes the queue behind it, so
every window sees the change as it does one made in Issues.

An issue is named by the short number it keeps for good, or by the start of its
id when no issue has that number.

Agents get tools to read issues and to close, reopen and cancel them. Starting,
retrying and deleting stay with the user, so that what an agent saves still
waits for a person, and what the user wrote cannot be lost by an agent. The
same actions are open to the command line, which a session's agent can run
only through its own shell permissions.

## Alternatives considered

- Offer every verb as a tool. A tool is usually approved once for a session,
  so one approval would let an agent start work the user never queued, or
  delete a request.
- Flags on the bare command, such as `tsumugi --issue`. Every other command
  of Tsumugi is a subcommand, and flags cannot hold a verb and a name together
  without growing a grammar of their own.
- Write the queue's database from the command. A second writer bypasses the
  host's rules for what may change in each state and leaves the windows
  unaware until the next listing.

## Consequences

- The command starts the work host where none runs, as the other commands do.
- An agent that needs an issue started or deleted asks the user, or runs the
  command and answers to the permission prompt of its own agent.
- An issue's number is kept beside its id and is never given to another issue.
