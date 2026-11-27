# The Android app

The Android app shows which sessions on a work machine wait on you, with the
recent words that say why, and sends a short reply to one whose turn ended.
It is a paired client of the machine's gateway like any other
([paired work machines](remote-execution.md#paired-work-machines)), reached
directly over the local network or a VPN you already run. It needs no relay,
no push service and no account, and it opens no port.

## Pairing

The app asks for the work machine's address: a host name or an IP address,
with the gateway's port 47130 when none is given. It then pairs as
`tsumugi connect pair` does: once the machine's operator can see the request,
the app shows a code to type there, keeps asking while the machine does not
accept requests, and shows a new code after a wrong one. The app shows its own
key, as `tsumugi remote devices` lists it.

A paired machine is known by its key. Its name and its address can be changed
in the app without pairing again; another machine at its address is refused.
Forgetting a machine in the app does not revoke the phone there: `tsumugi remote
revoke` or Settings on the work machine does.

## What waits

The app lists the sessions of the machine's own environment that wait on you,
the one that began to wait last first, each with its name, its workspace, its
agent, and the last words of you and of its agent in its conversation, where
the work host reads them.

A session waits on you while its turn has ended, or while it asks a question
or for a permission. One that rests, hibernated or exited, does not. The work
host numbers each wait as it begins: a session that works and waits again, or
comes to wait on something else, waits anew.

A question or a permission dialog is shown as waiting to be answered at a
window; the app does not answer it.

## Replying

A session whose turn ended takes a reply written in the app, typed into its
prompt as a window's prompt would type it, without attachments. A reply names
the wait it answers, and the work host types it only while the session still
stands in that wait: a reply to a session another window answered meanwhile,
or that moved on, is refused and nothing is typed.

A reply is sent once. The app never sends it again by itself, and a draft is
kept until its reply is known to have been typed. The app tells apart a reply
that was not sent, as when the machine could not be reached, one the machine
refused, and one whose outcome was lost after it was sent. The last may have
been typed, so the app sends nothing more to that wait: you look at the
session before replying again, and a later wait of the same session takes the
draft again.

## Connection

The app connects while it is in front and lets the connection go when it is
not; it keeps nothing running in the background and tells you of nothing
while closed. Each time it comes to the front, or after losing the connection,
it takes the whole work again. While it is not connected it shows the waits it
was last told and how long ago that was, and tries again.

The app tells a machine that refused it apart from one that cannot be reached:
a phone the machine does not know, as after it was revoked there, is told to
pair again, and a machine whose key is not the paired one is refused, neither
of which it tries again. An app and a work host that share no version of the
[work protocol](work-protocol.md) say which of the two is to be updated.

## Building it

The app lives in `android/` and is built with Gradle, which builds the Rust
core from `crates/mobile` with the Android NDK for each ABI it carries. It is
distributed as an APK.
