# Reviewing a diff

You review what an agent wrote by commenting on lines of its diff, then send
every comment together to an agent, instead of copying the path, the lines and
the remark into a prompt by hand.

## Commenting

A diff tab shows a change's line diff with the lines numbered on both sides,
and a cursor you move with the viewer's movement keys
([diff tabs](details.md#diff-tabs)). A comment is on one line or
on lines you mark first, and on one side of the change only: the lines before
it (removed lines) or after it (added and unchanged lines). It is kept with
the file, the side, the numbers, the lines' text as they were, and the change
the diff was read from: the worktree, one commit, or two revisions. The keys are in
[the reference](reference/keys.md); typing the comment ends with `Enter`, which
keeps it, or `Escape`, which gives it up.

Difftastic's structural diff has no line numbers to rely on, so a file shown
in it is commented on in the line diff instead. Headings, lines of two hunks
and removed lines together with added ones cannot be commented on together.

Comments are kept on the work host, per worktree, in files of Tsumugi's own,
and are never sent to GitHub. They outlive windows and sessions, and every
window of the environment sees the same ones. They are deleted only by you.

## Notes from agents

A session's agent can leave notes on lines of a change in its worktree, as you
leave comments, through its tools ([what a session can do](opening-sessions.md#what-a-session-can-do)).
A note is drawn beside the lines it is on, marked as the agent's and named by
the session that wrote it, and can be hidden with every other note. Notes never
move your view; you step from one to the next and back
([keys](reference/keys.md)). A note is checked against the diff as a comment
is, and only you resolve or delete it. Notes are not sent back to an agent.

The agent can also read the worktree's review: every comment and note, with
its file, side, lines and whether it is outdated, resolved or sent. It cannot
change or resolve a comment of yours.

## Viewed files

A file of the stream can be marked viewed, which folds it and checks it off in
the list; the list says how many of the change's files are viewed. The mark
is kept with what the file's diff was when you marked it, on the work host per
worktree, and goes away by itself once that diff changes. Comments and notes
are kept whether a file is viewed or not.

## Staying true to the code

Every time the diff is read again, each comment is checked against it: the
lines it was made on must still read as they did, on the same side at the same
numbers. A comment that does not is marked outdated and listed after the diff
with the lines it was made on. It is never moved to other lines. Only you
resolve a comment, to fold it away, and you can open it again.

## Sending

Sending shows the prompt first: one text naming, for every comment not
resolved in any file of the worktree, its file, side and lines, quoting them,
with what you said; outdated ones say so. You choose a session running in the
worktree, which takes it as a message ([prompt](prompt.md)), or prepare a new
session's draft in the worktree with the text written in it, to start as any
draft.

Nothing is sent until you ask, and no comment is lost to sending: sending
resolves none, a comment stays after it was sent, marked, so the next prompt
repeats it and you can check the agent's fix against it. A prompt that was
cancelled, refused (the session waits on you) or could not be confirmed leaves
every comment as it was, and a comment edited while the prompt was on its way
is not marked as sent.

## Tests

- The numbers of each side are read from the hunk headings.
- A removed line is commented on the old side and an added one on the new.
- A comment is outdated when its lines changed, moved or went.
- A comment on a commit reads that commit's diff whatever the worktree holds.
- Paths and lines with Unicode are quoted whole.
- The prompt leaves out resolved comments and marks outdated ones.
- What was edited while sending is not marked sent.
- A note is kept and checked as a comment is, and is told apart from one.
- An agent reads every comment and note of its own worktree only.
- A viewed mark goes once the file's diff changes, and stays while it does not.
