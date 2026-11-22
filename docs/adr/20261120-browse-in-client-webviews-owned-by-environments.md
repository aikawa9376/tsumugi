# Browse in client WebViews owned by environments

Status: Accepted

## Context

Work often needs a web page beside its editors and terminals. The same site
may be used with different accounts in different environments.

## Decision

Browse in client WebViews owned by environments, with a separate website
data store for each environment and, within an environment, for each named
browser profile.

## Alternatives considered

- Open an external browser. Its windows and accounts do not follow the
  environment the user is working in.
- Embed pages in frames of the workspace page. Sites can refuse framing,
  and the workspace's own page would own their browsing context.
- Run browsers on work hosts. Displaying and controlling a remote browser
  would require a separate transport and browser runtime.

## Consequences

- Browsing uses the client's network, including its meaning of localhost.
- Switching work environments can keep website accounts separate.
- One environment can hold pages signed in as different users, each in a
  profile of its own.
- Browser pages have their own input focus and must give it back to the
  workspace explicitly.
