# Keep terminal state and input routing independent of the selected display

Status: Accepted

## Context

An embedded native terminal display should fit inside the workspace without
replacing its running terminals or changing the keys used to move through it.

## Decision

Terminal state and input routing remain independent of the selected display.
Native displays present the terminal host's screen inside the window, while
the existing input route continues to own keys, input methods and mouse modes.

## Alternatives considered

- Give each display its own program and terminal state. Changing displays
  would replace a running terminal and separate it from the host's view.
- Let a native widget or adopted terminal window own input. This gives keys
  and input methods another owner to coordinate with the workspace.

## Consequences

- A display can be changed without ending the terminal or losing its history.
- Display availability belongs to the client computer.
- Native font metrics must reach sizing and pointer hit testing without
  replacing the metrics of the other displays.
