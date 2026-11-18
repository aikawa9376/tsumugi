# Log through tracing to standard error, chosen by TSUMUGI_LOG

Status: Accepted

## Context

Tsumugi's programs said what happened while they ran with lines written
straight to standard error. Nothing told those lines apart by how much they
matter, so none could be quieted or asked for in more detail, and the
libraries the window is drawn with already log through `tracing` with a
subscriber of their own. The hosts run as services, and the service manager
already keeps what a service writes to standard error. Commands Tsumugi runs
inside sessions inherit the environment a user set there for their own
programs.

## Decision

Tsumugi logs through `tracing`, and only the `tsumugi` program sets up the
subscriber: it writes to standard error, and the `TSUMUGI_LOG` environment
variable chooses what is written. Libraries log at debug, and at info only for the
flow an operator follows; the programs log a failure they recover from as a
warning and one that stops part of the work as an error. A failure returned to
the caller is not logged where it is returned. What a command tells the person
who ran it is its output, not a log, and stays plain.

## Alternatives considered

- A log file in the instance's state, beside or instead of standard error.
  The services' logs are kept already by the service manager, and Tsumugi would
  take on rotating and removing files.
- `RUST_LOG` to choose what is written. A value a user set in a session for
  their own programs would reach the commands Tsumugi runs there.

## Consequences

- One variable turns on detail for any part of Tsumugi, without a rebuild.
- The window writes the subscriber Tsumugi sets up, not the one its UI library
  would, so the library's own detail is quiet unless asked for.
- A line a program ends with when it fails stays plain, so a caller reading it
  sees the reason as before.
