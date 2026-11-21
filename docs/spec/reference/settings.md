# Settings reference

<!-- Written by `just references` from the code; edit the code, not this file. -->

Every setting `settings.toml` holds, table by table: its type, what it is
when the file does not name it, and what it does
([settings](../settings.md)).

## The top level

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `appearance` | one of `system`, `light`, `dark` | `"system"` | Which of the two themes colours the console and the window: `system` follows macOS as it switches between light and dark, `light` and `dark` keep to one. The Settings window chooses it, and the two themes, once for every environment and writes them here. |
| `auto_scale` | boolean | `true` | Adapt window scale to the current display. |
| `close_successful_commands` | boolean | `true` | Close a command's tabs and splits when it finishes successfully; its output stays available from the tree. Off, its windows stay open too. |
| `dark_theme` | string | `"Solarized Dark"` | The theme drawn while the appearance is dark, named as the light one is. |
| `font_family` | string | `""` | The font the console and the prompt are drawn in, as the system names its family; empty for Tsumugi's own, JetBrainsMono Nerd Font Mono. Tsumugi's own stands behind any other, so Nerd Font glyphs draw whatever font is chosen, and a Japanese font behind that. A line is as tall as the font asks, as in a terminal. |
| `font_size` | number | `15.0` | The font's size, in points, from 8 to 32. |
| `font_weight` | integer | `400` | The font's weight, from 100 to 900; Tsumugi's own comes in regular and bold, and draws other weights in the nearer of them. |
| `keep_awake` | boolean | `true` | Keep the Mac from idle sleep while a session or a shell runs, the window open or not; the display still sleeps. |
| `light_theme` | string | `"Solarized Light"` | The theme drawn while the appearance is light, by name: one tsumugi comes with, or one in `themes/` beside `settings.toml`. |
| `notify_done` | boolean | `true` | Show a notification when a session finishes its turn while the window is not in front, as `notify_waiting` does. |
| `notify_failed` | boolean | `true` | Show a notification when a session fails while the window is not in front, as `notify_waiting` does. |
| `notify_waiting` | boolean | `true` | Show a notification when a session starts waiting on you while the window is not in front; with several windows open, the one holding the lowest number tells. |
| `profile` | string | `"claude"` | The profile a new session starts from, by id, when it names neither a profile nor an agent; left out, the first profile. A session not started yet can be switched to another agent for the current run of the window. |
| `prompt_max_rows` | integer | `12` | The most lines the prompt grows to before it scrolls inside. |
| `prompt_min_rows` | integer | `3` | The fewest lines the prompt shows. |
| `rename_sessions` | boolean | `true` | Let a session's agent name its session once it has answered the first prompt, and again when the work turns to something else; a name given by hand, above the prompt or by renaming, is kept all the same. |
| `restore_sessions` |  | `"ask"` | Whether the sessions and terminals a terminal host took with it, as it restarted, crashed or went with the machine, come back: offered by the window to bring back, at once, or never. |
| `scratch_dir` | string | `""` | Where scratch sessions run, `~` standing for your home folder; empty for Tsumugi's own folder. |
| `zoom` | number | `1.0` | The user's zoom adjustment, multiplied by the display scale when automatic scaling is enabled. |

## `[console]`

How the console takes keys and draws its cursor.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `console.cursor_blink` | boolean | `false` | Whether the cursor blinks when the program lets it. |
| `console.option_as_alt` | one of `both`, `left`, `right`, `none` | `"both"` | Which Option keys send Alt to what runs there, `both`, `left`, `right` or `none`, the others typing what macOS makes of the key. |
| `console.renderer` | one of `builtin`, `vte`, `kitty` | `"builtin"` | Console display: `builtin`, `vte` for Linux terminal tabs with GTK3 VTE, or `kitty` for Linux session and terminal consoles with kitty and an X11/XWayland window. The built-in display is used when the chosen display is unavailable, while images are shown, and for Neovim GUI terminals. |

## `[delegation]`

How sessions opened by sessions run.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `delegation.enabled` | boolean | `true` | Whether sessions may open sessions beside them. Off, a start is refused without ending those already running; it is off while the file cannot be read. |

## `[details]`

How much the details panel reads and shows.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `details.diff_lines` | integer | `2000` | The lines of a diff shown before it is cut. |
| `details.links_kept` | integer | `100` | The links of each kind the overview keeps, the newest, up to 1000. |
| `details.links_shown` | integer | `5` | The links of each kind the overview shows before a row that shows them all. |
| `details.log_commits` | integer | `50` | The changes Log lists. |

## `[neovim]`

How a Neovim tab's Neovim starts, as an agent does; an empty command runs `nvim`.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `neovim.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of Tsumugi's own; empty for the program it starts: the profile's agent's, or Neovim. |
| `neovim.env` | table of strings | `{}` | Variables set for it. |
| `neovim.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

## `[panel]`

What the details panel is made of, chosen once for every environment; see [details](../details.md#components).

| Setting | Type | Default | Description |
| --- | --- | --- | --- |

### `[panel.environment]`

What an environment's heading shows.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `panel.environment.foot` | array of strings | `[]` | Shown under every tab, top first: each a component's name, or a table naming it with how it shows. |

### `[[panel.environment.tabs]]`

The tabs, in order; the keys that show a tab name it by its place.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `panel.environment.tabs.components` | array of strings |  | What it shows, top first: each a component's name, or a table naming it with how it shows. |
| `panel.environment.tabs.icon` | string |  | The Tabler icon beside its name, by its name in Tabler; none when unset. |
| `panel.environment.tabs.title` | string |  | Its name on the tab. |

### `[panel.github]`

What a pull request's or an issue's row shows.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `panel.github.foot` | array of strings | `[]` | Shown under every tab, top first: each a component's name, or a table naming it with how it shows. |

### `[[panel.github.tabs]]`

The tabs, in order; the keys that show a tab name it by its place.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `panel.github.tabs.components` | array of strings |  | What it shows, top first: each a component's name, or a table naming it with how it shows. |
| `panel.github.tabs.icon` | string |  | The Tabler icon beside its name, by its name in Tabler; none when unset. |
| `panel.github.tabs.title` | string |  | Its name on the tab. |

### `[panel.session]`

What a session, draft, shell or file shows.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `panel.session.foot` | array of strings | `[]` | Shown under every tab, top first: each a component's name, or a table naming it with how it shows. |

### `[[panel.session.tabs]]`

The tabs, in order; the keys that show a tab name it by its place.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `panel.session.tabs.components` | array of strings |  | What it shows, top first: each a component's name, or a table naming it with how it shows. |
| `panel.session.tabs.icon` | string |  | The Tabler icon beside its name, by its name in Tabler; none when unset. |
| `panel.session.tabs.title` | string |  | Its name on the tab. |

## `[picker]`

Where the picker looks for directories to start a session in.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `picker.ghq` | boolean | `true` | Offer the repositories ghq keeps, and its roots in the file tree. |
| `picker.roots` | array of strings | `[]` | Folders whose folders are offered, and which the file tree lists, `~` standing for your home folder. |

### `[[picker.scopes]]`

What the picker searches, each `[[picker.scopes]]` table one mix of sources: the first is where it opens, `Ctrl+g` steps through the others, and a scope with a `prefix` is entered by typing it first. Writing any replaces those the picker starts with ([picker](../tree.md#searching-more-than-sessions-and-directories)).

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `picker.scopes.name` | string | `""` | What the scope is called where the picker shows it. |
| `picker.scopes.prefix` | string | `""` | What is typed first to enter the scope instead of stepping to it; none when empty. What follows it is what is searched for. |
| `picker.scopes.sources` | array of `sessions`, `directories`, `worktrees`, `resources`, `files`, `actions` | `[]` | The sources it mixes, their rows listed in this order. |

## `[[profiles]]`

The profiles sessions start from, each `[[profiles]]` table one of them ([settings](../settings.md#profiles)). An agent's default profile is the default profile when that starts the agent, else the agent's first profile.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.agent` | one of `claude`, `codex`, `pi`, `opencode`, `cursor` |  | The agent it starts, `claude`, `codex`, `pi`, `opencode` or `cursor`. |
| `profiles.home` | string |  | The folder of its own its agent keeps its login, history and settings in, as an absolute path without `~`, `.` or `..` and with no `/` doubled or at its end, which no other profile names; it is fixed when the login is made, since the CLIs tell logins apart by it as it is written. Left out, the agent runs in its own environment, shared with the terminal. |
| `profiles.id` | string |  | The profile's id, kept when it is renamed: lowercase letters, digits and `-`. |
| `profiles.name` | string | `""` | What the profile is called; empty for its id. |

### `[profiles.claude]`

Claude Code's model, permission mode and tools denied, for a `claude` profile; what is left empty is Claude Code's own choice.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.claude.confirm_channel_warning` | boolean | `true` | Whether Tsumugi confirms, once a launch, Claude Code's warning about loading development channels when it names Tsumugi's own channel and nothing else; every other dialog is left to you. It applies to sessions launched while it is on. |
| `profiles.claude.disallowed_tools` | array of strings | `[]` | The tools denied it. |
| `profiles.claude.model` | string | `""` | The model it starts with, as Claude Code's `--model` takes it. |
| `profiles.claude.permission_mode` | string | `""` | Its permission mode, one of `default`, `acceptEdits`, `plan` and `bypassPermissions`. |

### `[profiles.codex]`

Codex's model and reasoning effort, approval policy and sandbox, for a `codex` profile; empty values leave Codex's own settings in effect.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.codex.approval_policy` | string | `""` | When Codex asks before running a command: empty, `on-request` or `never`. |
| `profiles.codex.model` | string | `""` | Codex's model; empty for its own choice. |
| `profiles.codex.reasoning_effort` | string | `""` | Its reasoning effort, one the selected model and Codex version support; empty for its own choice. |
| `profiles.codex.sandbox_mode` | string | `""` | Where its commands can write: empty, `read-only`, `workspace-write` or `danger-full-access`. |

### `[profiles.cursor]`

Cursor CLI's model, mode and sandbox, for a `cursor` profile; empty values leave Cursor's own settings in effect.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.cursor.mode` | string | `""` | The mode it starts in: empty for its agent mode, `plan` or `ask`. |
| `profiles.cursor.model` | string | `""` | Cursor's model, as `agent models` lists it; empty for its own choice. |
| `profiles.cursor.sandbox` | string | `""` | Whether its sandbox is `enabled` or `disabled`; empty for its own setting. |

### `[profiles.launch]`

How its agent is started; an empty command runs the agent's own program.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.launch.command` | array of strings | `[]` | The program and the arguments it always gets, ahead of Tsumugi's own; empty for the program it starts: the profile's agent's, or Neovim. |
| `profiles.launch.env` | table of strings | `{}` | Variables set for it. |
| `profiles.launch.via_shell` | boolean | `false` | Whether your shell starts it, as an interactive login shell, so that what its startup files set reaches it. |

### `[profiles.opencode]`

OpenCode's model as `provider/model`, the agent it starts as, its permission rules and whether it approves on its own, for an `opencode` profile; empty values leave OpenCode's own settings in effect.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.opencode.agent` | string | `""` | The primary agent it starts as, such as `build` or `plan`; empty for its own default. |
| `profiles.opencode.auto` | boolean | `false` | Whether every tool call no rule denies is allowed without asking, as OpenCode's `--auto` does. |
| `profiles.opencode.model` | string | `""` | OpenCode's model as `provider/model`, with `#variant` for one of its variants; empty for its own choice. |

### `[[profiles.opencode.permissions]]`

Permission rules put after those of its login's own configuration, the last that matches a tool call deciding whether it is allowed, asked about or denied.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.opencode.permissions.action` | string | `""` | What a tool does, such as `edit`, `shell` or `webfetch`, or `*` for anything. |
| `profiles.opencode.permissions.effect` | string | `""` | `allow`, `ask` or `deny`. |
| `profiles.opencode.permissions.resource` | string | `""` | What it does it to, as a pattern: a path, a command, or `*`. |

### `[profiles.pi]`

Pi's model as `provider/id`, thinking level and tools, for a `pi` profile; empty values leave Pi's own settings in effect.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `profiles.pi.model` | string | `""` | Pi's model as `provider/id`; empty for its own choice. |
| `profiles.pi.thinking` | string | `""` | Its thinking level: `off`, `minimal`, `low`, `medium`, `high`, `xhigh` or `max`, as the model supports; empty for its own choice. |
| `profiles.pi.tools` | array of strings | `[]` | The tools it may use, by Pi's names, such as `read`, `bash`, `edit` and `write`; empty for Pi's own set. |

When the file holds none, it holds these, as Tsumugi writes them:

```toml
[[profiles]]
id = "claude"
name = "Claude Code"
agent = "claude"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.claude]
model = ""
permission_mode = ""
disallowed_tools = []
confirm_channel_warning = true

[[profiles]]
id = "codex"
name = "Codex"
agent = "codex"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.codex]
model = ""
reasoning_effort = ""
approval_policy = ""
sandbox_mode = ""

[[profiles]]
id = "pi"
name = "Pi"
agent = "pi"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.pi]
model = ""
thinking = ""
tools = []

[[profiles]]
id = "opencode"
name = "OpenCode"
agent = "opencode"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.opencode]
model = ""
agent = ""
permissions = []
auto = false

[[profiles]]
id = "cursor"
name = "Cursor CLI"
agent = "cursor"

[profiles.launch]
command = []
via_shell = false

[profiles.launch.env]

[profiles.cursor]
model = ""
mode = ""
sandbox = ""
```

## `[shell]`

How a shell tab's shell starts.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `shell.command` | array of strings | `[]` | The shell and its arguments; empty for your account's login shell, started as a login shell. |
| `shell.env` | table of strings | `{}` | Variables set for it. |

## `[usage]`

Machine resources sampled for a window that does not say which it shows; the Settings window chooses them once for every environment.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `usage.cpu` | boolean | `true` | Show the whole machine's CPU use. |
| `usage.gpu` | boolean | `true` | Show the GPU's use, when the system reports it. |
| `usage.memory` | boolean | `true` | Show the physical memory used, as a share of all of it. |
| `usage.storage` | boolean | `true` | Show the capacity used on the startup volume. |

## `[worktree_setup]`

Whether the setup script a repository lists runs in its new worktrees.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `worktree_setup.run_policy` |  | `"ask"` | What a repository with no `[[worktree_setup.repositories]]` table of its own gets: `run`, `ask` to show the script and run it when you say so, or `skip`. |

### `[[worktree_setup.repositories]]`

The repositories that differ, each `[[worktree_setup.repositories]]` table one of them.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `worktree_setup.repositories.path` | string | `""` | The repository's primary checkout, the directory holding its `.git`. |
| `worktree_setup.repositories.run_policy` |  | `"ask"` | What its new worktrees' setup script does. |
