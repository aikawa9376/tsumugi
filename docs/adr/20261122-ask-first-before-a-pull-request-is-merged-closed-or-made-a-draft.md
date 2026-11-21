# Ask first before a pull request is merged, closed or made a draft

Status: Accepted

## Context

A pull request's tab could show its review and checks but not finish it, so
merging meant leaving for a browser. Merging cannot be taken back by Tsumugi,
and GitHub's own refusals, a missing approval, a failed required check, a
conflict, a login that may not merge, come only after the attempt. Orca puts
the merge behind a menu and a confirmation, offers only the methods the
repository allows, and keeps auto-merge to pull requests that are held back.

## Decision

Merging a pull request, turning auto-merge on or off, closing or reopening it,
and marking it ready or a draft are each its own key in the pull request's
tab, and none writes until the user answers yes to a question that names the
pull request and what will be done. What the tab read of the pull request and
its repository decides first: where GitHub would refuse, or the repository
does not allow the method, the reason is told and nothing is asked. Every
write is made by `gh` on the work host, and a merge is fenced to the commit
the tab showed.

## Alternatives considered

- Merge from the prompt, as comments and reviews are written. Sending the
  prompt does not name what happens, and a merge has no text to write.
- Merge at once from the key. A key pressed by mistake would merge, which
  cannot be undone from Tsumugi.
- Offer every action and let GitHub refuse. The reason would arrive after the
  user had confirmed, and in GitHub's words rather than for what was pressed.

## Consequences

- A merge that GitHub would take is never made without a question, and one it
  would refuse is explained before the question.
- A pull request whose merge state could not be read is not merged until it is
  read again.
- A head that moved since the tab was read makes GitHub refuse the merge, so
  what was reviewed is what lands.
