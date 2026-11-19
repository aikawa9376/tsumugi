# Input context

On Linux, Tsumugi publishes its input destination through an
application-independent input-context protocol. An integration can discover
the foreground process through the operating system and subscribe by PID,
without knowing the application name, its configuration or its selected work
environment. Application identity and window titles keep their usual meaning.

## Subscription format

A participating GUI process listens on a Unix stream socket at
`$XDG_RUNTIME_DIR/input-context/<pid>.sock`. Without a nonempty
`XDG_RUNTIME_DIR`, the runtime root is `/run/user/<uid>`. The shared
`input-context` directory has mode `0700`; each process's socket has mode
`0600`. They are private to the user (and privileged local processes).

A connection requires no request. It immediately receives the current
snapshot, then a JSON object followed by a newline whenever the context
changes. Reconnecting always starts with a fresh snapshot:

```json
{"version":1,"pid":12345,"target":"editor"}
```

`version` is the format version, currently `1`. `pid` is the GUI process ID
used in the socket name. `target` is `none`, `gui`, `editor` or `terminal`;
`none` means no input destination is available. These fields describe
application facts; they do not name keyboard profiles or prescribe key
transformations. No keys, prompt text or filenames are sent.

Subscribers cannot control the application through this endpoint. A slow or
disconnected subscriber does not block the UI or other subscribers; a slow
subscriber may be disconnected and can reconnect for the latest snapshot.
The endpoint is removed when the GUI shuts down normally. Consumers must
handle a missing endpoint, an interrupted connection and process termination.

## Tsumugi's context

The destination follows the same receiver that delivers ordinary keys.
`editor` includes the prompt, console reader and file Neovims receiving keys
through either a GUI connection or their terminal. `terminal` includes shells
and direct input to agents, including their questions and permission dialogs.
Vim's modes do not change these destinations.

GUI controls, overlays, native fields, Settings and native menus report `gui`.
Losing application focus also reports `gui`.
