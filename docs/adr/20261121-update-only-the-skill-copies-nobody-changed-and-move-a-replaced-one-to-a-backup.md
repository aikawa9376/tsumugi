# Update only the skill copies nobody changed and move a replaced one to a backup

Status: Accepted

## Context

A skill folder is in the user's own agent folders, where they are free to edit
it, and where another tool may have put a skill of the same name. An update
that overwrites by name destroys that work without being asked.

## Decision

Tsumugi rewrites a skill's folder only when it is a copy tsumugi wrote and
nobody changed. Any other folder is left alone and reported. The user can ask
for it to be replaced, and tsumugi then moves it to a backup before it writes
its own.

## Alternatives considered

- Overwrite whatever stands under the skill's name. It loses the user's edits
  and another tool's skill.
- Never touch a skill once installed. Stubs that need to change would stay
  wrong until the user removes them by hand.
- Merge the user's edits into the new text. A merge of free-form prose has no
  right answer, and a failed one damages the user's copy.

## Consequences

- An update can run unattended: it changes nothing the user could have
  written.
- A copy tsumugi did not write stays as it is until the user decides.
- Telling tsumugi's copies from the user's means keeping a record of every
  stub it has shipped.
