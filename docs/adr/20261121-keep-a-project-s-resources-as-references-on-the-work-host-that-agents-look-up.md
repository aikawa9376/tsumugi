# Keep a project's resources as references on the work host that agents look up

Status: Accepted

## Context

The material around a project — a configuration file, a design document, a
site — lives outside the repository, and the user wants their agents to find
it when they say "check the conf link". A conversation only sees what it
came across, so the links the user gathers are lost to the next session and
to the next window.

## Decision

The user's resources are references, a path or an address with a label and a
note, kept by the work host for the project that holds them. Nothing they
point at is copied. Agents are not handed them up front; they look them up
when the user points at material loosely.

## Alternatives considered

- Copy the material into Tsumugi or into the repository. The copy goes stale
  the moment the original changes, and a repository would carry the user's
  private links.
- Keep a file of links in the repository. It would be shared with people who
  never asked for it, and a link the user keeps only for themselves has no
  place there.
- Keep them in the window. Each window and each computer reaching the work
  machine would hold different resources of the same project.
- Put every resource into every prompt. Most prompts need none, and a long
  list costs every turn.

## Consequences

- A reference whose target is gone stays listed and is reported missing,
  rather than being dropped.
- Every window and every session of a project sees the same resources, and
  they outlive both.
- An agent that is not told to look will not, so the instructions are part of
  the feature.
