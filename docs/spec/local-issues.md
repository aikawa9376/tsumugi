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
