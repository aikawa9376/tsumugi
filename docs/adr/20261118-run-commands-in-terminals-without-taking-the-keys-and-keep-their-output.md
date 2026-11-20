# Run commands in terminals without taking the keys and keep their output

Status: Accepted

## Context

A session's commands need a visible terminal when they ask the user for a
password or a login. Automatically giving that terminal the keys interrupts
the user's typing. Ending the terminal with the command also loses the output
before the session can read it.

## Decision

A session runs a command that asks the user in a Tsumugi terminal it asks to
be shown. Automatic terminal display preserves keyboard focus and leaves an
active input area undisturbed. The command terminal outlives its program,
keeping its screen and exit status until it is stopped. Sessions are told to
run commands there and direct the user to the terminal when it needs input.

## Alternatives considered

- Give the new terminal the keys automatically. A command can appear while
  the user is still writing, sending their input to another program.
- Hand the command to the user. They have to copy it to a terminal of their
  own, and the session cannot see what came of it.
- End the terminal with its command. The session can miss both its output
  and its exit status.

## Consequences

- The user selects a terminal before answering what its command asks.
- A command's output stays readable after its windows close.
- A terminal appearing while the user writes waits behind the shown tab.
