# Preview a past conversation by finding it again on the work host

Status: Accepted

## Context

The list of past conversations shows a title. To choose among them the user
wants to read the first prompt and the latest turns first. The files that hold
them are on the work machine, and can be very large or not transcripts at all.

## Decision

The window asks for a preview of one conversation by naming it, and the work
host finds that conversation again among those its logins keep and reads its
file, only at the file's two ends. A file named by the window is never read.

## Alternatives considered

- Send the preview of every listed conversation with the list. Most are never
  looked at, and the list is read again every half minute.
- Let the window name the file to read. A window, and the clients paired with a
  work host, could then make it read any file it can reach.

## Consequences

- A preview costs one search of the conversation's directory, and a
  conversation that is gone has none.
- Reading a transcript has a bound whatever the transcript holds.
