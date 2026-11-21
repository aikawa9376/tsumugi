# Reviewing a diff

You review what an agent wrote by commenting on lines of its diff, then send
every comment together to an agent, instead of copying the path, the lines and
the remark into a prompt by hand.

## Commenting

A diff tab shows a file's line diff with the lines numbered on both sides, and
a cursor you move with the viewer's movement keys. A comment is on one line or
on lines you mark first, and on one side of the change only: the lines before
it (removed lines) or after it (added and unchanged lines). It is kept with
the file, the side, the numbers, the lines' text as they were, and the change
the diff was read from: your uncommitted work, or one commit. The keys are in
[the reference](reference/keys.md); typing the comment ends with `Enter`, which
keeps it, or `Escape`, which gives it up.

Difftastic's structural diff has no line numbers to rely on, so a tab showing
it offers the line diff to comment on instead. Headings, lines of two hunks
and removed lines together with added ones cannot be commented on together.

Comments are kept on the work host, per worktree, in files of Tsumugi's own,
and are never sent to GitHub. They outlive windows and sessions, and every
window of the environment sees the same ones. They are deleted only by you.

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
