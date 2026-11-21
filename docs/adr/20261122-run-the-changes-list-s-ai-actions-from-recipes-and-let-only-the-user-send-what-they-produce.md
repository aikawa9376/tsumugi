# Run the Changes list's AI actions from recipes and let only the user send what they produce

Status: Accepted

## Context

Writing a commit message or a pull request, fixing what a hook refused and
resolving conflicts are work an agent does well, and Orca offers each as an
action backed by a recipe: the agent, its arguments and a prompt template.
Repositories differ in language, style and in which agent suits them. An
agent given a worktree can also commit, push and skip hooks, which would put
its work in effect before the user has read it.

## Decision

Each of the four AI actions of the Changes list is run from a recipe of the
user's settings, for every repository and where it differs for one, and its
result takes effect only when the user sends it: text is run once and put in
the prompt to be read and sent, and fixing and resolving open a draft whose
prompt the user sends to start the session. Every prompt tells the agent not
to bypass hooks, commit or push.

## Alternatives considered

- Start a session for all four, as the fixing and resolving ones are. The
  commit message and the pull request would end as an agent's reply to copy
  from, not as text in the prompt the user already edits and sends from.
- Let the agent finish what it fixed by committing or pushing. The user would
  find the result already in effect, and a hook it could not satisfy might be
  skipped to get there.
- Keep a repository's recipes in a file of the repository. A cloned
  repository would choose which program runs with which arguments on the
  user's machine, which a file in it never does for its setup script.
- One fixed prompt per action. A repository could not choose its style or its
  agent without every other repository following.

## Consequences

- A recipe is the user's own, kept with their settings, never read from a
  repository.
- What keeps an agent from committing or pushing is its prompt, not a
  sandbox: a template that leaves out the built-in prompt takes the
  instruction with it.
- An agent has to be run once without a session, so each supported agent
  needs a non-interactive way to ask it.
