# The tree

Tsumugi lists every session it runs by the directory it runs in
([details](sessions.md)). What holds them is what git says about the directory
each runs in. Sessions started outside Tsumugi are not listed; their
conversations can be resumed in Tsumugi
([resuming](sessions.md#resuming-a-past-conversation)).

The sidebar's Agents section offers a separate view of session states and the
parents that started them, without unfolding directories
([the agents panel](agents-panel.md)).

## Workspaces and worktrees

A **workspace** is where sessions and the shells you open in Tsumugi run:

- Scratch, holding every session and shell that runs in the scratch folder or
  below it ([scratch sessions](#scratch-sessions));
- a git repository, holding every session and shell that runs in any of its
  worktrees or in a directory below one;
- otherwise the directory a session or a shell runs in.

Inside a git workspace, each worktree where something runs has a row of its
own; a worktree where only shells or Neovim run is listed for them. A
workspace where everything runs in its main checkout shows that checkout's
contents straight under it, with no worktree row; one outside git always does.
A workspace where no session or terminal runs starts folded; once you fold or
unfold it, it stays as you left it.

A worktree's files sit under its Files heading, and what runs in it under
Sessions, Terminals and Neovim: agent sessions, shells and commands, and
Neovim are listed and counted separately. A Neovim started on a file is
represented by that file's row. A terminal whose program reports how it
stands, as a session's does
([how a session stands](sessions.md#how-a-session-stands)), is marked with
that state as a session is, done until it is viewed; one whose program
reports nothing carries no such mark. A heading folds, and the row of one for
what runs counts what it holds and shows how that work stands, folded too.
Workspace and worktree rows summarize the work they hold without opening them.
A row of what is unread, and the rows that hold it, are drawn bold
([unread sessions](sessions.md#unread-sessions)); a workspace or worktree whose
notifications are muted is marked
([muting](notices.md#outside-the-window)).
Where a workspace has no worktree row, the headings are its checkout's.

The browser pages opened in a worktree are listed under its Browser heading
([browsing](content.md#browsing)), each by the site it shows. They are not
counted among what runs.

A workspace with worktree rows lists, above them, its own Sessions,
Terminals and Neovim: every session and terminal of all its worktrees, so
what runs in the repository is seen without opening each worktree. The
worktree rows start folded, each still holding its own Files, Sessions,
Terminals and Neovim. The row of what a window shows is the one under the
workspace's heading, or, while that is folded, the one under its worktree's.

```text
api                        ← a git repository with sessions in two worktrees
├── Sessions               ← every session of every worktree of api
│   ├── auth-revamp
│   ├── review-api
│   └── implement
├── Terminals
│   └── $ dev-server
├── Pull Requests          ← what is open on GitHub, folded until opened
├── Issues
├── main                   ← a worktree, folded until opened
│   ├── Files
│   ├── Sessions
│   │   ├── auth-revamp
│   │   └── review-api
│   └── Terminals
│       └── $ dev-server
└── feat/oauth
    ├── Files
    ├── Sessions
    │   └── implement
    └── Terminals
notes                      ← a directory outside git: no worktree row
├── Files
└── Sessions
    └── outline-the-talk
Scratch                    ← every scratch session, straight under it
└── explain-regex-flags
```

A git workspace with a remote on GitHub lists Pull Requests and Issues
after the rows of what runs in it, while `gh` is installed on its work
machine; without `gh` it lists neither. Both start folded. Unfolded, each
lists what is open on GitHub, as the login
`gh` uses there sees it, each row by its title and who opened it, a pull
request marked with how its checks stand and drawn dim while it is a draft.
What is listed is read again every minute while the heading is open. Each
heading shows how many are open, all GitHub counts and not only those
listed, folded too: that count is read again every few minutes while the
heading is folded, and with its listing while it is open. When
`gh` cannot answer, because it is not logged in or GitHub cannot be reached,
the heading holds one row saying so.

Opening a row shows it in a tab of its own, as GitHub shows its
conversation: its title, how it stands, who opened it and, of a pull
request, the branches it merges; then a card for its body and for every
comment and review that says something, oldest first, each drawn as a
Markdown document is, with the images it shows, those of a private
repository too, and from a work machine too. A pull request's tab shows,
in turn, the files it changes with their diffs and the checks run on its
head, those that failed first. A link in it opens in the browser. The tab takes the viewer's keys, reads it again every minute while
it is shown and whenever the work is read again, and is not kept for the
next run. `g x` on its row or in its tab opens it on GitHub in the browser
instead, from a work machine too.

A pull request opened from the details panel ([details](details.md#changes))
shows in this tab as soon as it exists.

`n` on a row opens a new session on it, its first prompt already naming it
and left to finish and send: an issue's in the repository's checkout, a pull
request's in a new worktree fetched from GitHub: on the pull request's own
branch, tracking the remote's, when it comes from the repository, and at its
head, on a branch named after its number, when it comes from a fork.

Files starts folded. Unfolded, it lists the worktree's files as a
[file tree](#file-trees). Hidden files once shown, and what git ignores, are
drawn dim; version control metadata is not listed. A file Git sees changed is
marked at the end of its row with the letter Git gives it, and a directory
holding one with a dot. The marks and the listing follow the disk within seconds.
Opening a file shows it in a viewer or in Neovim, by its kind
([viewers](content.md#viewers)), and its row shows the tabs it is shown in.
A file a Neovim started on it has open is drawn brighter than the other
files, whether a window shows that Neovim or not.

A new file or directory is named in place, in the directory selected, the
one the file selected is in, or the worktree from its Files heading, and
made there on the work machine ([keys](reference/keys.md#actions)). A name
with `/` makes the directories it passes through too, and cannot reach
outside the directory it is typed in. What is there already is never
overwritten. What is made is selected once it is listed.

A row of a session, a shell or a Neovim shows the numbers of the content's tabs
it is shown in, and the row of what the focused window shows stands out.
Moving the cursor to one of these rows shows its tab while the keys stay in
Tree. A file already open in a tab follows the cursor too; moving through
files does not start an editor.

A session opened by another session is listed by the directory it runs in,
like any other; the session it came from is told in its details
([details](details.md)). There is one tree: no view lists sessions by who
opened them.

Opening a terminal's row shows it where the key pressed says and moves the
keys there, as opening a session's row does ([content](content.md#opening)).
Closing a row's windows from the tree closes every window that shows it, and
leaves it running. Closing its
windows hides it; Stop Terminal
explicitly ends its program ([terminals](content.md#terminals)). Going from session to
session, the waiting sessions and the picker pass shells over.

`!` in the tree shows every worktree git knows of each workspace, those
where nothing runs too, drawn dim; opening one starts a session there.
Pressing it again goes back to the worktrees where sessions or shells run. The
window does not keep this between runs.

A session that has ended is listed no more; its agent keeps its
conversation, which can be resumed in a new session
([resuming](sessions.md#resuming-a-past-conversation)). A workspace stays
listed once anything has run in it, after everything there has ended and
across runs, until it is removed from the tree, from its row's menu or the
close button its row shows under the pointer. Where nothing runs, removing it
asks nothing; where something does, it asks first and then ends every session
and terminal running there. Starting anything in a removed workspace lists it
again. A workspace whose directory is gone is listed no more.

A worktree where something runs is closed the same ways: it asks first, then
ends every session and terminal running there, and its row goes while its
workspace stays listed.

## Writing on a pull request or an issue

The tab of a pull request or an issue shows the prompt, and what is written
there is written on GitHub, as the login `gh` uses on the work machine,
from a work machine too. Sending the prompt posts it as a new comment. Keys of
the tab ([keys](reference/keys.md#actions)) prepare the prompt for more: one
selects a card of the conversation, and the others reply to it, react to it,
resolve the thread of lines it belongs to, write a comment, or write a review
of a pull request, which approves it, asks for changes or only comments. A
reply goes into the card's thread of lines, where it has one, and otherwise is
a comment quoting the card. Comments on lines of the files are drawn among
the conversation, with where they are and whether their thread is resolved;
starting a new one on a line is not offered here. A file of a pull request
has a box to mark it viewed.

Nothing is written on GitHub but by sending the prompt, by the key that
resolves a thread, by that box and by the keys that finish a pull request
([below](#merging-and-closing-a-pull-request)). A write that fails, because the work
machine is offline, `gh` is logged out or GitHub refuses the login, says why
in GitHub's words and leaves the text in the prompt to be sent again.
Leaving the prompt gives a prepared write up, and the text stays. A comment
or review that is posted is never edited or deleted from here, and neither
is anyone else's.

## Merging and closing a pull request

From a pull request's tab, keys ([keys](reference/keys.md#actions)) merge it,
turn GitHub's auto-merge on or off, close or reopen it, and mark a draft ready
or an open one a draft. A merge takes the method the repository prefers, or
squash, a merge commit or rebase as the key says; only the methods the
repository allows are taken.

Nothing is merged, closed or changed without a question that names the pull
request and what will be done, and no answer but yes does it. Where GitHub
would refuse, the reason is said instead of asking: the pull request is
merged, closed or a draft, the login may only read or triage, a review is
required or has asked for changes, there are conflicts, the branch is behind,
required checks have not passed (named, when they failed), GitHub has not yet
worked out whether it can merge, or the method is not allowed. Auto-merge is
offered only while something holds the merge back, and turning it off is
offered wherever it is on. A merge is made only while the pull request still
ends at the commit the tab showed. When what merging depends on could not be
read, nothing is merged until the tab is read again.

The repository's merge queue is not looked for: auto-merge is asked of
`gh pr merge --auto`, which GitHub applies to a queue itself. The keys act on
the tab shown; a pull request's row in the tree has none of them.

## Reading a check's log

A check of a pull request that failed or still runs, run by GitHub Actions,
opens its job's log in a window of the content when it is clicked in the
details panel; any other check opens where it tells more, as before. The
window shows the end of the log, the same excerpt a request to fix the pull
request quotes ([below](#asking-an-agent-to-fix-a-pull-request)), scrolled to
the first error in it, and says so when earlier lines were left out. It is
read with the viewer's keys, reads again with `refresh`, and `viewer.copy_path`
copies the log shown.

A log that cannot be read says why where the log would be: the check has not
started or is still running, so it has no final log; the repository has no
remote on GitHub; the login may not read it; or the network failed. Reading
it again once the run is over shows it.

`check.rerun` ([keys](reference/keys.md#actions)) runs the failed jobs of the
check's Actions run again, and the jobs that need them, not the ones that
passed. It asks first, naming the pull request, like the writes above, and
writes nothing on any answer but yes. A check whose address does not name its
run says so instead of asking. The window reads its log again once it is done.

## Asking an agent to fix a pull request

From a pull request's tab, the keys of `github.fix_checks` and
`github.fix_threads` ([keys](reference/keys.md#actions)) prepare a request for
an agent to fix what its failed checks or its unresolved review threads say.
For threads, the request is for the thread of the card selected, or for every
unresolved one with no card selected.

The request is written in the draft of a new session on the pull request, in
the worktree that has its branch checked out where there is one, otherwise in
a worktree made for it as `n` on its row does. It says the environment, the
repository, the branch and its base, and the worktree; gives the pull request's
title and link; and, for checks, each failed one with its name, link and the
end of its log, those still running by name, and for threads each one's file,
line, remarks and whether GitHub marks it outdated. The title, names, logs and
remarks are given as data to be treated as untrusted, never as instructions.

The work host reads the pull request's head again for it. When the head is no
longer the one the tab showed, the request says so, so the agent looks at the
code as it is. A log that cannot be read, because the check was not run by
GitHub Actions, the login may not read it, or the network failed, is said so
in its place and the request still names the check and links it. Only the
first few failed checks have their logs read, the rest say so, and a log is
cut to its end with the earlier errors kept. When nothing failed, or the head
cannot be read, a notice says so and nothing is prepared.

Preparing a request sends nothing to the agent: you edit the prompt and send it
as any other. Neither preparing nor sending replies on GitHub, resolves a
thread, pushes or merges; those stay your own acts.

## Making and deleting worktrees

A worktree is made from the tree, by the `+` of a workspace or a worktree, its
menu or its key ([keys](reference/keys.md#actions)). A dialog asks for its
name, where it starts from and, if wanted, the branch it works on. It starts
from the repository's checkout, a local branch, a branch of a remote, which is
fetched first so it is as the remote has it, or any revision typed, a commit
for one. The new branch is named after the worktree's directory unless given
one; a worktree made from a remote's branch does not push to it.

Once the dialog is answered it closes and the worktree is made in the
background: a row under its workspace says which step it is at, and a session's
draft opens in the worktree when it is made. A worktree that cannot be made, for
a branch or a directory that is taken, a name git refuses, a start that is no
revision or a remote that cannot be fetched, leaves nothing behind and its row
stays with the reason until it is put away. Work already in the repository is
never touched. This works for a repository on a work machine as for one on this
computer, and for a directory outside git it says it has no worktrees. A new
worktree is also given the ignored files its repository lists and has the
repository's setup script run in it as you allow, before the session's draft
opens ([worktree setup](worktree-setup.md)).

A worktree is deleted from its row, which asks first. The question says what
deleting loses: the files changed and not committed, the commits pushed nowhere,
and what runs in the worktree, which ends first. A worktree with changes is
deleted only when the question is answered to discard them. The main checkout and
a locked worktree are never deleted. Its branch goes with it when its commits are
all merged or pushed; otherwise it is kept, and the question offers to delete it
too, saying how many commits that loses. A branch git refuses to delete is left,
and the status line says so. When deleting fails, nothing is lost.

## Saying how a worktree's work stands

A worktree can say how its work stands by one line of text and a status: to
do, in progress, in review or completed. The line is shown beside the
worktree's name, after the status as a chip. The agent working in a worktree,
or you, sets them with `tsumugi worktree set`, which takes `--comment <text>`
and `--status <status>`, or `--clear-comment` and `--clear-status` to empty
one, for the worktree of `--dir` or else of the directory it runs in;
`tsumugi worktree show` reads the line. An agent sets them with its
`set_worktree_checkpoint` tool too, which reads what the worktree says when it
is given nothing to change. You set the status from the worktree's right-click
menu. What is not named is left as it was. A comment is cut to its first line
and 200 characters. While a worktree has no comment, its line is the latest
note an agent working there wrote on its timeline. What a worktree says is kept
on the work machine with the worktree, not with a session, so it stays when the
session that set it ends, and every window shows it.

The tree's filter takes `status:<status>`, as `status:in-review`, to keep the
worktrees in that status with all they hold; the words left narrow by name as
usual, and several `status:` words keep the worktrees in any of them. Clicking
a worktree's status chip adds its word to the filter, and clicking it again
takes the word away.

## A branch against its base

A worktree's row, and a workspace's row standing for its main checkout, says
how its branch stands against its base ([the base](details.md#branch-and-conflicts)):
how many commits it is ahead and behind, and the lines it added and took out
since the two parted, what is only edited in the worktree included. A branch
that cannot be compared shows nothing there. The numbers follow the disk and
the base's last fetch.

From the row, or its right-click menu ([keys](reference/keys.md#actions)):

- the branch's change opens in a diff tab: the worktree against its base, or
  only what the branch committed ([diff tabs](details.md#diff-tabs));
- two revisions of the repository, named in the prompt, open in a diff tab
  against each other;
- a draft in the worktree asks an agent to review the branch's change, to
  explain it, or to leave notes on it for your review
  ([review](review.md#notes-from-agents)); sending the draft starts the
  session, as with the [AI actions](ai-actions.md).

## Merging a worktree into main

A finished worktree's branch is merged into the branch the main checkout has
checked out from the worktree's row, which asks first. The merge is a merge
commit of its own, and the question says how many commits it brings. Nothing
of the main checkout is ever lost to it:

- A merge that would conflict is never begun in the main checkout, which stays
  as it was. The question names the files instead and offers to bring the main
  checkout's branch into the worktree's branch, leaving the conflicts there to
  resolve, continue or abort like any merge in progress
  ([branch and conflicts](details.md#branch-and-conflicts)), and to start a
  session in the worktree that resolves them.
- Uncommitted changes in the main checkout are never stashed, committed or
  overwritten unless the question says so. It says how many files they are and
  by default sets them aside in a stash, which is kept and never popped; its
  choice commits them on the branch instead. Nothing merges without that
  answer.
- A merge Git fails to finish is aborted, so the main checkout is as it was.
- A detached head, a branch the main checkout already holds and an unfinished
  merge, rebase, cherry-pick or revert in the main checkout are told, and
  nothing changes. The worktree's own uncommitted changes are not part of the
  merge, and the question says so.

The question names the [local issue](local-issues.md) the worktree's session
was started for, which the merge closes. Once the branch is merged, the
worktree is offered for deleting as above, with the loss told, its branch going
with it.

The same merge is made from the command line and by agents; see
[merging a finished branch](leader-mode.md#integrating-a-finished-branch).

## Cleaning up worktrees

Clean Up Worktrees ([keys](reference/keys.md#actions)), from a workspace's or a
worktree's row or its menu, lists the repository's worktrees, the main checkout
left out, each with what to weigh before deleting it: its branch, the files
changed and commits not pushed, whether the branch is merged into the base
branch, the state of its pull request, the disk it takes and the age of its
last commit. Worktrees whose pull request GitHub says is merged or closed are
chosen beforehand, when `gh` can say; one with changes or commits not pushed,
and a locked one, never is. A locked worktree cannot be chosen at all.

The chosen worktrees are deleted together. A worktree that loses nothing, with
nothing running in it, is deleted without a question; as soon as one of them
would lose changes, commits not pushed, a detached head's commits or running
sessions and terminals, one question names each of them and what it loses, and
nothing is deleted until it is answered yes. A branch whose commits nothing
else has is kept, as deleting a worktree keeps it. What the list says of
GitHub is left out where it cannot be asked, and nothing is chosen beforehand.

## Putting a worktree to sleep

Sleep, from a worktree's row or its menu ([keys](reference/keys.md#actions)),
puts the worktree to sleep: the programs of its sessions are stopped and its
terminals end, while its files, its branch and its sessions' rows and tabs stay.
The row is marked as asleep while every session in it sleeps. Wake brings its
sleeping sessions back with their conversations, as focusing one does
([hibernation](sessions.md#hibernation)); a session that sleeps wakes the same
way whichever way it was put to sleep.

Nothing is asked where the sessions are done and no terminal runs. A session
in the middle of a turn, or a terminal, is stopped only after a question that
says so. A session that cannot be woken into its conversation again is not put
to sleep, and the status line names it.

## Arranging the tree

A workspace or a worktree is pinned from its row or its menu
([keys](reference/keys.md#actions)): a pinned workspace stands above the
others, scratch excepted, and a pinned worktree above the others of its
workspace. The rest of a workspace's worktrees keep the order the work host
lists them in, or are put in the order of their last activity, which is when a
session in them was last written to or started, or of their names. Hide
filters leave worktrees out of the tree: those that sleep, those where nothing
works, waits or starts, and the main checkout of a workspace that lists other
worktrees. A worktree left out still has its sessions and terminals listed
under its workspace, and a pinned one is never left out. The pins, the order
and the filters are kept for the next run of the window.

Reveal Current selects the row of the session or terminal the focused window
shows, unfolding the way to it, and shows its worktree again where a filter
left it out.

The filter at the top of the tree narrows it as it is typed into, by the
fuzzy match the pickers use, fzf's syntax included, on the names of
sessions, terminals, worktrees and workspaces; a worktree or a workspace that
matches keeps all it holds, and the rows keep the tree's order. The sessions
waiting on you are listed again at the bottom of the tree, apart from it.

The tree heads each [environment](environments.md)'s work with a row named
after it, which folds like a heading, one for every environment the window
shows, however many there are. The same directory in two environments is two
rows. Each heading says in words when its environment is being reached,
cannot be reached, with how old its work is, reads only, or holds a change
sent there that needs you. The heading also summarizes its sessions, terminals and
Neovims and how they stand, whether folded or open, across all its work even
while the tree is filtered.
The rows of every environment but the one of the selected row, or else of what
is shown, are drawn dimmer, all but the sessions waiting on you.
Selecting a heading shows its environment in the details: how its connection
stands, what went wrong, a way to try again at once or to pair with its work
machine again, the address and key of its work machine, its agents, the
drafts and changes waiting for it, and a way to its settings. The
sessions waiting on you are gathered from every environment, and going to the
next waiting session reaches each of them. A session, a shell or a Neovim
started from a row, or beside a tab, starts in the environment of that row or
tab. From a heading, a new session opens the picker on that environment, and a
shell or a Neovim starts in its host's home directory, once a host on a work
machine has said where that is. The `+` on a heading offers what starts
work in its environment: a new session, a shell, a Neovim, and Add to Tree
for any directory. A heading's menu also opens the environment's
settings, renames it in place, tries its work machine again at once while the
window waits to, pairs with its work machine again, and removes it.

## Going to a session or starting one

`Mod+p`, `w`, `+` beside the tree's filter, or the Session menu opens the
**picker**. Its first row starts a [scratch session](#scratch-sessions). It
lists every session next, each with where it runs and how it stands, and then
the directories to start a new session in:

- the directories the tree lists;
- the directories Claude Code has been used in, the most recent first;
- the repositories `ghq list` names, when ghq is installed and the settings
  do not turn it off;
- the folders in each folder the settings list for the picker
  ([settings](settings.md#settingstoml)).

A session stands as its row in the tree shows it, a finished one the user has
seen as idle. With nothing typed, the sessions are listed by how much they
need the user, those waiting on the user first and those finished unseen
next, and of those that stand alike the one the window last saw change comes
first. The tree keeps its own order.

Typing narrows both by a fuzzy match, fzf's syntax included: a session on its
name and where it runs, a directory on its path. The sessions stay above the
directories however well either answers, and the scratch row stays on top
while what is typed matches its words. A word `@` followed by a state, or the
start of one, keeps only the sessions in that state, `@w` those waiting on
the user, and offers nothing else. While the rows move as sessions change, the
cursor stays on the row it is on. The keys [over the window](keys.md#over-the-window)
move, choose and close the picker.

Choosing a session opens it as its row in the tree would, leaving the keys
where they were. Choosing a directory opens a new session there: the first
prompt sent starts it, named from a first line `name: <label>`, or else
from its first words, a path or a URL among them by what it names. A prompt
with words in another script, Japanese for one, is named by its own letters
as far as a row of the tree holds them, cut where a word likely ends. A name
made from the prompt stands until the session's agent
[names the session](sessions.md#renaming-a-session).

A window showing more than one environment offers the sessions and the
directories of each, every row naming its environment, and a new session
starts in the environment its directory was chosen in. Its file tree and its
scratch row are of the environment of the selected row or the tab shown.

`Ctrl+f` turns the picker into a [file tree](#file-trees) for a directory the
list does not offer. It shows the directories under the home directory of the work machine it
browses, under each ghq root and under each folder the settings list, side by side, and
choosing one with the filer's open key starts a session there. `Ctrl+f` goes back to the list, and
the filer's keys come before those over the window.

Instead of choosing a row, in the list or the file tree, `Ctrl+s` opens a
shell and `Ctrl+e` Neovim in its directory: a directory's own, the worktree a
session runs in, the scratch folder for the scratch row. `Ctrl+t` adds the
directory's workspace to the tree with nothing started there.

Typing a repository ghq can clone — `host/owner/repo` or a URL git clones
from — offers a row to clone it after
the directories, which typing a filter and `Enter` therefore never chooses
over a directory that matches, in each environment where ghq is offered as
above and does not keep that repository already. Choosing it runs `ghq get`
on that work machine in a terminal shown with the keys, so whatever the clone asks, a
passphrase or a host to trust, is answered there. Once the clone succeeds,
the repository's workspace is added to the tree, and its directory is taken
as if it had been chosen from the list,
by `Enter`, `Ctrl+s`, `Ctrl+e` or `Ctrl+t` as the clone row was; once it
fails, the terminal stays with what it wrote.

Typing `github:` looks through GitHub instead, and the list offers nothing
else. Where ghq is offered, a row says so while nothing, or the start of
`github`, is typed, and choosing it types `github:`. After `github:`, the list offers the owners
`gh` is logged in as on that work machine, the user and their
organizations, narrowed by what is typed; choosing one types it and a
slash. After `github:owner/`, it offers a clone row for each of that
owner's repositories that ghq does not keep, as `gh` lists them, the
private ones its login can see included, narrowed by what is typed after
the slash. Where `gh` is not installed or not logged in, no owner is
offered, and `github:owner/repo` typed in full offers that repository
alone.

Add to Tree, in the Session menu, on an environment's heading or `A` in the
tree, opens the picker on the directories alone, of the environment it was
opened in: choosing one, from the list or the file tree, adds its workspace
to the tree, and `Ctrl+s` and `Ctrl+e` still open a shell and Neovim there.

The `+` of an environment, workspace or worktree also opens
[Local Issues](local-issues.md), where requests can be queued for the
environment's default agent. The working directory is filled from the row
where it was opened and can be changed before queuing the request.

`n` on a row starts a session in that row's worktree or directory without the
picker. `Ctrl+q }` and `Ctrl+q {` open a session of the next and the
previous workspace without asking, the one last shown there.

## Searching more than sessions and directories

The picker searches in **scopes**: each is a mix of sources, and the rows of
every source of the scope are listed in the order the scope names them, each
group under its heading. The picker opens on the first scope, which goes to a
session or starts one as above. The sources are:

- **sessions**, with the scratch row, and **directories**, with the rows
  that clone, as above;
- **worktrees**, every worktree the tree lists, by its branch and workspace.
  A name no worktree has is offered last as **Create worktree**, which opens
  the dialog for a new worktree ([worktrees](#making-and-deleting-worktrees))
  of the project shown with that name filled in; the matches stay first, so
  `Enter` still goes to a worktree that exists. Going to one selects its row
  in the tree and leaves the keys where they were;
- **resources**, what projects keep ([resources](resources.md)), those of the
  project shown first; choosing one opens it as the details panel's row
  does;
- **files**, those git tracks or would track in the worktree shown, ranked by
  their path; choosing one opens it as a row of the file tree does. They are
  read only while a scope that searches them is in use;
- **actions**, everything the keys and menus run
  ([keys](reference/keys.md#actions)), by the name the menus give it or the
  name bindings use; choosing one closes the picker and runs it as its key
  would.

`Ctrl+g` steps to the next scope and `Ctrl+Shift+g` to the previous, going
round, and the scopes are listed above the rows, where a click goes to one.
What is typed stays. A scope with a prefix is entered by typing the prefix
first, and what follows is what is searched for, as `>` for the actions: the
prefix wins over the scope stepped to. A prefix of letters stands alone as a
word. The picker starts with scopes for going to a session or a directory,
for the actions, the files, the resources and the worktrees each alone, and
for the last three together; the settings replace them with any mix and any
prefixes ([settings](reference/settings.md#picker)). Adding to the tree
offers directories alone, whatever the scopes say.

## Scratch sessions

A **scratch session** is for talking to Claude about something no project
holds, with no directory to choose. The picker's first row or New
Scratch Session in the Session menu opens one, with the prompt ready as for a
new session, and so does a prompt sent with nothing open
([prompt](prompt.md)). Every scratch session runs in the scratch folder: the
one the settings name, or `$XDG_DATA_HOME/tsumugi/scratch`
(`~/.local/share/tsumugi/scratch` when `$XDG_DATA_HOME` is unset), made when
the first one starts.

The tree lists everything that runs in the scratch folder or below it as one
workspace, Scratch, after every other, its sessions and shells straight under
it. Starting a session from it starts another scratch session; a shell opened
from it runs in the scratch folder.

A scratch session keeps to talk that belongs to no project. When the talk
turns to work on a project, it opens a session in that project's directory,
handing it what it needs, and asks that it be shown
([sessions that open sessions](opening-sessions.md)): while you are looking at
the scratch session, the new one opens in its place with the keys, and the
work carries on there.

Claude Code asks whether to trust a folder before a session first works in
it: the first scratch session shows the question in its console until it is
answered there, and none asks again.

## File trees

The tree's files and the picker's directories are one file tree, drawn and
moved through alike, with the `filer` area's keys
([keys](reference/keys.md#actions)). Directories come first, each entry with
its Nerd Font icon in the colour vim-glyph-palette gives it, drawn from
Tsumugi's own Nerd Font whatever the console's font is. A directory is read
when it is opened. Going into a directory opens it and goes to its first
entry once it is read; going out closes the directory selected when it is
open, or else the one it is in, and goes there. Names with a leading dot are
listed only while the hidden are shown, and are drawn dim.

In the tree, going out of any row closes the one it sits under, and going
into a heading or a workspace goes to its first row, as in a directory.
