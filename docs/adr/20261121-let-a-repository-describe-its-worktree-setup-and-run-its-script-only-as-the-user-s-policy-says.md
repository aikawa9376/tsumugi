# Let a repository describe its worktree setup and run its script only as the user's policy says

Status: Accepted

## Context

A new worktree is a clean checkout: the ignored files and dependencies a
project needs to run are not in it, so an agent starts in a place it cannot
build. Orca fills the gap with a file the repository checks in, one for the
script and one for the ignored files to carry over, and Tsumugi follows its
model. But a repository's file is written by whoever wrote the repository, and
what it holds is code that would run with the user's permissions.

## Decision

A repository describes its worktree setup in files it checks in, which say what
to give a new worktree and what to run in it. Whether the script runs is the
user's own policy for that repository, kept in the user's settings and never
read from the repository: it runs, it is shown first and runs when the user
says so, or it does not run. A file being there runs nothing.

## Alternatives considered

- Run the script because the repository lists it. Anyone who can change a
  repository could then run code on every machine that makes a worktree of it.
- Keep the policy in the repository's file, beside the script. The repository
  would then decide whether its own code is trusted.
- Offer no script, only the files to carry over. Dependencies are rebuilt per
  worktree in most projects, so the agent would still start where it cannot
  run the project.

## Consequences

- Without the user's say, a repository's file asks for the files it lists and
  nothing more; the first worktree of an unknown repository asks the user what
  to do.
- Where no person can be asked, as when a session starts a worktree for
  another, a script that would be asked about does not run.
- The script runs in a terminal anyone can read, as any command Tsumugi runs
  does, so a failure is read where it happened and nothing runs unseen.
- A script that fails leaves the worktree and what it made as they are; setup
  never deletes the user's work.
