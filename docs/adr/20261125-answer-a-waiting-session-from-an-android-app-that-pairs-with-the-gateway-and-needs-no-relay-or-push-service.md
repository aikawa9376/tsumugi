# Answer a waiting session from an Android app that pairs with the gateway and needs no relay or push service

Status: Accepted

## Context

A session left waiting for an answer while the user is away from a window
stalls until they are back. Orca ships an iOS and Android companion that lists
each agent's status and sends a short reply to one waiting on input; it pairs
with the desktop once, reaches it over the LAN or a VPN directly, and is told
of finished agents by push through a service Orca hosts. Tsumugi already pairs
clients with a work machine's gateway, knows both sides by their keys, finds a
machine again by its key, and has decided that a phone shows a session only
from what the work host knows of it and answers by typing into its terminal.
The user carries an Android phone and no other.

## Decision

Tsumugi gets an Android app that pairs with a work machine's gateway as any
client does and reaches it directly, over the LAN or a VPN the user already
runs. It shows which sessions wait on the user and the recent words that say
why, and sends a text reply the user wrote. It needs no relay, no push service
and no account of Tsumugi's own, and opens no port to the internet.

## Alternatives considered

- An iOS app as well, as Orca has. It doubles what is built and kept in step
  with the work protocol, and adds a yearly developer account and store
  review, for a platform the user does not carry.
- A web client the gateway serves. A browser can neither present a raw public
  key nor pin the work machine's, so it would bring a second way to know a
  client, which knowing both sides by their keys alone excludes.
- A relay and push notifications, as Orca offers. They need a service someone
  runs, a push provider account and an outbound dependency of every work
  machine; a user who wants reach beyond their network already has a VPN.
- Notifications alone. Without a push service nothing reaches a phone that is
  not connected, and one that is connected can answer as well as read.

## Consequences

- The phone is a client with the authority of the user the work machine runs
  as, since the gateway relays the protocol as it is. Replying only is the
  app's scope, not a boundary the gateway enforces.
- Away from the LAN and any VPN the phone sees nothing, and nothing wakes it;
  the user opens the app to learn who waits.
- The app is kept in step with the work protocol's versions for as long as it
  exists, and an app too old for the work host is told to update.
- A window and the phone can answer the same wait; the first reaches the agent
  and the second is refused, which needs a wait to be named by more than its
  session.
- A choice question or a permission dialog is shown as waiting to be answered
  at a window until a decision of its own lets the phone answer it.
