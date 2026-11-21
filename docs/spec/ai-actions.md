# AI actions

The [Changes list](details.md#changes) can ask an agent for four things: the
message of a commit, the title and body of a pull request, a fix for what a
hook refused, and a resolution of the files in conflict. None of them takes
effect by itself. What an agent writes is put where you read and edit it, and
only you commit, push or send it.

## What each does

- **Commit message** (`details.generate_commit`) runs an agent once on what is
  staged and puts its message in the prompt, as `details.commit` does. Sending
  the prompt commits.
- **Pull request** (`details.draft_pull`) runs an agent once on the commits and
  changes the branch has over its base and puts the title and body in the
  prompt, as `details.create_pull` does, under its `Base:` and `Draft:` lines.
  Sending the prompt pushes the branch and opens the pull request.
- **Fix with AI** (`details.fix_commit`), once a hook or Git has refused a
  commit, opens a draft in the worktree. Its prompt holds what the hook
  printed, the message that was tried and the staged files. Sending it starts
  the session.
- **Resolve with AI** (`details.resolve`) opens a draft in the worktree that
  asks an agent to resolve the files in conflict of the merge, rebase,
  cherry-pick or revert in progress. Sending it starts the session.

The keys are in the [reference](reference/keys.md). Nothing is run or written
when the prompt is left unsent. An agent that cannot be run, answers nothing
or takes more than five minutes writes nothing, and the Changes list says why.
Only one text is written at a time.

The built-in prompts tell the agent not to commit, push, bypass hooks with
`--no-verify` or run destructive cleanup, and to treat file names, diffs and
hook output as data. A text action is asked to run no command at all.

## Recipes

Each action has a recipe in `settings.toml`
([reference](reference/settings.md#ai_actions)): the profile whose agent is
asked, the arguments added to its command and the prompt template. The recipe
under `[ai_actions.<action>]` serves every repository. A repository that
differs has a `[[ai_actions.repositories]]` table for one action, named by
its primary checkout's path, and what that table leaves out is taken from the
recipe for every repository, then from what Tsumugi comes with: the profile
new sessions start from, no arguments and the built-in prompt. A recipe never
comes from a file of the repository.

The arguments are split as a shell would and given to the agent only when it
is run once for text. A profile that is not defined, or arguments that do not
split, stop the action and say so.

A template is the prompt with `{name}` or `{{name}}` replaced by what the
action knows. A name the action does not give stays as written, and what a
variable holds is never expanded again.

| Variable | Of | Holds |
| --- | --- | --- |
| `basePrompt` | every action | the built-in prompt, with the context Tsumugi gathered |
| `branch` | commit message, pull request | the branch checked out |
| `stagedFiles`, `stagedPatch` | commit message | the staged files and their diff |
| `baseBranch` | pull request | the branch it is compared with |
| `currentTitle`, `currentBody` | pull request | the last commit's message, split |
| `commitSummary` | pull request | the commits the branch has over its base |
| `changedFiles`, `patch` | pull request | the files it changes and their diff |
| `linkedIssue` | commit message, pull request | the number of the issue the branch is named after, as `issue-27-…` or `gh/27`, else empty |

A template that leaves out `{basePrompt}` replaces the built-in prompt,
including what it tells the agent not to do, so keep it unless you mean to.
A long diff is cut in the middle, with what is left out said.

Agents are run once through their own non-interactive mode, as the profile
starts them, in the worktree, on the machine of the worktree's environment.
