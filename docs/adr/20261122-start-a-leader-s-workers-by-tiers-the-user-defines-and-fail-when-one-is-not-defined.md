# Start a leader's workers by tiers the user defines and fail when one is not defined

Status: Accepted

## Context

A leader session starts many workers and had to write an agent, a profile and
a model into every `start_session`. That repeats what the user already
decided, and an agent that guesses a model name starts something the user
did not choose. Orca names agent, model and effort on each worker start,
leaving the choice to the caller.

## Decision

The user defines worker tiers, sizes of work such as light, standard and
heavy, each as a profile and a model in the settings. A leader starts a
worker by naming the tier, and the work host resolves it. A tier that is not
defined, or whose profile or model is not offered, fails the start and starts
nothing; Tsumugi never starts another model in its place.

## Alternatives considered

- Let the leader name agent, profile and model on every start, as Orca's
  worker start does. The user's choice is then written again by an agent
  that can get it wrong, and the user cannot change the assignment in one
  place.
- Ship default tiers with models of Tsumugi's choosing. A model changes under
  a tier without the user's say, and a profile the user does not have is
  started.
- Fall back to the default profile when a tier is not defined. The work then
  runs on a model nobody chose, silently.

## Consequences

- A leader's tiers are inert until the user defines them.
- A tier replaces the agent, profile and model of a start; naming both is
  refused.
