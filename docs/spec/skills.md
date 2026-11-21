# Skills

Tsumugi ships skills that agents read from their own skills folders, such as
the one that tells an agent how to open a session beside the user's
([opening sessions](opening-sessions.md)). `tsumugi skills` lists them, prints
their guides, and puts them into the skills folders of your profiles. It acts
on files alone and needs no window and no host running.

## What is installed

A skill is installed as a short stub: when to use it, and an instruction to run
`tsumugi skills get <skill>` for the guide. The guide is part of the tsumugi
that runs, so it follows tsumugi when tsumugi is updated, without anything
being written again. `get` prints the guide as Markdown, or with `--json` an
object with the skill's name, description and content. `--full` is accepted
and prints the same, since a guide has nothing bundled beside it.

## Where

A skill goes into the `skills` folder of the state directory of a profile's
agent: the profile's own login when it has one, else where the agent keeps
its state for you. Profiles that share a folder are one place, and are listed
together.

- `install` adds a skill to the profiles whose agent's state directory exists,
  or to those named with `--profile <id>[,<id>…]`, whose folder it makes.
- `update` touches only the places that already hold the skill. It never
  adds one.

`--skill <name>` chooses a skill, as often as needed, and `--all` every one.
With neither, `install` and `update` list the skills and write nothing.

## What is never overwritten

A copy is tsumugi's own while it is a `SKILL.md` alone that tsumugi wrote. Only
such copies are updated. A copy you changed, a folder with more in it, another
skill of the same name, or a link is left alone and reported with its reason,
and that is not a failure.

`--force` replaces such a copy, after moving it whole to a backup folder in
Tsumugi's state directory; the backup's path is reported. A copy is written
only if its folder is still as the plan saw it.

## Dry runs and JSON

`--dry-run` prints what each place would get, and a digest of that plan,
without writing anything. `--json` prints the plan, and for a real run the
result of each place, as an object, for both. The exit code is 1 when a
folder failed or changed while the command ran, 2 for a command or an
argument it does not accept, and 0 otherwise.
