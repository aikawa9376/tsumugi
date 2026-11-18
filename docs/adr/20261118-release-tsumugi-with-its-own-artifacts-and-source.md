# Release Tsumugi with its own artifacts and source

Status: Accepted

## Context

A personal project needs a repeatable distribution process that its own
maintainers can run. Reused implementation and bundled components also need
traceable attribution and the source needed to rebuild modified versions.

## Decision

Publish Tsumugi's binaries, license notices, checksums and rebuildable source
through releases owned by the Tsumugi repository.

## Alternatives considered

- Use another project's distribution services: depends on credentials and
  support arrangements that Tsumugi does not own.
- Ship binaries alone: leaves their notices and rebuild instructions apart
  from the application people receive.

## Consequences

- Release ownership and artifact provenance stay with this project.
- Source and notices are maintained alongside distribution scripts.
- Signing and external distribution channels require the maintainers' own
  identities and arrangements before they can be enabled.
