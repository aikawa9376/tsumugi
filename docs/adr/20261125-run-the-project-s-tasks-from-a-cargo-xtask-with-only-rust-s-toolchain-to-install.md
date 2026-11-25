# Run the project's tasks from a cargo xtask with only Rust's toolchain to install

Status: Accepted

## Context

The quality gate, the tests' isolation and the bundles were recipes for a
task runner, whose tools came from a Nix development shell. Neither is on
every machine Tsumugi is developed on, and Nix does not run on Windows, so the
gate could not be run as written there, CI ran Windows another way, and a test
of a recipe failed wherever the task runner was missing.

## Decision

The project's tasks are a Rust program in the workspace, run through Cargo as
an xtask. Developing Tsumugi needs only rustup, which reads the pinned
toolchain, and the platform's own packages; CI installs the same and runs the
same tasks on every platform. The tasks are tested as Rust code, by what they
would run, rather than by running them.

## Alternatives considered

- Keep the task runner and Nix, and install them where they are missing. Each
  machine then carries two more tools for what Cargo can already run, and
  Windows still runs the gate apart.
- Keep the task runner as a thin layer over the xtask. Every task is then
  named twice, and the two drift apart.

## Consequences

- The gate is one command, the same locally and in CI on every platform.
- The system libraries the window links come from the platform's packages,
  not pinned by a lock file.
- Nix remains only a way to build the package.
