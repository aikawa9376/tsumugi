# Draw a pull request's review threads in its branch's diff and send one to an agent by taking it into the review

Status: Accepted

## Context

Reviewers on GitHub comment on lines of a pull request. The pull request's
tab lists those threads among its conversation and can ask a new session to
fix them, but the diff of the branch, where the user reads the code and keeps
their own comments and the agents' notes, does not show them, and a request
can go only to a new session, not to the one already working on the branch.
Conductor and Cursor's BugBot bring a pull request's review comments to the
code and hand them to an agent.

## Decision

A diff of a branch against its base draws the review threads of the branch's
open pull request on the lines they are on, beside the user's comments and
the agents' notes. The work host reads them through `gh` when such a diff is
opened, never on a timer. Each thread is checked against the diff as a
comment is: it stands where its lines, as GitHub quoted them, still read so,
and is listed as outdated otherwise, never moved; GitHub calling it outdated
makes it outdated too.

The threads are GitHub's and are not kept by Tsumugi. Nothing of them goes to
an agent until the user takes one into the review: that keeps, as a comment of
the user's on the same lines, what the thread said and where it came from.
From there it is sent as any comment, to a session running in the worktree or
a new session's draft, and resolved by the user alone. Replying to the thread
and resolving it on GitHub stay with the pull request's tab.

## Alternatives considered

- Send a pull request's unresolved threads to an agent along with the user's
  comments. A reviewer's words would reach the agent without the user choosing
  them, and every send would repeat threads the user has already dealt with.
- Keep a copy of the threads with the user's comments. The copy would drift
  from GitHub, where threads are answered and resolved, and would have to be
  read again to be trusted anyway.
- Read the threads on a timer. Each read costs a run of `gh` and the login's
  rate limit, for diffs nobody is looking at.
- Place a thread by the line number GitHub gives, as GitHub does. The number
  is of the pull request's head, which the worktree may have moved past; a
  thread would point at lines it was not written on.

## Consequences

- What the diff shows of a pull request is as fresh as the last time the diff
  was opened.
- A thread taken into the review stays in it once resolved on GitHub, until
  the user resolves or deletes it.
- A thread whose lines the worktree has changed since is listed as outdated
  with what it quoted, as a comment would be.
- Taking a thread in is how its reviewer's words are vouched for; the request
  from the pull request's tab, which quotes them as untrusted data, stays for
  sending a pull request's threads without reading them in the diff.
