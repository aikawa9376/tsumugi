# Worktree setup

A new worktree is a clean checkout, without the ignored files and the
installed dependencies a project needs to run. Tsumugi can give a worktree the
ignored files of its repository and run the repository's setup script in it,
as the repository asks and as you allow, before an agent starts there. A
repository that asks for nothing is set up as it always was: with nothing.

## What a repository asks for

A repository asks in two files at the root of its primary checkout, the
directory holding its `.git`; commit them so everyone who uses the repository
has the same setup.

`tsumugi.toml` holds the script, the terminals and the shared directories. Its
keys are all optional, and a key it does not know is ignored.

```toml
setup_agent_startup_policy = "wait_for_setup"  # or "start_immediately"

[scripts]
setup = """
pnpm install
"""

[[default_tabs]]        # the first is the agent's own and runs nothing
title = "Agent"
[[default_tabs]]
title = "Git status"
command = "git status --short"
color = "#a1b2c3"       # #RGB or #RRGGBB

[worktree]
shared_directories = [".cache"]
```

- `scripts.setup` runs in the new worktree.
- `setup_agent_startup_policy` says whether the agent starts at once
  (`start_immediately`, the default) or only once the script ended well
  (`wait_for_setup`).
- Each `default_tabs` entry after the first opens a terminal in the new
  worktree, titled as it says. Its `command` runs only when the script's
  commands run, and does not wait for the script. The first entry stands for
  the agent, which Tsumugi starts itself.
- `worktree.shared_directories` lists directories of the primary checkout that
  every new worktree links to: they must exist and be ignored by Git, and
  editing a file in one edits it for every worktree.

A field that does not hold, a string that is empty or too large, a wrong type,
is left out and the others stand. A file that is not TOML, or is larger than
256 KiB, configures nothing, and that is reported. The script, the
startup policy and the terminals are read from the new worktree's own copy of
the file; the shared directories and `.worktreeinclude` from the primary
checkout's.

`.worktreeinclude` lists ignored files and directories to copy, one literal
path a line, relative to the repository root; blank lines and lines starting
with `#` are ignored. A pattern with `*` or `?`, a line starting with `!`, and
a path that is absolute, climbs with `..` or goes through `.git` are left out
with a warning. A copy is the worktree's own: editing it does not touch the
primary checkout's file.

## What is given to a worktree

Only what exists in the primary checkout and is ignored by Git is given, so a
file a repository tracks is never copied over or linked. Nothing is ever
overwritten: where the worktree already has something at a path, it is left
as it is. Nothing is reached outside the primary checkout or the worktree:
a link that leads out of the primary checkout is not followed, a link inside a
copied directory is left out, and a worktree directory that is a link is not
written through. A copy too large, over 2 GiB or 50,000 entries, is left out.
What is left out is reported as a warning, and never stops the worktree or
the rest.

## When the script runs

Whether the setup script, and the commands of the terminals, run is your
policy for the repository, `run_policy` in `[worktree_setup]` of
[settings](settings.md): `run`, `ask` (the default) or `skip`, for every
repository or, in `[[worktree_setup.repositories]]`, for one by the path of its
primary checkout ([reference](reference/settings.md)). A repository cannot
change it, and the files being there runs nothing.

Under `ask`, making a worktree from the tree stops at the question, which
shows what would run; answering yes runs it and no leaves it unrun, and the
worktree goes on either way. Where no person can be asked, as when a session
starts a worktree for another with `tsumugi session start --worktree`, `ask`
runs nothing.

The script runs in a command terminal called Setup in the new worktree, which
shows its output and stays until stopped. It is told where it runs in
`TSUMUGI_ROOT_PATH`, the primary checkout, `TSUMUGI_WORKTREE_PATH` and
`TSUMUGI_WORKSPACE_NAME`, the worktree's directory name. It stops at its first
failing command.

With `wait_for_setup` the agent, or the draft for one, starts only once the
script ended well. A script that fails stops there: the worktree and what the
script left in it stay, nothing is deleted, the row of the worktree says why,
and the output is in the Setup terminal; no agent is started.

## Related

[Making and deleting worktrees](tree.md#making-and-deleting-worktrees),
[saved commands](saved-commands.md) for the commands you run again by hand.
