# Write on a pull request or an issue from the prompt under its tab

Status: Accepted

## Context

Tsumugi's tabs for a pull request or an issue showed its conversation and
could post nothing, so answering a review meant leaving for a browser. GitHub
holds the text a user writes there, and Tsumugi has a prompt its users edit in
with Neovim and no GitHub token of its own.

## Decision

A pull request's or an issue's tab shows the prompt, and what is written on
GitHub is written there: sending the prompt is the one act that posts. A key
that starts a reply, a reaction or a review only prepares the prompt, and an
act that needs no text, resolving a thread or marking a file viewed, is its own
key or click. Every write is made by `gh` on the work host.

## Alternatives considered

- A composer drawn inside the tab's page, as GitHub's own page and Orca's draw
  it. It would be a second text editor beside the prompt, without the Vim
  editing the rest of the window has.
- Sending as soon as the text is complete, or on leaving the prompt. A comment
  cannot be taken back by Tsumugi, which never edits or deletes what is
  written on GitHub, so posting must be a deliberate act.
- GitHub's API with a token Tsumugi keeps. It would store a second credential
  for what `gh` already holds, and a work machine would need it too.

## Consequences

- A write that fails, offline, refused or for a login that may not write
  there, puts the text back in the prompt, so nothing written is lost.
- The tab of a pull request or an issue has a prompt like a session's, and the
  keys of its cards are bound apart from the other viewers'.
- What is written is only added: Tsumugi offers no editing or deleting of
  anyone's comment.
