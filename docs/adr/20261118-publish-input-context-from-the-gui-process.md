# Publish input context from the GUI process

Status: Accepted

## Context

Keyboard integrations see Tsumugi as one foreground application, but its
controls and its embedded editors need different keyboard profiles. The
window already knows where it sends keys, including when a dialog takes them
away from an editor.

## Decision

Publish input context from the GUI process, deriving the destination from the
same policy that delivers its keys. Local integrations can identify that
process through the operating system's foreground window and subscribe
without depending on Tsumugi's selected work environment.

## Alternatives considered

- Infer the destination from running programs or displayed terminal output.
  An editor can remain alive or visible while another control takes its keys.
- Encode the destination in the window title or application identity. This
  makes an integration concern change the window's user-facing identity.
- Reuse the instance's window-command endpoint. A foreground process need
  not reveal which work instance it opened, and subscribers need no commands.

## Consequences

- A local integration can follow destination changes without inspecting
  user input or editor contents.
- The GUI owns publishing and cleanup; work hosts and keyboard firmware do
  not need to reproduce the window's routing policy.
