# Take a page's answer to an agent only from the script the window ran on it

Status: Accepted

## Context

The scripts of a page are not trusted: they come from the web, or from a
development server whose dependencies nobody read. A page can already send the
workspace messages, so what an agent is told must not be something such a
message could produce, and what it is told must not be taken for an
instruction.

## Decision

The window reads and acts on a page by running its own script there and takes
the answer only from that script's result. The page's own message channel
carries no command and no answer, and what a page says to an agent is handed
over as the page's content, never as the workspace's word.

## Alternatives considered

- Have the injected script report back through the page's message channel. Any
  script of the page could send the same message and forge an answer, or
  pretend to be a command.
- Run the script in a world the page cannot see. The browser engines Tsumugi
  embeds offer no such world through the one interface they share.

## Consequences

- A page can lie about itself to an agent, by changing what the script reads;
  it cannot reach the workspace, other pages, or the agent's other tools.
