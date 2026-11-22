# Open a failed check's log in a window of its own, cut to the excerpt a fix request quotes

Status: Accepted

## Context

A check in a pull request's panel opened its page on GitHub, so finding why it
failed meant leaving for a browser and scrolling a log that install and cache
output bury. Preparing a fix request already reads the end of a failed job's
log and keeps the earlier errors beside it. Orca shows the same tail of a
check's log where the check is expanded and scrolls to its first error.

## Decision

A failed or running check run by GitHub Actions opens its job's log in a
window of the content, which shows the same excerpt a fix request quotes, cut
from the same code, and scrolls to the first error in it. The log is read on
the work host, by `gh`, and only the excerpt reaches the window.

## Alternatives considered

- Read and show the whole log. Logs run to megabytes, which would cross the
  connection to a work machine and bury the failure again.
- Show the log inline under the check in the details panel. The panel is a
  list of rows with no room to read, scroll and act on a log, and a window
  gets the viewers' keys.
- Excerpt it a second way for reading. Two cuts of one log would disagree
  about where the errors are, and each would need its own fixes.

## Consequences

- What is read of a log is one piece of code for a fix request and for a
  reader, so an improvement to one reaches both.
- A log that is cut says so, and the lines before it stay on GitHub.
- A check that is not an Actions job has no log to open and keeps opening its
  page.
