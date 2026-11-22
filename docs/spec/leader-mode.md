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

## Worker tiers

A leader starts a worker by tier, `light`, `standard` or `heavy`, and the
settings say which profile and model each is. A tier that is not defined, names
a profile that does not exist or a model its profile does not list fails the
start, naming what is wrong; no other model is started instead. A tier replaces
the agent, profile and model of a start, so naming both is refused. The leader's
own session starts from `[leader]`'s profile and model.
