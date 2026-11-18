# Derive automatic display scale in each window

Status: Accepted

## Context

A fixed zoom leaves text small on an unscaled 4K monitor. Windows can move
between monitors, and a remote work host does not describe the client's display.

## Decision

Each desktop window derives its display scale locally and composes it with the
user's saved zoom adjustment. Respect system scaling before using resolution
as a fallback, and persist the user's preferences rather than the derived size.

## Alternatives considered

- Increase the default zoom everywhere: also enlarges Full HD and already-scaled displays.
- Store the derived zoom as the preference: loses the user's adjustment when displays change.
- Derive scale from window size: makes split or resized windows change text size on the same monitor.

## Consequences

- Different windows can use different display scales at the same time.
- The workspace and Settings use the same policy and retain manual control.
- Monitor changes must be detected even when the compositor sends no move event.
