# Local issues

Local issues keep work to do in an environment, without a GitHub account.
Open Local Issues from a `+` menu, choose the working directory, and write a
title and a description. Local issues also appear in the workspace's Issues
list alongside GitHub issues, marked as local. Saving an issue leaves it open;
its Start button queues it for work. Queuing authorizes the environment's
default agent profile to start a session and work on that request with its
normal permissions.

Agents can save local issues from a conversation, including research findings
and follow-up work. These issues also wait for the user to start them.

The work host keeps the queue and starts one issue at a time in each working
directory. Different directories can make progress independently. Closing a
window does not discard issues or stop processing. An issue waiting for a
permission or an answer keeps its place until its session can continue.

The issue list shows progress and opens the session that works on an issue.
An agent finishing its turn marks its issue done, and so does ending its
session from Tsumugi once it has worked; this records the agent's completion,
not a human review. A session lost with the terminal host, or whose program
exits on its own, fails its issue. A failed issue retains its request and
error and can be retried explicitly. Queued issues can be cancelled.

A schedule can queue issues on a timetable ([schedules](schedules.md)); an issue it queued is an ordinary issue that says which schedule made it.

Closing an issue preserves its request so it can be reopened later.
Completed or closed issues can be reopened for another round of work; reopening
leaves them saved until Start is pressed. The details offer these actions and
deletion. The list also offers deletion with its X button. Deleting an issue
removes the saved request but leaves its session intact; active work must finish
before its issue can be closed or deleted.

Restarting the work host retains issues and follows sessions already started.
A start interrupted before it can be confirmed is not silently repeated.
An environment begins processing its saved queue when its host serves it
again. Issues belong to that environment on the work machine, so every
window connected to it sees the same work.

## Trying an issue several ways

An issue can be started as several attempts at once, to compare what
different agents, or the same agent twice, make of it. The issue offers to
choose the attempts, each an agent profile and, if wanted, a model; the same
choice can be made more than once.
Each attempt is a session started for the issue in a new worktree of the
issue's directory, all made from the directory's current commit, so every
attempt begins from the same code and none sees another's changes. A
directory outside Git cannot be tried this way, and an attempt whose worktree
cannot be made fails alone.

Attempts start at once, side by side: they neither wait for nor hold the
issue's directory against the queue, though the limits on what a work
machine carries still hold ([resource limits](resource-limits.md)). The issue
lists its attempts, each with its profile, its worktree and how it stands,
and opens each one's session. The issue is worked on while any attempt is,
waits on the user while one waits and none works, and is done once every
attempt has ended its turn; it fails only when every attempt has failed.

### Comparing attempts

From the issue, each attempt's change opens in a diff tab, its worktree
against the commit the attempts were made from, with everything the attempt
has not committed. Any two attempts also open in one diff tab, the second's
branch read against the first's; what either has not committed is not part
of that diff, and the issue says so. Comments and notes on these tabs are those
of each attempt's worktree, as on any diff of it ([review](review.md)).

### Choosing one

Merging an attempt's worktree into the main checkout, as any worktree is
merged ([merging a worktree into main](tree.md#merging-a-worktree-into-main)),
closes the issue and then offers to discard the attempts not chosen. The
question names each one's worktree and what deleting it loses, as deleting a
worktree does; discarding ends their sessions and deletes their worktrees, and
their branches as deleting a worktree does. Declining leaves them as they are,
for cleaning up or the user to clear away later. An attempt can also be
discarded on its own from the issue at any time, its loss told the same way.

Reopening an issue keeps its attempts listed; starting it again starts new
attempts in their place, and the old ones' sessions and worktrees stay until
they are cleared away.

### Attempts a session opens

A session can start an issue as attempts too, as it works an issue
([working an issue from a session](#working-an-issue-from-a-session)): it
names each attempt's profile and model, and opens every attempt as a session
beside it, supervised by it when it asks, so it hears from each when its work
is done and can read and compare them for the user. Choosing one to merge
stays where merging is decided, with the user or within a leader's
discretion ([leader mode](leader-mode.md)).

## From the command line

`tsumugi issue` does from a terminal what the Issues window does, through the
running work host of the environment, starting it where none runs; every
window sees the change as it does one made there.

```
tsumugi issue list [--state <state>] [--dir <path>]
tsumugi issue show <issue>
tsumugi issue create --title <title> [--body <text|->] [--dir <path>]
tsumugi issue start <issue> [--attempt <profile>]...
tsumugi issue cancel|retry|close|reopen|delete <issue>
```

Each takes `--json`, which prints `{"ok":true,"result":…}`, or on failure
`{"ok":false,"error":{"code":…,"message":…}}`, and `--instance <name>` to act
in the environment of another instance, as the other commands do. A command
typed in a session acts in that session's environment.

An issue is named by its number, which `list` shows and which is never given
to another issue, or by the start of its id. A name that fits no issue or
several issues is refused, and the several are listed. Saving leaves the issue
open, as in the window; `start` with `--attempt` starts the issue as those
attempts, one for each time it is given; `create` takes the description from `--body`, or from
standard input for `-`, and the directory from `--dir`, or else where it is
typed. Active work cannot be closed or deleted, and the other changes the
Issues window refuses are refused here. A failed command exits 1 and a command
not understood exits 2.

## From an agent

Each session's agent can list and read local issues, and close, reopen and
cancel them, as well as save them. Reopening and cancelling never start work.
Starting, retrying and deleting are for the user, from the Issues window or
the command line.

### Closing a merged issue

Merging the branch of the worktree a session works in can close the issue the
session was started for, whatever state its session left it in, since the work
is in. Closing it does not end the session, and an issue closed already is left
alone ([merging a worktree into main](tree.md#merging-a-worktree-into-main)).

### Working an issue from a session

A session that opens another can name a local issue for it to work
([sessions that open sessions](opening-sessions.md#working-an-issue)). That is
the supervisor's own act of starting the issue, so it needs no Start from the
user. The issue is linked to the session at once and the link outlives
restarts of the window and the work host; it is shown and followed as an
issue the queue started. The issue's request is the session's first prompt,
followed by what the opener adds.

The one request at a time in a directory limits only what the queue starts:
an issue a session works holds its directory against the queue until its turn
ends, but is itself never made to wait for a request already at work there.

## Tests

- Attempts are made from one commit and start without waiting for the
  directory's queue.
- An attempt whose worktree cannot be made fails while the others start.
- Attempts are refused outside Git.
- The issue's state is drawn from all its attempts: worked while any works,
  done once all ended, failed only when all failed.
- A diff of two attempts reads their branches and says it leaves out what is
  not committed.
- Merging an attempt closes the issue and offers the others for discarding,
  each with its loss.
- Declining leaves the other attempts' sessions and worktrees as they were.
- Starting a reopened issue again lists only the new attempts.
- Every attempt's profile and model are checked before any attempt starts.
- A session's attempts are refused beside a directory, worktree, base,
  agent, profile, model, tier or resume of their own.
- An attempt discarded from its issue is asked about as deleting its row is.
