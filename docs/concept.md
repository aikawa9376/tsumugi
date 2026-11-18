# Tsumugi

## What it is

Tsumugi is a desktop workspace for working through software changes with coding
agents, a shell and Neovim. A project can hold several conversations at once;
the work stays together while you move your attention between them.

The name つむぎ comes from spinning thread. Conversations, files and terminal
sessions are the threads: Tsumugi gives them a shared place without taking over
the tools that do the work.

## Who it is for

People who prefer Vim's way of moving and editing, use coding agents as part of
their own development, and want to keep several pieces of work within reach.
The workspace spans Linux, macOS and Windows, with Linux as the primary
development environment. This is a personal, experimental product whose scope
grows from the maintainer's daily use and focused contributions.

## Where its value lies

An agent's terminal, the prompt you are writing, the relevant files and the
changes it has made are close enough to move between without rebuilding your
working context. A conversation waiting for your decision stays visible.

Neovim is an actual running editor. Your normal file-editing configuration can
live beside the embedded prompt, so a file tab feels like the editor you already
use. Shells run with your account's environment. Agents use their own accounts
and their own terminal interfaces.

Work has a longer lifetime than a window. Hosts keep the terminal sessions and
workspace state independently, so closing a view does not discard a running
conversation or an editor's unsaved buffer.

## Principles

- **Keep the person in charge.** Sessions answer to the user. Opening another
  conversation makes another visible piece of work with its own context.
- **Keep tools recognizable.** Agent interfaces, Neovim configuration and shell
  behavior remain familiar when brought into the workspace.
- **Make movement cheap.** Keyboard navigation, tabs, splits and a project tree
  make changing focus part of working, rather than a separate management task.
- **Keep ownership clear.** The window presents the work; hosts own running
  programs and shared state; adapters understand individual agents.
- **Be honest about the product.** Features follow working implementations and
  tests. Reused open source remains credited, and experimental support is
  described as such.

## Scope

Tsumugi supplies the workspace around independently installed tools. Agent
providers supply model access, authentication and their own execution policies.
Repositories remain ordinary Git or jj repositories, and files remain yours to
open with other editors. The workspace is a place to do and review work, with
its own small set of controls for keeping that work connected.
