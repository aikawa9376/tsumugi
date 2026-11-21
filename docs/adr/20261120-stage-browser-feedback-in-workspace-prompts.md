# Stage browser feedback in workspace prompts

Status: Accepted

## Context

A page can identify its rendered elements, but its content is untrusted.
Element feedback needs both that context and the user's intent, addressed
to the right environment's agent.

## Decision

Let the page supply element context during an explicit selection. Keep the
user's instructions and destination choice in the workspace, and stage
the resulting feedback in the chosen session's prompt for review.

## Alternatives considered

- Let pages address agents directly. Visiting a page would give its scripts
  authority to choose work and submit instructions.
- Send selected elements immediately. The user could not finish instructions
  for several elements or review what the agent will receive.

## Consequences

- Page scripts cannot submit agent work through the browser bridge.
- Feedback is reference material accompanying the user's instructions.
- Images cross the environment's normal attachment boundary, including when
  the destination work host is remote.
