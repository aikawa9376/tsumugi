# Answer a waiting session from a native phone app that pairs with the gateway and needs no relay or push service

Status: Proposed

This record is a proposal for the user to approve or reject, and nothing in it
is built. Approving it changes the status to Accepted and moves the research
section into the specification; rejecting it deletes the file.

## Context

Orca ships an iOS and Android companion app that lists every worktree with its
agent's status, shows the recent terminal scrollback or a chat transcript,
and sends a short reply when an agent waits on input. It pairs once with the
desktop, reaches it directly over the LAN or a VPN such as Tailscale, can be
pointed at a new address without pairing again, revokes from the desktop, and
is told when an agent finishes by push notifications through a service Orca
hosts. Tsumugi is being moved toward Orca, and a session left waiting for an
answer while the user is away stalls until they are back at a window.

What Tsumugi already holds: a gateway that relays a paired client to the work
host's and the terminal host's sockets as they are, reached over a connection
that pins each side's key; pairing by a code the client shows and the operator
types at the work machine; revocation that ends open connections; discovery
of the work machine again by its key when its address changes; changes
admitted once by an identifier, whose outcome a client can ask after a lost
answer; and a decision that a phone shows a session only from what the work
host knows of it, never drawing its terminal, and answers by typing into the
terminal.

## Decision

Tsumugi gets a native iOS and Android app that pairs with a work machine's
gateway as any client does, reaches it directly over the LAN or a VPN the user
already runs, and lets the user see which sessions wait on them, read the
recent words that explain why, and send a text reply they wrote. It needs no
relay, no push service and no account of Tsumugi's own, and opens no port to
the internet. Choice questions and permission dialogs are answered from it
only after a decision of their own.

## Alternatives considered

- A web client the gateway serves. It would need no store and one codebase,
  but a browser can neither present a raw public key nor pin the work
  machine's, so it would bring a second way to know a client, which the
  decision to know both sides by their keys alone excludes.
- A relay and push notifications, as Orca offers. They need a service someone
  runs, accounts with Apple and Google for push delivery, and an outbound
  dependency of every work machine; the first deliverable is not to assume
  any of them, and a user who wants reach beyond their network already has a
  VPN.
- Notifications alone. Without a push service nothing reaches a phone that
  is not connected, and one that is connected can answer as well as read. A
  notice the user cannot act on only tells them to go to a window, which the
  desktop's notifications already do.
- Orca's terminal scrollback and live input on the phone. The accepted
  decision that a phone never draws a session's terminal stands: it would
  reflow the session under the desktop's console.
- Do nothing. The desktop's notices and notifications cover the maintainer's
  daily use at a window; this record only matters once the user wants to
  answer while away, which is why it is proposed at P3 and not started.

## Consequences

- A phone is a client with the authority of the user the work machine runs as,
  because the gateway does not reinterpret the protocol, as with Orca's
  paired phone. Replying-only is the app's scope, not a boundary it enforces;
  a phone limited by the gateway would need a decision against the gateway's.
- Away from the LAN and any VPN, the phone sees nothing. Nothing wakes a
  phone whose app is not open; the user opens it to learn who waits.
- Two native platforms are to be built, shipped and kept in step with the work
  protocol's versions for as long as the app exists. That cost, not the
  protocol work, is the main one.
- A window and a phone can answer the same wait; the first answer reaches the
  agent and the second is refused, which needs a wait to be named by more
  than the session.

## Research behind the proposal

Removed from this record once it is accepted, its rules moving to the
specification.

### What Orca does, from its source

- The app is React Native with Expo. It dials the desktop's WebSocket server,
  by default on port 6768, over the LAN or Tailscale directly, and over Orca's
  relay if the user signs in. A saved host's name and address can be edited
  without pairing again.
- Its RPC carries a protocol version, and a phone or desktop too old for the
  other is blocked with a word to update, as the work protocol's hello does.
- A terminal send is built with `failWhenDisconnected`, with the comment that
  keystrokes parked during a connect wait would replay stale input after
  the reconnect. A reply whose answer was lost is marked delivery-unknown and
  never retried; the app says "Response unconfirmed — check chat before
  retrying".
- Permission and question answers are structured requests that name the
  pending item, each with an operation identifier, not keystrokes.
- Revocation is host-side: the phone watches for a definite rejection and
  tells it from an unreachable desktop. Removing a host on the phone does not
  revoke it.
- Push is delivered through a gateway Orca hosts; while connected, the app
  subscribes to the desktop's notifications and asks for what it missed.

### What Tsumugi's protocol offers a phone

| Need | Reusable as it is | Gap |
| --- | --- | --- |
| Speak to the work host | hello and version agreement; `Watch`, snapshot and numbered patches; a client that loses a gap asks again | none |
| List sessions and how each stands | the session `State`, with `Waiting(Permission \| Input \| Other)` | the lanes and a card's last words are derived by the window's core and not by the work host; the phone shares that core or the host sends them |
| Recent context | the transcript the board quotes | not traced to where it is read; if it is read on the work machine for a window, a phone needs an ask for it |
| Send a text reply | `Ask::SubmitPrompt` through `Ask::Apply` | none for text; the agent must be one whose prompt it types into |
| Never send twice | `MutationStamp` bound to the instance and both hosts' incarnations; `Ask::MutationStatus` to learn an outcome | the phone keeps its draft until the outcome is known, as the remote window does |
| Be told it waits | `Notices` of the work host: the newest two hundred, across its restarts | the phone's read marks are its own, as a window's are; a read of what it missed on connect |
| Revoke | `tsumugi remote revoke` and Settings list and end the connection | the phone tells a revoked key from an unreachable machine |

`send` between sessions is not reused: it tells a peer something in another
agent's voice and costs it a turn, while a reply is the user's own words and
has to reach the agent as the user's prompt.

A choice question or a permission dialog needs new meaning. Today the work
host learns only that a session waits and whether for a permission or for
input; I found no carrier in the protocol for a question's text or its
choices, and the plugin Tsumugi loads holds skills only, so the accepted
decision to learn them from hooks has to be checked against the code before
this record is relied on. An answer then needs an ask that names the session,
the identity of that wait and the choice, is refused when the wait has since
changed, and is typed or passed on as the earlier decision says. Without
it, a phone shows such a session as waiting to be answered at a window,
which the earlier decision already permits.

### Connectivity, reconnection and revocation

The gateway listens on every interface at port 47130 and announces itself by
mDNS on the LAN. On a LAN the phone finds a paired machine by its key; over a
VPN there is no announcement, so the address is typed and kept, and changing
it never pairs again because the machine is known by its key. iOS asks for
local-network permission, and an app suspended in the background loses its
socket, so the phone reconnects on return, is sent the whole work again,
and shows how old what it displays is. Keys typed before a connection is lost
never reach a program after it returns.

### Estimated cost

These are estimates from reading, not from a prototype.

- Shared: the pinned-key TLS, the pairing exchange and the protocol types are
  Rust crates; the app reaches them through bindings, or reimplements them,
  which is the larger risk. The window's board derivation is the same
  question.
- iOS: an Apple developer account, a yearly fee, with review for the App
  Store or TestFlight; local-network permission text; no background socket.
- Android: Google Play's one-time fee, or an APK as Orca also offers; a
  foreground service would be needed to hold a socket, and is not proposed.
- Both: each protocol version Tsumugi raises is a release of the app, and an
  old app is told to update.

### Open for the user

- UI toolkit: Orca's Expo and React Native, or Rust-native, which fits
  Tsumugi's code and shares crates. The standing direction picks Orca's, so
  the bindings become the work.
- Whether the first release includes choice and permission answers; this
  proposal says no.
- Whether reply-only should be enforced by the gateway; this proposal says
  no.
