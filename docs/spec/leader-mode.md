# Leader mode

A session can lead: the user hands it a set of issues, and it picks them,
starts and watches the sessions that work them, integrates their results and
cleans up, deciding for itself within the discretion the user set and calling
the user only for what it cannot decide. What the user reads is in the user's
language; everything else is English.

The settings that shape it are in the [reference](reference/settings.md):
`language`, `[leader]`, `[leader.discretion]` and `[worker_tiers]`.

## The skill

A leader works from the `issue-supervisor` skill Tsumugi ships, installed like
the others by `tsumugi skills install` ([skills](skills.md)). It tells the
leader how to find the work, start workers by tier and supervised, wait for
their reports instead of polling, have the central gate run
([the gate](gate.md)), merge finished branches while `main` is clean, remove
finished worktrees and close the issues, and when to ask the user instead.
Everything the user does not read is written in English in it, and it asks
the leader to speak to the user in the language it is given.

## Language

What the leader says to the user (reports, questions and confirmations) is in
`language`, a tag such as `ja`. Left empty, it is the language of the
environment's locale, and English where the locale names none. Prompts to
sessions, messages between sessions, tool guidance, commands, paths and commit
messages are English whatever it says.

## Discretion

Each kind of action a leader may take is either decided by the leader or asked
of the user first. By default a leader decides starting, resuming and, once
their work is done, closing its workers, merging a finished branch into
`main` while the main checkout is clean, deleting a finished worktree and
closing an issue. It asks before merging over uncommitted changes in `main`,
pushing, forcing anything, deleting an unmerged branch, starting an issue that
is only to be researched, and settling a design question Orca does not settle.
Where Orca settles a design question the leader follows it without asking.

The leader is told its discretion in full, and that whatever is not listed is
asked too.

## Integrating a finished branch

A leader merges a finished branch with `tsumugi worktree merge`, or the
`merge_worktree` tool, and does not use `git merge` in the main checkout. It
makes the merge the window makes ([merging a worktree into
main](tree.md#merging-a-worktree-into-main)), by the worktree's directory
(`--dir`) or its branch (`--branch`):

- `--check` only reads how the merge stands, as JSON: the branch and the one it
  merges into, the commits it brings, the files that would conflict, the
  uncommitted files in the main checkout and in the worktree, and the issue it
  would close. Nothing is changed.
- Without a choice, the merge is refused while the main checkout has
  uncommitted changes. `--stash` or `--commit` is the choice the window asks the
  user for; a leader makes it only where its discretion lets it merge over
  uncommitted changes, and asks the user otherwise.
- A merge that would conflict touches nothing and exits with 3 naming the files.
  `--resolve` then brings the main checkout's branch into the worktree's branch
  and starts a session there to resolve the conflicts.
- `--close-issue` closes the local issue the worktree's session was started for,
  and `--remove` deletes the worktree and its merged branch as a
  [cleanup](resource-limits.md#clearing-away-worktrees) does, which keeps a
  worktree with changes or a session in it.
- `--message` sets the merge commit's message.

Merging into the main checkout is the leader's own decision while the main
checkout is clean, and the user's while it is not
([discretion](#discretion)).

## Worker tiers

A leader starts a worker by tier, `light`, `standard` or `heavy`, and the
settings say which profile and model each is. A tier that is not defined, names
a profile that does not exist or a model its profile does not list fails the
start, naming what is wrong; no other model is started instead. A tier replaces
the agent, profile and model of a start, so naming both is refused. The leader's
own session starts from `[leader]`'s profile and model.

## Handing issues over

Let the leader handle it… is an action of the Actions picker. It opens the
Issues sheet on the open issues of the environment and asks to confirm before
anything starts: the issues to hand over, how many workers the leader keeps at
work, what each tier runs, the leader's own profile and model, and the actions
it decides and those it asks about first. The settings window's Leader page
changes all of them. Nothing starts before the confirmation.

A start is refused, saying why, when no issue is chosen, an issue is not open,
sessions opening sessions is switched off, or no worker tier is defined. The
leader opens in a tab like any session and is told its issues, tiers, count,
discretion and language in its first prompt.

## Watching the workers

The Agents panel groups the sessions a leader opened under it, each with its
state ([the agents panel](agents-panel.md)). The leader's details list its
workers with how each stands and what it last said, those waiting on the user
first, so a question is seen before work in hand.
