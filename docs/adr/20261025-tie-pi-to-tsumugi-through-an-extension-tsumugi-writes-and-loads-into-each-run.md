# Tie Pi to Tsumugi through an extension Tsumugi writes and loads into each run

Status: Accepted

## Context

Every session Tsumugi starts carries Tsumugi's MCP server and hears the
messages other sessions send it through its agent's own way of hearing from
outside. Claude Code and Codex take a server for one run on their command
line and have such a way. Pi has neither: it reads MCP servers only from its
configuration, and has no channel or queue a program outside it can put a
message on. It runs extensions, though, loaded for one run from a path on its
command line, which can register a server, put a message to the agent as the
user's and see which conversation the process holds.

## Decision

Tsumugi writes an extension of its own into its state folder and loads it
into every Pi session it starts. The extension registers Tsumugi's MCP server
for a session that carries Tsumugi's credential, hears that session's
messages from Tsumugi's server as Claude Code does, and hands each to Pi as a
message from the user, and keeps a record of the conversation the process
holds for Tsumugi to follow it.

## Alternatives considered

- Write Tsumugi's server into the configuration of the profile's own Pi.
  Tsumugi would put its own files into a state folder it otherwise only names,
  and every Pi run in that folder, Tsumugi's or not, would carry the server.
- Type each message into the session's console. It would land in whatever
  the user is typing, and a dialog open would take it as an answer.
- Leave Pi without Tsumugi's tools and messages. A Pi session could neither
  start nor message sessions, nor hear its report, against the decision that
  every session Tsumugi starts has the same means.

## Consequences

- A Pi session hears its messages only while the extension runs: a Pi that
  loads no extensions, or one Tsumugi did not start, hears none.
- Tsumugi's server takes a message as received once the extension has heard
  it, as it does once Claude Code has. The extension keeps one heard while
  no conversation is open for the next, so a Pi that ends before then loses
  it, and it is not sent again.
- The extension runs inside Pi with the user's rights, so what Tsumugi writes
  there is code it ships, written afresh before each session.
- The extension follows Pi's extension interface, which is unversioned; a
  release that changes it leaves a Pi session without Tsumugi until Tsumugi
  follows.
