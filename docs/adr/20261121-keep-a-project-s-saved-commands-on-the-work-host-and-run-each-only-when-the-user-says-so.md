# Keep a project's saved commands on the work host and run each only when the user says so

Status: Accepted

## Context

The user types the same shell commands over and over for a project: the
checks, a development server, a setup script. The project's terminals end
with their programs and a shell's history belongs to one shell, so nothing
near the work holds them. Where a command is kept also decides where it may
run: a command saved for a repository has to run in whichever worktree of it
the user is in, and on the machine that worktree is on.

## Decision

A saved command is a label and the text of a shell command, kept by the work
host for the project that holds it, as the project's resources are. Saving
runs nothing. The user runs one by choosing it, after being shown the text,
the directory and the machine it will run on, and it runs in a command
terminal Tsumugi starts in the worktree selected then; running it again is
another choice. Tsumugi runs a saved command only so, and never one that
comes from files in the repository.

## Alternatives considered

- Keep the commands in a file of the repository, as task runners do. They
  would run on a reading of the repository, which anyone who can change it
  can use to run a command the user never chose, and a command the user
  keeps for themselves has no place in a shared file.
- Keep them in the window. Each window, and each computer reaching the work
  machine, would hold different commands, and a command could not follow its
  project to the machine that runs it.
- Keep them per worktree. The same command would be saved again for each
  worktree of a repository, though only where it runs differs.
- Run them by typing into a shell terminal already open. Whatever that shell
  is doing receives the text, and the output cannot be told from what was
  there.
- Run them under a job runner of Tsumugi's own. Command terminals already
  keep the screen and the exit status until stopped, and a second way to run
  and watch a process would have to agree with them.

## Consequences

- Every window and every session of the project sees the same commands, and
  they outlive both.
- A command runs on the machine of the environment it was chosen in, since
  that environment's host both keeps and starts it.
- Output, exit status, stopping and running again are the command
  terminals' own, with nothing added.
- The text reaches the shell as one argument, so quoting, spaces, Unicode and
  several lines are the shell's to read, not Tsumugi's.
