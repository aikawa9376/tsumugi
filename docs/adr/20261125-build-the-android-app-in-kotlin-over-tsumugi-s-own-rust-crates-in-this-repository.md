# Build the Android app in Kotlin over Tsumugi's own Rust crates, in this repository

Status: Accepted

## Context

The Android app speaks the work protocol through a work machine's gateway,
over TLS that pins each side's key, and pairs as any client does. The
protocol's types, its versions, the pinned TLS and the pairing exchange are
Rust crates in this repository, and every change to the protocol has to reach
the app. Orca's companion is React Native with Expo, which pays off by serving
iOS and Android from one codebase; Tsumugi builds for Android alone.

## Decision

The app's interface is Kotlin, and everything below it, from the connection
and pairing to the work protocol, is Tsumugi's own Rust crates, which the app
calls through generated bindings. The app lives in this repository beside the
crates it uses.

## Alternatives considered

- React Native with Expo, as Orca does. Its gain is a second platform, which
  is not built; the pinned TLS and the protocol would be written again in
  TypeScript, or reached through native modules anyway.
- Kotlin with the protocol and the pinned TLS written again in Kotlin. Two
  implementations of the same wire format and key checks drift apart, and a
  mistake in the copy weakens how the phone knows the machine.
- A repository of its own. A change to the protocol and the app that follows
  it would land in two repositories, and the crates would have to be
  published and versioned for the app to use them.

## Consequences

- A change to the work protocol and the app's handling of it lands in one
  commit, and the app is built against the crates of the same revision.
- The repository takes in the Android toolchain, Gradle and the NDK; the
  app's build runs as a quality-gate job of its own, so the Rust gate does not
  need them.
- The crates the app uses must build for Android's targets.
