# Let a leader session run issues and decide within the discretion the user sets

Status: Accepted

## Context

Working through a set of issues takes many small judgements: which to start
first, when a worker's branch may be merged, whether a clash is mechanical.
Asking the user at each step makes them the scheduler, and leaving the
agent unbounded lets it push, force and delete without being asked.

## Decision

A session leads the issues the user hands it: it chooses, starts, watches
and integrates the work, and clears away what is finished, itself. The user
sets, kind of action by kind of action, which it decides and which it asks
about first, and it asks before anything outside that, or not listed. What
the user reads is in the user's language; what they do not read is English.

## Alternatives considered

- Have Tsumugi's own code schedule and merge the issues. Ordering by
  dependency and settling clashes need judgement, which is what an agent
  supplies; code would need a rule for every case or stop and ask.
- Ask the user before every step. The user does the leader's work.
- Give the leader no bounds. What cannot be undone, such as a push, is then
  decided by an agent without the owner.

## Consequences

- The bounds are told to the leader, not enforced by the work host on its
  tools, so they hold as far as the agent follows its instructions.
- The leader is a session like any other: the user can read it, steer it and
  answer its questions in its console.
