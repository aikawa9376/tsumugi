# Write commit messages in the prompt

Status: Accepted

## Context

Committing from the Changes list needs a message edited as text. Tsumugi
already has a Neovim-backed prompt that its users edit in, send from and keep
drafts in.

## Decision

A commit's message is written in the prompt, and sending the prompt commits.

## Alternatives considered

- A separate message field in the details panel. It would be a second text
  editor beside the prompt, without the Vim editing the rest of the window
  has.
- Starting `git commit` with an editor in a terminal. The message would be
  lost to the terminal's scrollback when a hook fails, and the window would not
  know whether the commit happened.

## Consequences

- A message a hook refused is put back in the prompt, as a prompt that could
  not be sent is.
- While a commit is being written, the prompt is not a message to the session
  it is shown for.
