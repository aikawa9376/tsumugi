# The central gate

A repository's full check, `just ci` or whatever it is, is run in one place
and once. A session does not run it; it asks the work host, which runs it one
run at a time and tells the session how its commit stands. Sessions in many
worktrees of one repository then build and test it once between them, not once
each.

## Setting a gate

The command is yours, set per repository in `[[gate.repositories]]` of the
[settings](settings.md) ([reference](reference/settings.md)): the repository's
primary checkout, the command line, and the words of the failures the command
is known to have on this machine. A repository cannot set it, and no file in
one is ever run as a gate. A repository with no command has no gate: an ask is
refused, telling where the command is set.

## Asking

A session asks with the `request_gate` tool, or you with `tsumugi gate` in
the repository (`--dir` names another, `--wait` how many seconds to wait). It
checks the latest commit of the directory it is asked in; what is not
committed is not checked, and the answer says how many uncommitted changes
were left out.

- The work host runs one gate at a time. An ask that comes while another runs
  waits for its turn, in the order asked.
- An ask for a commit that is already queued or running joins that run: it
  gets the same answer and starts nothing.
- The command runs in a worktree of the gate's own, never in a checkout you
  work in, put at the commit asked. What the build left there, ignored by Git,
  stays for the next run.
- It runs in a terminal called Gate that keeps its output and the status it
  ended with until stopped, as any command terminal does
  ([terminals](content.md#terminals)).
- An ask waits up to its `wait_seconds`, 600 by default. A run that is not
  over by then goes on, and the ask is told it is queued or running; asking
  again for the same commit waits for the same run.

## The answer

An answer says whether the commit passes, the status the command ended with,
the failed tests, the failed tests left out as known, the end of the output
and the terminal that holds all of it.

The command passes when it succeeds, or when it fails and every test its
output names as failed has a word of your known failures in it. A failure
that names no test is never a known one. `tsumugi gate` exits with 0 when the
commit passes, 3 when the run is not over, and 1 otherwise.

The gate is run through a POSIX shell: with another shell set, an ask is
refused.

## Related

[Limits on sessions and builds](resource-limits.md), for how the gate's build
is shared with the worktrees; [worktree setup](worktree-setup.md), which keeps
a repository's own files from deciding what runs.
