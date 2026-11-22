# Run a repository's gate on the work host, one run at a time, for the sessions that ask

Status: Accepted

## Context

Sessions working in worktrees of one repository each ran the full check
themselves. Every run built the project again: the processor stayed at its
limit and each worktree grew a build folder of tens of gigabytes. Where the
tools the check is written for were missing, the check was typed out by hand,
once per session. A repository needs its check run in one place, once.

## Decision

A session does not run the full check; it asks the work host to. The work host
runs the repository's gate command one run at a time, on the commit the
session asks for, in a worktree of its own for that repository, in a terminal
that keeps the output. An ask for a commit that is already queued or running
joins that run instead of starting another. The command, and the failures
that are known to come with it on this machine, are the user's own setting for
the repository; the repository does not name them.

## Alternatives considered

- Leave each session to run the check in its own worktree and only cap how many
  run. The builds still repeat per worktree, and nothing lists what is known to
  fail.
- Run the check in the repository's primary checkout. It disturbs the
  checkout the user works in, and it checks that checkout instead of the
  commit the session finished.
- Read the command from a file in the repository. Anyone who can change the
  repository would then run code with the user's permissions on every ask, as
  setup scripts would if the repository decided whether they run.
- Serialise only the builds, with the lock the build tool already takes. The
  tests still run once per session, and a known failure still fails each.

## Consequences

- Only committed work is checked; what a session has not committed is not.
- A run goes on whether or not the session that asked still waits, and asking
  again for the same commit finds it.
- The gate's worktree and what it builds are kept between runs, so the next
  run is incremental.
- A repository has no gate until the user sets a command for it.
