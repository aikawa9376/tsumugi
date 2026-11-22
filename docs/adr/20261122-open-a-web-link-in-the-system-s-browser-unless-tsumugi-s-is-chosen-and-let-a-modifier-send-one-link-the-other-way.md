# Open a web link in the system's browser unless Tsumugi's is chosen, and let a modifier send one link the other way

Status: Accepted

## Context

Tsumugi has a browser of its own, which keeps a page open per worktree and
reaches the work machine's loopback. A web link in a conversation, a pull
request, an issue or a document can open there or in the browser the user
lives in, where their sign-ins and extensions are. Orca routes links with one
setting and a modifier, and Tsumugi follows it.

## Decision

A web link opens in the system's browser, as it did before Tsumugi had one of
its own, until the user chooses Tsumugi's browser for links. A modifier held
while pressing a link sends that one link the other way: to the system's
browser while links open in Tsumugi's, and, only when the user also chose it,
to Tsumugi's while they open in the system's.

## Alternatives considered

- Open links in Tsumugi's browser by default. A link would stop leaving the
  browser holding the user's sign-ins, which most links need, until the user
  found the setting.
- Choose by what the link points at, local development servers in Tsumugi's
  browser and the rest in the system's. The user could not predict where a
  link goes, and a setting is what Orca gives.
- Let the modifier always invert. A user whose links open in the system's
  browser would open Tsumugi's by an accident of the keys; Orca keeps the
  modifier a one-way escape unless asked.

## Consequences

- Behaviour stays as it was for everyone who changes nothing.
- The choice is the user's, kept on the computer showing Tsumugi, as the
  browser runs there.
