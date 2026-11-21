# Open pull requests through gh from the prompt

Status: Accepted

## Context

Tsumugi shows the pull request a branch has, but opening one meant leaving it
for a terminal or a browser. Opening one needs a title and a body edited as
text, a base branch, a choice of draft, and a login to GitHub. Tsumugi already
has a prompt its users edit in with Neovim, and it holds no GitHub token of
its own.

## Decision

A pull request is written in the prompt, and sending the prompt pushes the
branch and opens the pull request with `gh` on the work host.

## Alternatives considered

- A form of fields in the details panel. It would be a second text editor
  beside the prompt, without the Vim editing the rest of the window has.
- GitHub's API with a token Tsumugi keeps. Tsumugi would store a second
  credential for what `gh` already holds, and a work machine would need it
  too.
- Running `gh pr create` in a terminal. The window would not know whether the
  pull request was opened, and could not show it afterwards.

## Consequences

- A refused push, a missing or logged-out `gh`, or a branch that has a pull
  request open already fails with its own words, and the text written goes
  back to the prompt.
- Remote machines behave as local ones, since the work host runs the
  commands.
