# Keep and open a project's resources from the details panel, in the content

Status: Accepted

## Context

A project's resources were kept only through the tools of its agents, so the
user could not gather material as they came across it, and could not see it
as anything but a list. The user wants the panel to be where project
material is collected, and to read a resource in Tsumugi itself.

## Decision

The details panel keeps, describes and removes a project's resources on the
work host, where agents already find them. Opening a resource shows it in the
content as anything else opened does: a path as the file tree opens it, an
address in the browser. A screenshot is kept as a file on the work machine,
and as a resource pointing at it.

## Alternatives considered

- Leave keeping to agents. The user would have to say in words what the panel
  could take with a click, and an empty panel would show nothing to start
  from.
- Open a resource in the system's application. It leaves the window, cannot
  reach a path of a work machine, and cannot sit in a tab or split beside the
  session.
- Keep the screenshot's bytes in the resource. Resources are references to
  what is kept elsewhere; a file keeps the picture the same way for the user
  and for agents.

## Consequences

- A screenshot's file outlives its resource: removing the resource does not
  remove the picture.
- A path of a work machine opens only where its kind can be shown there, as
  files of the tree do.
