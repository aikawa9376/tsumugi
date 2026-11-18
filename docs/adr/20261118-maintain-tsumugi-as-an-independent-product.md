# Maintain Tsumugi as an independent product

Status: Accepted

## Context

Tsumugi is being prepared for public development as a personal Linux desktop
workspace. Its starting implementation contains substantial MIT-licensed
Adroit code, but product ownership, support and distribution belong to its
own maintainers.

## Decision

Maintain Tsumugi as an independently branded product, with its own application
identity and release process, while preserving the authorship and licensing
of reused code and assets.

## Alternatives considered

- Keep a compatibility-branded distribution: makes ownership and support
  ambiguous and ties product identity to another project's naming.
- Reimplement every component: discards functioning behavior without creating
  corresponding value for the intended users.
- Present reused code as wholly original work: misrepresents its authorship.

## Consequences

- Product naming and distribution are maintained together across components.
- Provenance and license notices accompany source and binary distributions.
- Reusing code is documented honestly without making upstream maintainers
  responsible for Tsumugi.
