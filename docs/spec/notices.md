# Notices

When you are away, the work host notes each time a session begins waiting on
you, fails, or finishes its turn, so you can go through what needed you
afterwards, in the order that suits you, without opening every conversation.
A state mark says how a session stands now; a notice says what happened.

## What is noticed

A notice records the session and its workspace as they were called, when it
happened and, in a few words, why. A session is noticed when it enters one of
the three states, once however often it is looked at: waiting for something
else instead of what it waited for, a connection coming back or a second
window looking is not another notice. A session that works and then waits
again is noticed again.

The host keeps the newest two hundred, across its own restarts, in its
instance's state. What stood when the host started is not news, so a restart
adds no notice for sessions still waiting, failed or finished.

## Going through them

**Notices…** in the Session menu (`notices.open`) opens a list of the notices
of every environment the window shows. Type to narrow it by session,
workspace, kind or reason. A row whose session still stands as the notice
says is marked as current and comes first; then those not read, each newest
first. A notice whose session has moved on is marked past, and one whose
session no longer runs is marked ended.

Choosing a row marks it read and shows its session. For an ended session it
marks the row read and stays in the list. The list marks the row under the
cursor read or unread, and every row it shows read, from the keys it shows
along its foot.

What you have read is kept by each window, apart from other windows and
under the window's own number, so a window opened again remembers it and
another window does not share it. A window forgets the marks of notices the
host no longer keeps.

## Outside the window

A notification outside the window is told when a failure or a completion
happens while the window is not in front, as for a session that waits
([settings](settings.md)); each of the three is on until Settings turns it off.

**Mute or unmute notifications** (`notifications.mute`), on the row of a
workspace, a worktree or a session, silences the three kinds for what runs in
that workspace or worktree, a session's meaning the worktree it runs in, and
tells them again when used a second time. A muted row is marked, and its rows
and the notices list stand as they would: only the notification outside the
window is not told. A window remembers what it muted across runs, apart from
other windows.
Nothing is told of what the host kept before the window first heard from it,
nor twice of the same notice, and with several windows open the one holding
the lowest number tells.
