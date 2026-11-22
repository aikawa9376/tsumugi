# Reach a work machine through an ssh tunnel to a gateway on its loopback

Status: Accepted

## Context

A work machine is reached through its gateway, which listens on the network
and announces itself, and a client is let in by pairing. Many work machines
sit where that is unwelcome or impossible: a network that blocks the port, or
a machine whose owner will open nothing but ssh. Orca reaches such machines
over SSH, and its users already have their hosts, keys and agents set up for
`ssh`.

## Decision

A client may reach a work machine's gateway through a tunnel that the
OpenSSH program on the client makes, ending in the loopback of both machines,
so that the gateway needs to listen on the work machine's loopback alone and
announce nothing. The client pairs with the gateway and knows it by its key
as it would over any other network: ssh only carries the bytes.

## Alternatives considered

- Speak Tsumugi's protocol over an ssh session's standard input and output.
  It would be a second way into the hosts beside the gateway, with its own
  authorization, and leave pairing and revocation to be written twice.
- Link an ssh library and read the user's configuration ourselves. Host-key
  confirmation, passphrases, agents, jump hosts and second factors are what
  OpenSSH already does for the user, and a reimplementation would do fewer of
  them.
- Open the gateway on every interface and rely on the network. This is what
  the user cannot always do.

## Consequences

- Trust rests on the work machine's key, not on the ssh login: whoever can
  reach the tunnel's end still has to be paired.
- A tunnel that is lost ends the client's connection, which the window shows
  as any lost connection; connecting again makes a new tunnel.
- Tsumugi has to be installed on the work machine before the client can
  connect; the client says so when it is not.
