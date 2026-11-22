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
| `hibernate_after` | integer | `0` | How many minutes a session whose turn is done waits, out of view and left alone, before Tsumugi stops its program to spare the machine and keeps its row and tabs; focusing it or sending to it wakes it with its conversation. 0, the default, never does. One that works, waits on a dialog or has sessions it opened still running is never stopped. |
| `keep_awake` | boolean | `true` | Keep the Mac from idle sleep while a session or a shell runs, the window open or not; the display still sleeps. |
| `language` | string | `""` | The language a leader speaks to the user in, as a tag such as `ja` or `en`; empty for the language of the environment's locale. What the user does not read — prompts to sessions, commit messages, code — stays English whatever this says. |
| `light_theme` | string | `"Solarized Light"` | The theme drawn while the appearance is light, by name: one tsumugi comes with, or one in `themes/` beside `settings.toml`. |
| `notify_done` | boolean | `true` | Show a notification when a session finishes its turn while the window is not in front, as `notify_waiting` does. |
| `notify_failed` | boolean | `true` | Show a notification when a session fails while the window is not in front, as `notify_waiting` does. |
| `notify_usage` | boolean | `true` | Show a notification when a limit of an agent account passes 80% used while the window is not in front, as `notify_waiting` does. |
| `notify_waiting` | boolean | `true` | Show a notification when a session starts waiting on you while the window is not in front; with several windows open, the one holding the lowest number tells. |
| `profile` | string | `"claude"` | The profile a new session starts from, by id, when it names neither a profile nor an agent; left out, the first profile. A session not started yet can be switched to another agent for the current run of the window. |
| `prompt_max_rows` | integer | `12` | The most lines the prompt grows to before it scrolls inside. |
| `prompt_min_rows` | integer | `3` | The fewest lines the prompt shows. |
| `rename_sessions` | boolean | `true` | Let a session's agent name its session once it has answered the first prompt, and again when the work turns to something else; a name given by hand, above the prompt or by renaming, is kept all the same. |
| `restore_sessions` |  | `"ask"` | Whether the sessions and terminals a terminal host took with it, as it restarted, crashed or went with the machine, come back: offered by the window to bring back, at once, or never. |
| `scratch_dir` | string | `""` | Where scratch sessions run, `~` standing for your home folder; empty for Tsumugi's own folder. |
| `usage_left` | boolean | `true` | Show how much of each usage limit is left; off, how much is used. |
| `zoom` | number | `1.0` | The user's zoom adjustment, multiplied by the display scale when automatic scaling is enabled. |

## `[ai_actions]`

The agents asked for commit messages, pull requests, hook failures and conflicts from the Changes list, and how.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |

### `[ai_actions.commit_message]`

Writes a commit message, run once; the user reads it before it commits.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `ai_actions.commit_message.args` | string | `""` | Arguments added to the agent's command when it is run once for text, split as a shell would, such as `--model sonnet`. They are not given to the sessions the other actions start. |
| `ai_actions.commit_message.profile` | string |  | The profile whose agent is asked; the profile new sessions start from when left out. |
| `ai_actions.commit_message.template` | string | `""` | The prompt, in which `{basePrompt}` stands for the built-in prompt of the action. A template that leaves it out replaces the built-in prompt, including what it tells the agent not to do. The other variables an action takes are in the [reference](../ai-actions.md#recipes). |

### `[ai_actions.fix_commit_failure]`

Starts a session that fixes what refused a commit; it does not bypass the hook, commit or push.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `ai_actions.fix_commit_failure.args` | string | `""` | Arguments added to the agent's command when it is run once for text, split as a shell would, such as `--model sonnet`. They are not given to the sessions the other actions start. |
| `ai_actions.fix_commit_failure.profile` | string |  | The profile whose agent is asked; the profile new sessions start from when left out. |
| `ai_actions.fix_commit_failure.template` | string | `""` | The prompt, in which `{basePrompt}` stands for the built-in prompt of the action. A template that leaves it out replaces the built-in prompt, including what it tells the agent not to do. The other variables an action takes are in the [reference](../ai-actions.md#recipes). |

### `[ai_actions.pull_request]`

Writes a pull request's title and body, run once; the user reads them before they are sent.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `ai_actions.pull_request.args` | string | `""` | Arguments added to the agent's command when it is run once for text, split as a shell would, such as `--model sonnet`. They are not given to the sessions the other actions start. |
| `ai_actions.pull_request.profile` | string |  | The profile whose agent is asked; the profile new sessions start from when left out. |
| `ai_actions.pull_request.template` | string | `""` | The prompt, in which `{basePrompt}` stands for the built-in prompt of the action. A template that leaves it out replaces the built-in prompt, including what it tells the agent not to do. The other variables an action takes are in the [reference](../ai-actions.md#recipes). |

### `[[ai_actions.repositories]]`

The recipes that differ in a repository, each `[[ai_actions.repositories]]` table one action of one repository.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `ai_actions.repositories.action` |  | `null` | The action it is the recipe of: `commit_message`, `pull_request`, `fix_commit_failure` or `resolve_conflicts`. |
| `ai_actions.repositories.args` | string | `""` | Arguments added to the agent's command; left out, those for every repository. |
| `ai_actions.repositories.path` | string | `""` | The repository's primary checkout, the directory holding its `.git`. |
| `ai_actions.repositories.profile` | string |  | The profile whose agent is asked; left out, the one for every repository. |
| `ai_actions.repositories.template` | string | `""` | The prompt; left out, the one for every repository. |

### `[ai_actions.resolve_conflicts]`

Starts a session that resolves the files in conflict; it does not push.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `ai_actions.resolve_conflicts.args` | string | `""` | Arguments added to the agent's command when it is run once for text, split as a shell would, such as `--model sonnet`. They are not given to the sessions the other actions start. |
| `ai_actions.resolve_conflicts.profile` | string |  | The profile whose agent is asked; the profile new sessions start from when left out. |
| `ai_actions.resolve_conflicts.template` | string | `""` | The prompt, in which `{basePrompt}` stands for the built-in prompt of the action. A template that leaves it out replaces the built-in prompt, including what it tells the agent not to do. The other variables an action takes are in the [reference](../ai-actions.md#recipes). |

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

## `[floating_terminal]`

The shell that floats over the window.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `floating_terminal.directory` | string | `"~"` | The folder its shells start in, `~` standing for the home folder of the machine they run on; empty for that home folder. |

## `[gate]`

The command that gates each repository, run once at a time on the work host for the sessions that ask.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |

### `[[gate.repositories]]`

The repositories with a gate, each `[[gate.repositories]]` table one.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `gate.repositories.command` | string | `""` | The command line that runs the gate, for example `just ci`. |
| `gate.repositories.known_failures` | array of strings | `[]` | Failures the gate always has on this machine: a failed test whose line holds one of these words does not fail the gate. |
| `gate.repositories.path` | string | `""` | The repository's primary checkout, the directory holding its `.git`. |

## `[governor]`

What sessions may use of the work machine: how many run at once, how their builds share, how large worktrees may grow.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `governor.build_cache` |  | `"own"` | How the builds of a repository's worktrees share their work: `own`, `shared` or `sccache`. |
| `governor.build_jobs` | integer | `0` | How many jobs the builds of the sessions Tsumugi starts run at once (`CARGO_BUILD_JOBS`). |
| `governor.cleanup_branches` | boolean | `false` | Whether cleaning up a worktree also deletes its branch, which it does only for a branch the base already holds. |
| `governor.max_sessions` | integer | `0` | The most agent sessions that run at once on the work machine. A session another opens waits for room, and is refused, with how many run, once it has waited too long; sessions you open yourself are never held. |
| `governor.wait_seconds` | integer | `120` | How many seconds a session another opens waits for room under `max_sessions` before it is refused. |
| `governor.worktree_warn_gib` | integer | `0` | The size, in GiB, of a repository's worktrees above which Tsumugi warns. |

## `[leader]`

Leader mode: the session that supervises the others, how many workers it keeps at work and how far its discretion goes.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `leader.max_workers` | integer | `2` | The most workers a leader keeps at work at once. |
| `leader.model` | string | `""` | The model the leader's own session starts, one its profile lists; the agent's own default when empty. |
| `leader.profile` | string |  | The profile the leader's own session starts from, by id; the profile new sessions start from when left out. |

### `[leader.discretion]`

What the leader decides itself and what it asks the user first.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `leader.discretion.close_issues` |  | `"decide"` | Closing an issue whose work is merged. |
| `leader.discretion.delete_unmerged_branches` |  | `"ask"` | Deleting a branch that is not merged. |
| `leader.discretion.design_choices` |  | `"ask"` | Settling a design question Orca does not settle. Where Orca does, the leader follows it without asking either way. |
| `leader.discretion.force` |  | `"ask"` | Operations that discard work or rewrite history: forced pushes, hard resets, discarded changes, ending a session by force. |
| `leader.discretion.manage_sessions` |  | `"decide"` | Starting, resuming and, once their work is done, closing the worker sessions it supervises. |
| `leader.discretion.merge_over_changes` |  | `"ask"` | Merging into `main` while its checkout has uncommitted changes. |
| `leader.discretion.merge_to_main` |  | `"decide"` | Merging a finished branch into `main` while the main checkout is clean. |
| `leader.discretion.push` |  | `"ask"` | Pushing to a remote. |
| `leader.discretion.remove_worktrees` |  | `"decide"` | Deleting the worktree of a finished branch. |
| `leader.discretion.research_issues` |  | `"ask"` | Starting an issue that is only to be researched. |

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
| `profiles.poll_usage` | boolean |  | Whether the account's usage limits are asked of the agent; `false` stops every query for this login. Left out, they are asked. |

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

## `[worker_tiers]`

The tiers a leader starts its workers by, each one a profile and a model.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |

### `[worker_tiers.heavy]`

For large or hard tasks.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `worker_tiers.heavy.model` | string | `""` | The model it starts, one its profile lists; the agent's own default when empty. |
| `worker_tiers.heavy.profile` | string |  | The profile it starts from, by id; the profile new sessions start from when left out. |

### `[worker_tiers.light]`

For small tasks.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `worker_tiers.light.model` | string | `""` | The model it starts, one its profile lists; the agent's own default when empty. |
| `worker_tiers.light.profile` | string |  | The profile it starts from, by id; the profile new sessions start from when left out. |

### `[worker_tiers.standard]`

For tasks of ordinary size.

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `worker_tiers.standard.model` | string | `""` | The model it starts, one its profile lists; the agent's own default when empty. |
| `worker_tiers.standard.profile` | string |  | The profile it starts from, by id; the profile new sessions start from when left out. |

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
