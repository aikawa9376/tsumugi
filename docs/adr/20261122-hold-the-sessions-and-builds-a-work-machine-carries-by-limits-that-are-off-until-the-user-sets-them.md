# Hold the sessions and builds a work machine carries by limits that are off until the user sets them

Status: Accepted

## Context

Sessions open sessions, and each one builds its project in a worktree of its
own. On one machine that kept the processor at its limit and filled the disk
with build folders. No number of sessions or jobs is right for every machine,
and an earlier cap on sessions that opened sessions was dropped because no
number of it had a reason behind it.

## Decision

The user's settings hold the limits: how many agent sessions run at once, how
many jobs the builds of sessions Tsumugi starts run, how those builds share
what they build, and how large a repository's worktrees may grow before a
warning. Every limit is off until the user sets it, and building in each
worktree's own folder is what Tsumugi does until the user chooses to share. A
session that another opens waits for room under the cap and is refused, told
how many run, once it has waited too long; a session the user opens is never
held.

## Alternatives considered

- Limits on by default. A number that suits one machine holds back another,
  and the earlier cap showed that a default number is soon raised by hand.
- Share one build folder per repository by default. It changes how every
  repository builds without being asked, and builds of different worktrees
  then wait on one another's lock.
- Hold the sessions the user opens too. The user decides what to open; holding
  their own start would leave them with a window that does nothing.
- Only warn, never hold. A warning arrives after the machine is already loaded.

## Consequences

- Nothing about starting or building changes for a user who sets nothing.
- Sharing builds trades disk for waiting: the worktrees of a repository build
  one at a time into one folder, or, with a compiler cache, each in its own
  and without waiting.
- The size of worktrees is only reported, never acted on.
