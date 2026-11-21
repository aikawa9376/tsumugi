# Let every session open sessions beside it that stay open to the user and report to it when asked

Status: Accepted

## Context

A session could start sessions to do parts of its work, each told only its
first prompt, each doing one piece and reporting back before it ended. Every
handoff and every report lost detail, and the starting session passed a
summary of the reports on to the user. Follow-up work went to a new session
that rebuilt what the ended one had known. The plan stayed with the starter
while the work was done elsewhere, so a change of plan arrived only with the
next report. Within one conversation, an agent runs what can go alongside
through its own subagents and tools, and gets their results back in the same
context.

The same loss comes back the other way. A line of work that the user has to
steer — a design to choose, a draft to review, an interface to approve —
given to a subagent reaches the user only through the session that started
it, and the user cannot step into it. Such lines often turn up inside a
session's own work, and their briefs carry what they need all the same.

Sessions that only coordinate through the user, though, leave a session that
splits a large change into lines of work with no way to learn that a line is
done, failed or stuck except by asking the user or watching consoles. It
cannot end its turn and be woken by the work.

## Decision

There is one kind of session, and every session Tsumugi starts can open
sessions beside it and message any other. A session it opens is like one the
user started: the user can see it and step into its console at any time, it
ends only when the user or the agent ends it, and it can open sessions too.

By default it reports to no one. A session may open one as a worker it
supervises: the worker tells its opener how its work ended — succeeded or
failed — and Tsumugi tells the opener when a worker stops without saying so,
by ending its turn, by waiting on the user, or by ending, and the opener can
wait for these reports instead of ending its turn to be woken. Supervision
adds a way to learn how the work stands. It gives the opener no authority the
user does not also have: the user keeps steering the worker, and what the
worker reports is weighed like any other input.

Who will steer the work decides where it runs. A session opens one for a line
of work the user will want to steer themselves: one that needs their decisions
or review along the way, lands as a change of its own, or lives in another
repository, wherever it turned up, so long as what it needs fits in a first
prompt. What comes back to the session to fold into its own answer runs in its
agent's subagents and tools, and what follows from what it already knows stays
with it; a session never opens one to carry out a step of its own work, or
only to have something run alongside.

Sessions talk as peers: a message tells what the recipient should know or
asks what it knows, and never assigns it work; the recipient weighs it as any
other input, and the sender plans nothing on a reply coming. Tsumugi carries
the messages, addressed by session identity, and hands each to the receiving
agent through that agent's own way of hearing from outside; handing it over
is receipt. A report from a worker is such a message, sent once. Which session
opened which, which are supervised, and the messages, are all the work host
keeps between sessions.

## Alternatives considered

- Let a session start workers that report back and end. The work reaches the
  user through a summary of summaries, and follow-up work starts from nothing.
- Keep a worker after its report for follow-up work from its starter. The
  starter still stands between the user and the work, and keeps the plan away
  from the session carrying it out.
- Let every opened session report to its opener. Lines the user steers
  themselves would cost the opener a turn for every stop, and the user's own
  conversation with them would be narrated to a session that has no use for it.
- Give a worker no console of its own, as a subagent has. The user could not
  read or steer it, and the work would pass through its supervisor again.
- Let sessions poll each other's state. Each check costs a turn, and a
  supervisor that polls cannot end its turn.
- Open a session only for work that needs little of what the opener knows.
  Lines the user has to steer mostly come out of the opener's own work, so
  they go to subagents, and the user's decisions pass through the opener.
- Open a session for whatever can run alongside. Steps of one answer scatter
  across consoles, and the user is left to gather them.
- Leave every new session to the user. A separate line of work found inside a
  session would wait for the user to start it and write what it needs.
- Let sessions message only those they opened or were opened by. Unrelated
  sessions sharing a checkout could only coordinate through the user.
- Ask for an acknowledgement of every message. Receipt becomes work, and costs
  a turn.
- Let a message assign work and wait for its result. Workers come back through
  the messages, with the same summary between the user and the work.
- Keep sessions from talking at all. A session would rebuild from a diff what
  the session that made it could say.

## Consequences

- Work whose result returns to a session within its own turn runs in its own
  agent's subagents and tools; Tsumugi takes no part in it.
- Work asked for in parallel splits two ways: lines the user will steer go to
  sessions, supervised when the opener wants to know how they end, and the
  rest to subagents.
- A session opening another writes it a first prompt that carries all it
  needs, since the new session starts from nothing else.
- A supervisor can end its turn and be woken by a worker; nothing blocks it
  from waiting, and nothing has to bound how deep sessions open sessions.
- A stop is reported once, however long the worker stays stopped, and a
  report is never made again for one that may not have arrived; the opener
  asks the worker.
- A worker that dies still reaches its opener, as a failure.
- Every session Tsumugi starts carries Tsumugi's bridge; sessions it did not
  start can neither send nor receive.
- A message costs its recipient a turn, so sessions are told to send only what
  the recipient has to act on or answer.
- Who opened whom, who supervises whom and who told whom what outlive the
  sessions and the window, and the user can read every exchange.
