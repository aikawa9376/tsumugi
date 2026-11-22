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
An agent finishing its turn marks its issue done; this records the agent's
completion, not a human review. A failed issue retains its request and error
and can be retried explicitly. Queued issues can be cancelled.

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

## From the command line

`tsumugi issue` does from a terminal what the Issues window does, through the
running work host of the environment, starting it where none runs; every
window sees the change as it does one made there.

```
tsumugi issue list [--state <state>] [--dir <path>]
tsumugi issue show <issue>
tsumugi issue create --title <title> [--body <text|->] [--dir <path>]
tsumugi issue start|cancel|retry|close|reopen|delete <issue>
```

Each takes `--json`, which prints `{"ok":true,"result":…}`, or on failure
`{"ok":false,"error":{"code":…,"message":…}}`, and `--instance <name>` to act
in the environment of another instance, as the other commands do. A command
typed in a session acts in that session's environment.

An issue is named by its number, which `list` shows and which is never given
to another issue, or by the start of its id. A name that fits no issue or
several issues is refused, and the several are listed. Saving leaves the issue
open, as in the window; `create` takes the description from `--body`, or from
standard input for `-`, and the directory from `--dir`, or else where it is
typed. Active work cannot be closed or deleted, and the other changes the
Issues window refuses are refused here. A failed command exits 1 and a command
not understood exits 2.

## From an agent

Each session's agent can list and read local issues, and close, reopen and
cancel them, as well as save them. Reopening and cancelling never start work.
Starting, retrying and deleting are for the user, from the Issues window or
the command line.

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
