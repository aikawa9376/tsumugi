# Key reference

<!-- Written by `cargo xtask references` from the code; edit the code, not this file. -->

Every action a binding can name, what it does, what the menu bar calls it,
and the keys Tsumugi binds it to in each area before a file of bindings
changes them ([keys](../keys.md)).

## Actions

| Action | Menu bar | Does | Default keys |
| --- | --- | --- | --- |
| `none` |  | nothing; bound to keys, it takes them away |  |
| `window.left` | Left Window | move to the window to the left: the tree lies left of the content's windows | `global`: `Ctrl+q h`, `Ctrl+q ArrowLeft` |
| `window.down` | Window Below | move to the window below: a session's prompt lies below its console | `global`: `Ctrl+q j`, `Ctrl+q ArrowDown` |
| `window.up` | Window Above | move to the window above: a session's console lies above its prompt | `global`: `Ctrl+q k`, `Ctrl+q ArrowUp` |
| `window.right` | Right Window | move to the window to the right: the details panel lies right of the content's windows | `global`: `Ctrl+q l`, `Ctrl+q ArrowRight` |
| `window.next` | Next Window | move to the window after the one the keys are in, from the tree through the content's windows, each console before its prompt, to the details panel, going round at the end | `global`: `Ctrl+q q`<br>`tree`: `] ]`<br>`agents`: `] ]`<br>`details`: `] ]`<br>`console`: `] ]`<br>`viewer`: `] ]`<br>`board`: `] ]` |
| `window.prev` | Previous Window | move to the window before the one the keys are in, in the order `window.next` goes, going round at the start | `global`: `Ctrl+q Q`<br>`tree`: `[ [`<br>`agents`: `[ [`<br>`details`: `[ [`<br>`console`: `[ [`<br>`viewer`: `[ [`<br>`board`: `[ [` |
| `window.top` | First Window | move to the first window, the tree when shown | `global`: `Ctrl+q t` |
| `window.bottom` | Last Window | move to the last window, the details panel when shown | `global`: `Ctrl+q b` |
| `window.back` | Back to Window Before | move back to the window the keys were in before | `global`: `Ctrl+q p` |
| `window.split` | Split One Above the Other | split the focused window one above the other, the new window, below it, showing the same ([content](../content.md)) | `global`: `Ctrl+q s` |
| `window.vsplit` | Split Side by Side | split the focused window side by side, the new window, right of it, showing the same ([content](../content.md)) | `global`: `Ctrl+q v` |
| `window.new` | Split for a New Session | split the focused window one above the other and start writing a new session in the new one, in the worktree the focused window shows | `global`: `Ctrl+q n` |
| `window.quit` | Close Split | close the focused window, and its tab when it was the last; what it showed runs on | `global`: `Ctrl+q c`<br>`viewer`: `q`<br>`board`: `q` |
| `window.only` | Close Other Splits | close every other window of the tab | `global`: `Ctrl+q o` |
| `window.move_left` | Move Window to the Left | move the focused window to the left of its tab, as tall as the tab | `global`: `Ctrl+q H` |
| `window.move_down` | Move Window to the Bottom | move the focused window to the bottom of its tab, as wide as the tab | `global`: `Ctrl+q J` |
| `window.move_up` | Move Window to the Top | move the focused window to the top of its tab, as wide as the tab | `global`: `Ctrl+q K` |
| `window.move_right` | Move Window to the Right | move the focused window to the right of its tab, as tall as the tab | `global`: `Ctrl+q L` |
| `window.exchange` | Exchange Windows | exchange the focused window with the next one in its split, or the one before the last | `global`: `Ctrl+q x` |
| `window.rotate` | Rotate Windows | rotate the windows of the focused window's split forward, the last becoming the first | `global`: `Ctrl+q r` |
| `window.equalize` | Equal Sizes | give the windows of every split the same size | `global`: `Ctrl+q =` |
| `window.narrower` | Narrower | make the focused window a step narrower; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q <` |
| `window.wider` | Wider | make the focused window a step wider; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q >` |
| `window.taller` | Taller | make the focused window a step taller; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q Plus` |
| `window.shorter` | Shorter | make the focused window a step shorter; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q -` |
| `window.maximize_height` | Tallest | make the focused window as tall as the others allow | `global`: `Ctrl+q _` |
| `window.maximize_width` | Widest | make the focused window as wide as the others allow | `global`: `Ctrl+q \|` |
| `window.to_tab` | Move Window to a New Tab | move the focused window to a new tab, unless it is the only one of its tab | `global`: `Ctrl+q T` |
| `window.edit` |  | switch the focused window between a document's viewer and a Neovim on the document, which runs on hidden while the window shows the viewer ([content](../content.md#viewers)) | `global`: `Ctrl+q e`<br>`viewer`: `e` |
| `tab.merge_last` |  | merge the content's tab shown into the tab shown before it, or else the tab beside it: every window of it, as it is split, right of every window there, the focused one keeping the focus ([content](../content.md)) | `global`: `Ctrl+q M`, `Ctrl+q m m` |
| `tab.next` | Next Tab | show the next tab of the area the keys are in: the details panel's tabs there, elsewhere the content's tabs; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+PageDown`, `Cmd+}`, `Ctrl+q g t`<br>`details`: `Tab` |
| `tab.prev` | Previous Tab | show the previous tab of the area the keys are in: the details panel's tabs there, elsewhere the content's tabs; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+PageUp`, `Cmd+{`, `Ctrl+q g T`<br>`details`: `Shift+Tab` |
| `tab.last` | Tab Before | show the content's tab shown before the one shown now | `global`: `Ctrl+q g Tab` |
| `tab.move_prev` |  | move the content's tab shown one place toward the first; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q g <`, `Ctrl+Shift+PageUp` |
| `tab.move_next` |  | move the content's tab shown one place toward the last; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q g >`, `Ctrl+Shift+PageDown` |
| `tab.recent` |  | open the tab switcher on the content's tabs, by when each was shown last or else in their order, and move to the next in the list; pressed again while it is open, move on; the tab moved to is shown once the modifier keys are let go, or on Enter ([content](../content.md#tabs)) | `global`: `Ctrl+Tab` |
| `tab.recent_prev` |  | open the tab switcher as `tab.recent` does, and move to the previous in the list instead of the next | `global`: `Ctrl+Shift+Tab` |
| `tab.next_same` |  | show the next tab of the content that shows the same kind of thing as the tab shown does, going round at the end: sessions, terminals, files, browser pages or the rest | `global`: `Mod+Alt+]` |
| `tab.prev_same` |  | show the previous tab of the content that shows the same kind of thing as the tab shown does, going round at the start | `global`: `Mod+Alt+[` |
| `tab.close_files` |  | close every window of the content that shows a file in a viewer, and the tabs left empty; what else is shown stays | `global`: `Mod+Alt+w` |
| `cursor.down` |  | move the selection in the tree to the next row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `j`, `ArrowDown`<br>`agents`: `j`, `ArrowDown`<br>`details`: `j`, `ArrowDown` |
| `cursor.up` |  | move the selection in the tree to the previous row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `k`, `ArrowUp`<br>`agents`: `k`, `ArrowUp`<br>`details`: `k`, `ArrowUp` |
| `cursor.first` |  | move the selection in the tree to the first row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `g g`<br>`agents`: `g g`<br>`details`: `g g` |
| `cursor.last` |  | move the selection in the tree to the last row; in the details panel, in the list it shows, or scroll what else it shows | `tree`: `G`<br>`agents`: `G`<br>`details`: `G` |
| `cursor.go` |  | open the selected session, shell, worktree or file: go to a window showing it, in any tab, or else open it in a new tab after the one shown, and move to it, or to the prompt of a session not started yet; on a folded row, a worktree or a session holding others too, unfold it instead; fold or unfold a workspace, a heading or a directory; in the details panel, open the selected row, an image of Resources as a file is opened, the keys staying in the panel | `tree`: `Enter`, `o`, `l`, `L`, `Shift+Enter`, `Ctrl+Enter`<br>`agents`: `Enter`, `o` |
| `cursor.split` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, below it; in the details panel, the selected image of Resources | `tree`: `s`<br>`agents`: `s` |
| `cursor.vsplit` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new window split off the focused one, beside it; in the details panel, the selected image of Resources | `tree`: `v`<br>`agents`: `v` |
| `cursor.tab` |  | open the selected session, shell, worktree or file as `cursor.go` does, in a new tab after the one shown, though a window shows it already; in the details panel, the selected image of Resources | `tree`: `t`<br>`agents`: `t` |
| `cursor.fold` |  | fold the selected node when it is open, or else the row it sits under, going there; in the details panel, go back out to the list what it shows was opened from | `tree`: `h`<br>`details`: `Escape`, `Backspace` |
| `cursor.close` |  | close every window showing the selected session, shell or Neovim, or the selected file in its viewer or its Neovim; what they showed runs on | `tree`: `x` |
| `cursor.copy` |  | copy the path of the selected file, worktree or workspace, or the name of the selected session, shell or Neovim; in the details panel, the selected link of the overview: its address, or its file's path | `tree`: `y`<br>`agents`: `y`<br>`details`: `y` |
| `cursor.reveal` |  | in the details panel, show the selected file of the overview in the Finder |  |
| `cursor.browse` |  | open the selected pull request or issue on GitHub in the browser | `tree`: `g x`<br>`viewer`: `g x` |
| `filer.down` |  | move to the next file or directory, in the tree's files or the picker's directories | `filer`: `j`, `ArrowDown` |
| `filer.up` |  | move to the previous file or directory | `filer`: `k`, `ArrowUp` |
| `filer.into` |  | open the directory selected and go to its first entry once it is read; on a file of the tree, open it as `cursor.go` does | `filer`: `l`, `ArrowRight` |
| `filer.out` |  | close the directory selected when it is open, or else the one it is in, going there | `filer`: `h`, `ArrowLeft` |
| `filer.open` |  | in the tree, open the file or fold the directory selected as `cursor.go` does; in the picker, start a session in the directory selected | `filer`: `Enter` |
| `filer.hidden` |  | show or hide the files and directories whose names start with `.` | `filer`: `!` |
| `file.new` |  | in the tree's files, type the name of a file to make in the directory selected, in the one the file selected is in, or at the top of the worktree from its Files heading; a name with `/` makes the directories it passes through too, one ending in `/` makes a directory, and one already there is left as it is | `tree`: `N` |
| `directory.new` |  | type the name of a directory to make, where `file.new` makes a file | `tree`: `K` |
| `waiting.next` | Next Needing You | move the tree's cursor to the next session going round every session, those waiting on you first, then those finished unseen, then the rest in the tree's order, unfolding the way to it and showing it as moving the cursor does, the keys in the tree ([tree](../tree.md#going-to-a-session-or-starting-one)); repeats ([keys](../keys.md#repeating)) | `global`: `Mod+j` |
| `waiting.prev` | Previous Needing You | move the tree's cursor to the previous session going round every session as `waiting.next` does, the keys in the tree; repeats ([keys](../keys.md#repeating)) | `global`: `Mod+k` |
| `session.next` | Next Session | open the session after the one shown, in the tree's order over every session, folded away or filtered out too, going round at the end, as `cursor.go` does; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) |  |
| `session.prev` | Previous Session | open the session before the one shown, in the tree's order over every session, folded away or filtered out too, going round at the start, as `cursor.go` does; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) |  |
| `workspace.next` | Next Workspace | open a session of the workspace after the one shown, in the tree's order over every workspace holding a session, folded away or filtered out too, going round at the end: the session last shown there since the window opened, or else its first, as `cursor.go` does; the keys stay in the area they are in; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q }`<br>`tree`: `}` |
| `workspace.prev` | Previous Workspace | open a session of the workspace before the one shown, as `workspace.next` does, going round at the start; repeats ([keys](../keys.md#repeating)) | `global`: `Ctrl+q {`<br>`tree`: `{` |
| `session.pick` | Go to or Start Session… | open the picker, to go to a session or choose where a new one works ([details](../tree.md#going-to-a-session-or-starting-one)) | `global`: `Mod+p`<br>`tree`: `w`<br>`agents`: `w` |
| `session.new` | New Session | start a session, with the prompt ready to write its first message: in the selected row's worktree or directory from the tree, in the one the focused window shows from elsewhere | `global`: `Mod+t`<br>`tree`: `n`<br>`agents`: `n` |
| `session.scratch` | New Scratch Session | start a scratch session, with the prompt ready: one that runs in the scratch folder, for talking about something no project holds ([details](../tree.md#scratch-sessions)) |  |
| `session.resume` | Resume Conversation… | list the conversations Claude Code and Codex kept, to resume one in a new session ([details](../sessions.md#resuming-a-past-conversation)) | `global`: `Mod+o`<br>`tree`: `O` |
| `notices.open` | Notices… | open the list of what needed you, the sessions that began waiting on you, failed or finished a turn, to go through them, mark them read or unread and go to their sessions ([details](../notices.md)) | `global`: `Ctrl+q i` |
| `session.bring_back` | Bring Back Sessions… | list what the terminal host took with it when it went, to bring a session or a shell back ([details](../sessions.md#when-the-terminal-host-goes)) |  |
| `session.restart` |  | start a session whose program exited again: the same agent in the same directory, continuing its conversation; the session keeps its row and its tabs ([details](../sessions.md#when-a-program-exits)) | `tree`: `Ctrl+r`<br>`agents`: `Ctrl+r`<br>`console`: `Ctrl+r` |
| `session.stop` | End… | end a session Tsumugi runs after asking; Claude Code keeps its conversation, for `/resume` in a session started later; on a terminal's row, end that terminal | `global`: `Mod+Backspace`<br>`tree`: `X`<br>`agents`: `X`<br>`console`: `X` |
| `session.rename` | Rename… | edit the name of a session Tsumugi runs in place; the tree shows it, and the session is sent Claude Code's `/rename` so its own interface says it too | `tree`: `R`<br>`agents`: `R` |
| `session.mark_unread` |  | mark a session unread, as if it had finished out of view: its row is drawn bold and counted in the window's title until a window in front shows it anew ([details](../sessions.md#unread-sessions)) | `tree`: `U`<br>`agents`: `U` |
| `notifications.mute` |  | silence the notifications outside the window of the workspace or worktree of the selected row, or tell them again; its rows keep showing how its sessions stand ([details](../notices.md#outside-the-window)) | `tree`: `M` |
| `session.profile_next` | Next Profile of New Session | while a session not started yet is shown, switch the profile it is to start from to the next one ([details](../sessions.md#agents)) | `draft`: `Ctrl+g p` |
| `session.profile_prev` | Previous Profile of New Session | while a session not started yet is shown, switch the profile it is to start from to the previous one ([details](../sessions.md#agents)) | `draft`: `Ctrl+g P` |
| `shell.open` | Open Shell | open a shell in the worktree | `global`: `Mod+s`<br>`tree`: `$` |
| `terminal.stop` | Stop Terminal | end the terminal selected in the tree, or else the one shown |  |
| `neovim.open` | Open Neovim | open Neovim, with your own configuration, in the worktree as `shell.open` opens a shell ([shells](../content.md#terminals)) | `global`: `Mod+e`<br>`tree`: `e` |
| `browser.open` | Open Browser | open a new page of the environment's browser, listed under the worktree it is opened in |  |
| `browser.find` |  | search the browser page shown for text, in a bar under its toolbar: Enter shows the next match, Shift+Enter the previous and Escape closes the bar ([details](../content.md#browsing)) |  |
| `browser.reopen` |  | open again the browser page closed last in the worktree the keys are in, at the address it was closed at ([details](../content.md#browsing)) | `global`: `Cmd+T`, `Mod+Alt+t` |
| `tab.close` | Close Tab | close the content's tab shown and every window in it; agents and terminals keep running and remain reachable from the tree | `global`: `Mod+w` |
| `environment.add` | Add Environment… | add an environment, on this computer or on a work machine paired then, which every window lists at once ([details](../environments.md)) |  |
| `environment.remove` |  | remove the environment selected in the tree from every window after asking, unless the window was started in it; its sessions keep running on its host ([details](../environments.md)) |  |
| `environment.rename` |  | edit the name of the environment selected in the tree in place, on its heading; every window calls it by the name given ([details](../environments.md)) |  |
| `environment.retry` |  | try at once to reach again the work machine of the environment selected in the tree, or else of the tab shown, while the window waits to try it ([details](../environments.md)) |  |
| `environment.pair` |  | pair again with the work machine of the environment selected in the tree, or else of the tab shown, as when it no longer accepts this computer or answers at another address ([details](../environments.md#create-and-configure)) |  |
| `environment.editor` |  | open the editor on the work machine of the environment selected in the tree, or else of the tab shown, when the environment was made by `tsumugi connect ssh`: VS Code, VS Code Insiders or Cursor through Remote-SSH, or Zed by its ssh address, whichever `$TSUMUGI_REMOTE_EDITOR` names, VS Code when it names none ([details](../remote-execution.md#through-ssh)) |  |
| `workspace.add` | Add to Tree… | open the picker on the directories alone, to add the one chosen to the tree with nothing running there ([details](../tree.md#going-to-a-session-or-starting-one)) | `tree`: `A` |
| `local_issues.open` | Local Issues… | open local issues for this environment and working directory, to queue work for its default agent ([details](../local-issues.md)) |  |
| `local_issues.leader` |  | hand the open local issues of this environment to a leader session that carries them through itself within the discretion you set, after confirming the issues, its workers, their models and what it may decide ([details](../leader-mode.md)) |  |
| `saved_commands.open` | Saved Commands… | open the commands the project keeps, to search them, run one in the selected worktree after confirming it, and add, edit or delete one ([details](../saved-commands.md)) | `tree`: `c` |
| `resource.add` | Add Resource… | open the sheet that keeps a path or an address, with a label and a note, in the project of what the details panel shows, to open from its Resources tab ([details](../resources.md#keeping-and-opening-them)) | `details`: `a` |
| `resource.edit` |  | open the sheet on the pinned resource the details panel's cursor is on, to change its label and note ([details](../resources.md#keeping-and-opening-them)) | `details`: `e` |
| `resource.remove` |  | stop keeping the pinned resource the details panel's cursor is on, which leaves what it points at as it is ([details](../resources.md#keeping-and-opening-them)) | `details`: `D` |
| `resource.keep` |  | keep the address, file or image the details panel's cursor is on in the project's resources, unless it keeps it already ([details](../resources.md#keeping-and-opening-them)) | `details`: `p` |
| `resource.capture` |  | capture the screen now, Tsumugi's window and other applications alike, then keep the part of it dragged over among the project's resources ([details](../resources.md#keeping-and-opening-them)) |  |
| `resource.capture_later` |  | capture the screen as `resource.capture` does, once the seconds of the `details.capture_delay_seconds` setting have gone by ([details](../resources.md#keeping-and-opening-them)) |  |
| `workspace.remove` |  | take the workspace selected in the tree out of it, which lists it while nothing runs there too; where something runs, ask first and end every session and terminal running there. Starting anything there lists it again ([details](../tree.md#workspaces-and-worktrees)) |  |
| `worktree.close` |  | end, after asking, every session and terminal running in the worktree selected in the tree, which takes its row out of it; the workspace stays listed ([details](../tree.md#workspaces-and-worktrees)) |  |
| `worktree.new` |  | open the dialog asking for a new worktree of the repository of the workspace or worktree selected in the tree: its name, where it starts, listed from the repository's branches or given as a commit, and optionally the branch it works on; it is made in the background, a row in the tree showing how far it has come, and a new session's draft opens in it once it is made ([details](../tree.md#making-and-deleting-worktrees)) | `tree`: `W` |
| `worktree.merge` |  | merge the branch of the worktree selected in the tree into the main checkout with a merge commit, after asking and saying what it brings; a merge that would conflict is never begun there, and uncommitted changes in the main checkout are stashed or committed only as the question says; afterwards the local issue its session was started for is closed and the worktree is offered for deleting ([details](../tree.md#merging-a-worktree-into-main)) | `tree`: `I` |
| `worktree.delete` |  | delete the worktree selected in the tree, its directory and, as the question says, its branch, after asking and saying what is lost: files changed, commits not pushed, commits not merged, and the sessions and terminals that end with it; it is never deleted while it has changes unless the question is answered to discard them ([details](../tree.md#making-and-deleting-worktrees)) | `tree`: `D` |
| `worktree.cleanup` |  | list the worktrees of the repository of the workspace or worktree selected in the tree with what is weighed before deleting each: the branch, files changed, commits not pushed, whether the branch is merged into the base branch, the state of its pull request, its size and its last commit; those whose pull request is merged or closed and that lose nothing are chosen beforehand, and the chosen ones are deleted together, after a question that names what each loses wherever it loses anything ([details](../tree.md#cleaning-up-worktrees)) | `tree`: `C` |
| `worktree.diff` |  | open in a diff tab the change of the branch of the worktree or workspace selected in the tree: the worktree against the branch's base, committed or not, or what is not committed where the branch cannot be compared ([details](../tree.md#a-branch-against-its-base)) | `tree`: `d` |
| `worktree.branch` | Go to Branch… | open the picker on the branches of the repository of the worktree or workspace selected in the tree, or of the worktree shown: choosing one goes to the worktree it is checked out in, or makes one that checks it out, and `Ctrl+o` switches that worktree to it instead ([details](../tree.md#going-to-a-branch)) | `tree`: `b` |
| `worktree.review` |  | open a draft in the worktree or workspace selected in the tree whose prompt asks an agent to review the branch's change against its base; sending the draft starts the session ([details](../tree.md#a-branch-against-its-base)) | `tree`: `g r` |
| `worktree.explain` |  | open a draft in the worktree or workspace selected in the tree whose prompt asks an agent to explain the branch's change against its base; sending the draft starts the session ([details](../tree.md#a-branch-against-its-base)) | `tree`: `g e` |
| `worktree.notes` |  | open a draft in the worktree or workspace selected in the tree whose prompt asks an agent to leave notes on the lines of the branch's change for your review, through its `annotate` tool; sending the draft starts the session ([review](../review.md#notes-from-agents)) | `tree`: `g n` |
| `worktree.sleep` |  | put the worktree selected in the tree to sleep: its sessions' programs stop and its terminals end, while its files, its branch and its sessions' rows and tabs stay; one question names what is cut short where a session is working or a terminal runs, and a session that cannot be woken again keeps running ([details](../tree.md#putting-a-worktree-to-sleep)) | `tree`: `Z` |
| `worktree.wake` |  | wake the worktree selected in the tree: its sleeping sessions are brought back with their conversations ([details](../tree.md#putting-a-worktree-to-sleep)) |  |
| `worktree.status_todo` |  | mark the worktree selected in the tree as to do, which its row shows and the tree's filter finds by `status:todo` ([details](../tree.md#saying-how-a-worktrees-work-stands)) |  |
| `worktree.status_in_progress` |  | mark the worktree selected in the tree as in progress ([details](../tree.md#saying-how-a-worktrees-work-stands)) |  |
| `worktree.status_in_review` |  | mark the worktree selected in the tree as in review ([details](../tree.md#saying-how-a-worktrees-work-stands)) |  |
| `worktree.status_completed` |  | mark the worktree selected in the tree as completed ([details](../tree.md#saying-how-a-worktrees-work-stands)) |  |
| `worktree.status_clear` |  | take the status off the worktree selected in the tree, leaving its comment ([details](../tree.md#saying-how-a-worktrees-work-stands)) |  |
| `workspace.pin` |  | pin the workspace or worktree selected in the tree to the top of the tree or of its workspace, or unpin it; what is pinned is never left out by the hide filters and stays pinned across runs ([details](../tree.md#arranging-the-tree)) | `tree`: `p` |
| `tree.sort` |  | step the order of a workspace's worktrees below the pinned ones: as the work host lists them, by activity, the worktree a session was written to or started in last first, and by name ([details](../tree.md#arranging-the-tree)) | `tree`: `S` |
| `tree.hide_sleeping` |  | leave the worktrees whose sessions all sleep out of the tree, or show them again; their sessions stay listed under their workspace ([details](../tree.md#arranging-the-tree)) | `tree`: `g h s` |
| `tree.hide_idle` |  | leave the worktrees where no session works, waits or starts and no terminal runs out of the tree, or show them again; their sessions stay listed under their workspace ([details](../tree.md#arranging-the-tree)) | `tree`: `g h i` |
| `tree.hide_main` |  | leave the main checkout out of a workspace that lists other worktrees, or show it again ([details](../tree.md#arranging-the-tree)) | `tree`: `g h m` |
| `tree.reveal_current` |  | select in the tree the row of the session or terminal the focused window shows, unfolding the way to it and showing its worktree again where a hide filter left it out ([details](../tree.md#arranging-the-tree)) | `tree`: `g c` |
| `window.close` | Close Window | close the window; the instance's other windows stay open, and its sessions run on | `global`: `Cmd+W` |
| `window.open` | New Window | open another window of the instance ([windows](../sessions.md#windows)) | `global`: `Mod+n` |
| `refresh` | Refresh | read the sessions, and what git says of where they run, again now | `global`: `Mod+r`<br>`tree`: `r`<br>`agents`: `r` |
| `filter` | Filter… | narrow the tree to the sessions, shells, workspaces and worktrees whose names hold what is typed next; `Enter` keeps it, `Escape` clears it; while a browser page has the keys, search the page as `browser.find` does | `global`: `Mod+f`<br>`tree`: `/`<br>`agents`: `/` |
| `details.tab` |  | show the details panel's tab numbered `n`, from 1 to 9, of those it has for what it describes ([details](../details.md#components)), and the panel when it is hidden | `details`: `1` (`details.tab 1`), `2` (`details.tab 2`), `3` (`details.tab 3`), `4` (`details.tab 4`), `5` (`details.tab 5`), `6` (`details.tab 6`), `7` (`details.tab 7`), `8` (`details.tab 8`), `9` (`details.tab 9`) |
| `worktrees.toggle` | Toggle All Worktrees | list every worktree git knows, those without sessions too, or only those with sessions again | `tree`: `!` |
| `scroll.down` |  | scroll the details panel down a step; bound in the console, scroll it: Claude Code and other programs that read the mouse are sent the wheel, others are scrolled back through what scrolled off; bound in a viewer, scroll what it shows | `viewer`: `j`, `ArrowDown` |
| `scroll.up` |  | scroll up a step, what `scroll.down` scrolls | `viewer`: `k`, `ArrowUp` |
| `scroll.half_down` |  | scroll down half a page, what `scroll.down` scrolls | `details`: `Ctrl+d`<br>`viewer`: `Ctrl+d`<br>`overlay`: `Ctrl+d` |
| `scroll.half_up` |  | scroll up half a page, what `scroll.down` scrolls | `details`: `Ctrl+u`<br>`viewer`: `Ctrl+u`<br>`overlay`: `Ctrl+u` |
| `scroll.top` |  | scroll to the top, what `scroll.down` scrolls | `viewer`: `g g` |
| `scroll.bottom` |  | scroll to the bottom, what `scroll.down` scrolls | `viewer`: `G` |
| `console.send_next` |  | send the next key as it is, whatever it is bound to | `console`: `Ctrl+g Ctrl+o` |
| `console.pass_every_key` |  | send every key as it is until `Escape`; `Ctrl+c` sends `Escape` meanwhile | `console`: `Ctrl+g i`, `Ctrl+g Ctrl+i` |
| `console.read` |  | read the console with Vim's keys, to move, search, select and yank in it, whatever it shows: a shell or a session showing a dialog too, whose keys are otherwise all its own; from a session's prompt, read the console above it | `global`: `Ctrl+q [` |
| `prompt.focus` | Prompt | give the prompt the keys |  |
| `prompt.write` |  | give the prompt the keys, and the key pressed with them | `tree`: `i`, `a`<br>`agents`: `i`, `a`<br>`console`: `i`, `a`, `I`, `A`, `o`, `O`<br>`conversation`: `i`, `a`, `I`, `A`, `o`, `O` |
| `prompt.send` | Send Prompt | send what the prompt holds |  |
| `prompt.images` | Select Attached Image | select an attached image | `prompt`: `Mod+i` |
| `prompt.take_suggestion` |  | on an empty prompt, send the key pressed to the session and then bring the session's input into the prompt, so a prompt Claude Code suggests is taken to edit; an agent whose input cannot be taken back gets the key alone. On a prompt holding text or images, the key, Tab when none, is typed into Neovim |  |
| `prompt.history_prev` |  | put the prompt sent before the one shown in the prompt, to change and send again: those Claude Code recalls with its Up arrow where the focused window's session works | `console`: `ArrowUp`, `Ctrl+p` |
| `prompt.history_next` |  | put the prompt sent after the one shown in the prompt, as `prompt.history_prev` does | `console`: `ArrowDown`, `Ctrl+n` |
| `settings.open` | Settings… | open the Settings window, on the environment of the row selected in the tree or else of the tab shown | `global`: `Mod+,` |
| `daemon.restart` | Restart Session Daemon… | restart the sessions' daemon after asking, ending every session it holds ([details](../sessions.md#the-hosts)) |  |
| `keys.help` | Keyboard Shortcuts | show every binding in force, area by area, the areas the keys are in first, each with what its action does and whether it repeats; `Escape`, `q` or the key that showed it closes it, and no other key acts behind it | `global`: `F1`<br>`prompt`: `F1` |
| `zoom.in` | Zoom In | draw the window a step larger | `global`: `Mod+=`, `Mod+Plus` |
| `zoom.out` | Zoom Out | draw the window a step smaller | `global`: `Mod+-` |
| `zoom.reset` | Actual Size | draw the window at the zoom the settings give | `global`: `Mod+0` |
| `tree.toggle` | Toggle Tree | show or hide the tree | `global`: `Mod+b` |
| `tree.focus` |  | show and focus the file tree; from outside the sidebar, two Escapes run it while the first Escape keeps its usual meaning ([keys](../keys.md#where-the-keys-go)) | `details`: `Escape Escape`<br>`console`: `Escape Escape`<br>`prompt`: `Escape Escape`<br>`viewer`: `Escape Escape`<br>`board`: `Escape Escape` |
| `sidebar.next` |  | focus the other sidebar section, giving its list more room ([agents](../agents-panel.md)) | `tree`: `Tab`, `Shift+Tab`<br>`agents`: `Tab`, `Shift+Tab` |
| `agents.expand` |  | expand the selected agent family or move to its first child | `agents`: `l`, `ArrowRight` |
| `agents.collapse` |  | fold the selected agent family or move to its parent | `agents`: `h`, `ArrowLeft` |
| `agents.reveal` |  | from Agents, show the selected session in Tree, clearing Tree's filter and unfolding its directory path ([agents](../agents-panel.md)) | `agents`: `g t` |
| `details.toggle` | Toggle Details | show or hide the details panel | `global`: `Mod+d` |
| `details.stage` |  | in the details panel, stage the selected change of the worktree, or unstage it when it is only staged; a file in conflict is staged to mark it resolved ([details](../details.md#changes)) | `details`: `s` |
| `details.stage_all` |  | in the details panel, stage every change of the worktree, untracked files included ([details](../details.md#changes)) | `details`: `S` |
| `details.discard` |  | in the details panel, discard the selected change of the worktree, staged or not, after asking; a file in conflict cannot be discarded ([details](../details.md#changes)) | `details`: `X` |
| `details.commit` |  | in the details panel, write the message of a commit in the prompt: sending it commits what is staged, and leaving the prompt gives it up; a hook that refuses the commit says why and the message stays in the prompt ([details](../details.md#changes)) | `details`: `c` |
| `details.amend` |  | in the details panel, after asking, write the message of a commit that replaces the last one, starting from its message, as `details.commit` does ([details](../details.md#changes)) | `details`: `C` |
| `details.pull` |  | in the details panel, pull the branch's upstream; diverged branches are merged unless git is configured to rebase ([details](../details.md#changes)) | `details`: `u` |
| `details.push` |  | in the details panel, push the branch, setting its upstream when it has none ([details](../details.md#changes)) | `details`: `P` |
| `details.force_push` |  | in the details panel, after asking, push the branch over the remote's, but only if nobody else pushed to it since it was fetched ([details](../details.md#changes)) | `details`: `F` |
| `details.create_pull` |  | in the details panel, write a pull request for the branch in the prompt: the title on its first line, the body after it, and the base branch and whether it is a draft as headers above; sending pushes the branch and opens the pull request, and leaving the prompt gives it up ([details](../details.md#changes)) | `details`: `R` |
| `details.draft_pull` |  | in the details panel, ask the agent the pull request recipe names to write the title and body of a pull request for the branch, once, and put them in the prompt to read, edit and send; nothing is pushed or opened until you send ([ai actions](../ai-actions.md)) | `details`: `A` |
| `details.generate_commit` |  | in the details panel, ask the agent the commit message recipe names to write the message of a commit of what is staged, once, and put it in the prompt to read, edit and send; nothing is committed until you send ([ai actions](../ai-actions.md)) | `details`: `M` |
| `details.fix_commit` |  | in the details panel, after a hook refused a commit, open a draft in the worktree whose prompt asks the agent the hook failure recipe names to fix what it complained of without bypassing the hook, committing or pushing; sending it starts the session ([ai actions](../ai-actions.md)) | `details`: `H` |
| `details.abort` |  | in the details panel, after asking, give up the merge, rebase, cherry-pick or revert in progress and put the worktree back as it was before it began ([details](../details.md#branch-and-conflicts)) | `details`: `Q` |
| `details.resume` |  | in the details panel, go on with the merge, rebase, cherry-pick or revert in progress once every conflict is resolved and staged ([details](../details.md#branch-and-conflicts)) | `details`: `m` |
| `details.base` |  | in the details panel, write the base the branch is compared with in the prompt, an empty one giving up the choice for the one Tsumugi finds; sending chooses it ([details](../details.md#branch-and-conflicts)) | `details`: `b` |
| `details.resolve` |  | in the details panel, open a draft in the worktree whose prompt asks the agent the conflict recipe names to resolve the files in conflict, without pushing; sending it starts the session ([ai actions](../ai-actions.md)) | `details`: `I` |
| `details.neovim` |  | in the details panel, open in your Neovim the diff the cursor is on: the file or the commit selected, else the row opened, else what is not committed; a file in conflict opens in its Diffview where it has one ([details](../details.md#in-a-git-repository)) | `details`: `E` |
| `details.open_diff` |  | in the details panel, open in a diff tab the diff the cursor is on, as `details.neovim` chooses it ([details](../details.md#in-a-git-repository)) | `details`: `T` |
| `log.copy_hash` |  | in the details panel, copy the full hash of the commit selected in the log, or opened from it ([details](../details.md#log)) | `details`: `Y` |
| `log.open_remote` |  | in the details panel, open the commit selected in the log on GitHub, in the browser of the worktree; a repository that is not on GitHub says so ([details](../details.md#log)) | `details`: `O` |
| `log.explain` |  | in the details panel, open a draft in the worktree whose prompt asks an agent to explain the commit selected in the log; sending it starts the session ([details](../details.md#log)) | `details`: `x` |
| `log.branch` |  | in the details panel, write the name of a branch to make at the commit selected in the log in the prompt: sending makes it without checking it out, and leaving the prompt gives it up ([details](../details.md#log)) | `details`: `B` |
| `log.worktree` |  | in the details panel, open the new worktree dialog starting at the commit selected in the log ([details](../details.md#log)) | `details`: `W` |
| `log.cherry_pick` |  | in the details panel, after asking, cherry-pick the commit selected in the log onto the branch checked out; a dirty worktree, an unfinished operation, a merge commit and a commit already applied are told and change nothing, and a conflict is left to resolve, continue or abort ([details](../details.md#log)) | `details`: `t` |
| `log.revert` |  | in the details panel, after asking, revert the commit selected in the log in a commit of its own on the branch checked out, with the same refusals as `log.cherry_pick` ([details](../details.md#log)) | `details`: `v` |
| `viewer.zoom_in` |  | in a viewer, draw an image, a PDF, an HTML page or a Markdown document larger ([viewers](../content.md#viewers)) | `viewer`: `Plus`, `=` |
| `viewer.zoom_out` |  | in a viewer, draw an image, a PDF, an HTML page or a Markdown document smaller | `viewer`: `-` |
| `viewer.zoom_fit` |  | in a viewer, draw an image, a PDF, an HTML page or a Markdown document fitting the window | `viewer`: `0` |
| `viewer.page_next` |  | in a viewer, show a PDF's next page | `viewer`: `J` |
| `viewer.page_prev` |  | in a viewer, show a PDF's previous page | `viewer`: `K` |
| `viewer.play` |  | in a viewer, play or pause a movie | `viewer`: `Space` |
| `viewer.back` |  | in a viewer, seek a movie back | `viewer`: `h` |
| `viewer.forward` |  | in a viewer, seek a movie forward | `viewer`: `l` |
| `viewer.copy_path` |  | in a viewer, copy the path of the file it shows | `viewer`: `y` |
| `review.comment` |  | in a diff of lines, write a comment on the line the cursor is on, or on the lines marked with `review.select`; `Enter` keeps it and `Escape` gives it up ([review](../review.md)) | `viewer`: `c` |
| `review.select` |  | in a diff of lines, start marking lines from the cursor to comment on them together, or stop | `viewer`: `v` |
| `review.edit` |  | in a diff of lines, change the comment on the line the cursor is on | `viewer`: `Enter` |
| `review.delete` |  | in a diff of lines, delete the comment on the line the cursor is on | `viewer`: `d d` |
| `review.resolve` |  | in a diff of lines, resolve the comment on the line the cursor is on, or open it again; only this resolves a comment | `viewer`: `r` |
| `review.send` |  | in a diff of lines, show the prompt that carries every comment not resolved; shown, send it to the session chosen with the movement keys | `viewer`: `s` |
| `review.draft` |  | in a diff of lines, or in the prompt shown, write the prompt in a new session's draft in the worktree instead of sending it | `viewer`: `S` |
| `diff.next_hunk` |  | in a diff tab, move to the next change of the file ([details](../details.md#diff-tabs)) | `viewer`: `] c` |
| `diff.prev_hunk` |  | in a diff tab, move to the previous change of the file | `viewer`: `[ c` |
| `diff.next_file` |  | in a diff tab, open the next file of the list of files changed | `viewer`: `] f` |
| `diff.prev_file` |  | in a diff tab, open the previous file of the list of files changed | `viewer`: `[ f` |
| `diff.wrap` |  | in a diff tab, break long lines to the width of the tab, or scroll across them again; kept for the worktree | `viewer`: `w` |
| `diff.change` |  | in a diff tab on a worktree, show the next of the branch's changes: the worktree against the branch's base, what is not committed, then what the branch committed ([details](../details.md#diff-tabs)) | `viewer`: `t` |
| `diff.compare` |  | in a diff tab, write two revisions of the repository, as `main..HEAD`, and show the later against the earlier; a name that is not of a commit is refused ([details](../details.md#diff-tabs)) | `viewer`: `C` |
| `diff.columns` |  | in a diff tab, draw the lines in two columns where the tab is wide enough, always in one, or always in two, the old beside the new; kept for the worktree ([details](../details.md#diff-tabs)) | `viewer`: `\|` |
| `diff.structural` |  | in a diff tab, show the file opened as difftastic reads it, or its lines again; nothing can be commented on in difftastic's view ([details](../details.md#diff-tabs)) | `viewer`: `D` |
| `diff.stage` |  | in a diff tab on what is not committed, stage the hunk whose heading the cursor is on, or the lines the cursor or the mark span inside one hunk ([details](../details.md#diff-tabs)) | `viewer`: `a` |
| `diff.unstage` |  | in a diff tab on what is not committed, take the hunk or the lines chosen out of the index ([details](../details.md#diff-tabs)) | `viewer`: `u` |
| `diff.discard` |  | in a diff tab on what is not committed, discard the hunk or the lines chosen from the worktree, once the key is pressed again to say so ([details](../details.md#diff-tabs)) | `viewer`: `X` |
| `diff.search` |  | in a diff tab, write what to find and bring the cursor to the next line holding it; the case is ignored unless what is written has a capital ([details](../details.md#diff-tabs)) | `viewer`: `/` |
| `diff.search_next` |  | in a diff tab, bring the cursor to the next line holding what was searched for, going round at the end | `viewer`: `n` |
| `diff.search_prev` |  | in a diff tab, bring the cursor to the previous line holding what was searched for, going round at the start | `viewer`: `N` |
| `diff.unfold` |  | in a diff tab, show the file the cursor is in whole, its unchanged lines unfolded, or as its hunks again ([details](../details.md#diff-tabs)) | `viewer`: `z a` |
| `diff.viewed` |  | in a diff tab, mark the file the cursor is in viewed, which folds it and checks it off in the list until its diff changes, or not viewed ([review](../review.md#viewed-files)) | `viewer`: `V` |
| `diff.next_note` |  | in a diff tab, move to the next line an agent left a note under ([review](../review.md#notes-from-agents)) | `viewer`: `] n` |
| `diff.prev_note` |  | in a diff tab, move to the previous line an agent left a note under | `viewer`: `[ n` |
| `diff.files` |  | in a diff tab, show or hide the list of files changed; kept for the worktree | `viewer`: `f` |
| `diff.pictures` |  | in a diff tab on a picture, compare its versions side by side, with a divider swiped across them, or faded over each other; kept for the worktree | `viewer`: `m` |
| `diff.neovim` |  | in a diff tab, open the diff of the file the cursor is in in your Neovim, on the line the cursor is on; the whole change where the cursor is in no file ([details](../details.md#diff-tabs)) | `viewer`: `E` |
| `github.next` |  | in a pull request's or an issue's tab, select the next card of its conversation, which `github.reply`, `github.resolve` and `github.react` act on ([tree](../tree.md#writing-on-a-pull-request-or-an-issue)) | `github`: `n` |
| `github.prev` |  | in a pull request's or an issue's tab, select the previous card of its conversation | `github`: `N` |
| `github.comment` |  | in a pull request's or an issue's tab, write a new comment in the prompt under it; sending the prompt posts it, and leaving the prompt gives it up | `github`: `c` |
| `github.reply` |  | in a pull request's or an issue's tab, reply to the card selected: in its thread of lines when it belongs to one, otherwise as a comment that quotes it | `github`: `r` |
| `github.resolve` |  | in a pull request's tab, resolve the thread of lines the card selected belongs to, or open it again | `github`: `R` |
| `github.react` |  | in a pull request's or an issue's tab, react to the card selected: write `React:` and one of GitHub's eight reactions in the prompt, or `Unreact:` to take one back | `github`: `Plus` |
| `github.review` |  | in a pull request's tab, write a review in the prompt: `Review:` and approve, request-changes or comment on its first line, then your words; sending the prompt submits it | `github`: `v` |
| `github.fix_checks` |  | in a pull request's tab, prepare a request to fix its failed checks in the draft of a new session on the pull request, with the end of their logs read afresh; nothing is sent or posted until you send the prompt ([tree](../tree.md#asking-an-agent-to-fix-a-pull-request)) | `github`: `f` |
| `github.fix_threads` |  | in a pull request's tab, prepare a request to fix the unresolved review threads, or the thread of the card selected, in the draft of a new session on the pull request; nothing is sent, replied to or resolved ([tree](../tree.md#asking-an-agent-to-fix-a-pull-request)) | `github`: `F` |
| `github.merge` |  | in a pull request's tab, merge it with the method its repository prefers, after asking; what GitHub would refuse, such as a missing approval, a failed required check or a conflict, is said instead of asked ([tree](../tree.md#merging-and-closing-a-pull-request)) | `github`: `M` |
| `github.merge_squash` |  | in a pull request's tab, squash and merge it, after asking; refused where the repository does not allow it | `github`: `m s` |
| `github.merge_commit` |  | in a pull request's tab, merge it with a merge commit, after asking; refused where the repository does not allow it | `github`: `m m` |
| `github.merge_rebase` |  | in a pull request's tab, rebase and merge it, after asking; refused where the repository does not allow it | `github`: `m r` |
| `github.auto_merge` |  | in a pull request's tab, have GitHub merge it once what it waits on is met, or stop it where that is on, after asking; offered only while something holds the merge back | `github`: `m a` |
| `github.close` |  | in a pull request's tab, close it, or open it again where it is closed, after asking | `github`: `m c` |
| `github.ready` |  | in a pull request's tab, mark a draft ready for review, or make an open one a draft, after asking | `github`: `m d` |
| `check.rerun` |  | in a check's log, run the failed jobs of its Actions run again, after asking; a check whose address does not name its Actions run is said instead of asked ([tree](../tree.md#reading-a-checks-log)) | `check`: `r` |
| `board.open` |  | open the agent board, a window of every session as a card in the lane of what it asks of you; where it is open, go to it ([board](../board.md)) | `global`: `Ctrl+q a` |
| `board.left` |  | in the agent board, go to the lane on the left, skipping lanes without cards | `board`: `h`, `ArrowLeft` |
| `board.right` |  | in the agent board, go to the lane on the right, skipping lanes without cards | `board`: `l`, `ArrowRight` |
| `board.down` |  | in the agent board, go to the next card of the lane | `board`: `j`, `ArrowDown` |
| `board.up` |  | in the agent board, go to the previous card of the lane | `board`: `k`, `ArrowUp` |
| `board.first` |  | in the agent board, go to the first card of the lane | `board`: `g g` |
| `board.last` |  | in the agent board, go to the last card of the lane | `board`: `G` |
| `board.enter` |  | in the agent board, open the session of the card | `board`: `Enter` |
| `board.idle` |  | in the agent board, show or hide the lane of idle sessions, those that finished and have been quiet for half an hour | `board`: `i` |
| `board.group` |  | in the agent board, fold the sessions a session opened under its card, or give each its own card | `board`: `o` |
| `board.expand` |  | in the agent board, list or fold the sessions under the card, while they are grouped | `board`: `Space` |
| `board.project` |  | in the agent board, narrow it to the next project in turn, and then to none | `board`: `p` |
| `board.state` |  | in the agent board, narrow it to the next lane in turn, and then to none | `board`: `s` |
| `board.clear` |  | in the agent board, drop the search and every filter | `board`: `x` |
| `board.search` |  | in the agent board, type what to search the cards for; Enter keeps it and Esc clears it | `board`: `/` |
| `lightbox.next` |  | in the lightbox, show the next image of the session's Resources, going round at the end ([images in the console](../content.md#images-in-the-console)) | `lightbox`: `l`, `ArrowRight` |
| `lightbox.prev` |  | in the lightbox, show the previous image, going round at the start | `lightbox`: `h`, `ArrowLeft` |
| `lightbox.zoom_in` |  | in the lightbox, draw the image larger | `lightbox`: `Plus`, `=` |
| `lightbox.zoom_out` |  | in the lightbox, draw the image smaller | `lightbox`: `-` |
| `lightbox.zoom_fit` |  | in the lightbox, draw the image whole, in the middle | `lightbox`: `0` |
| `lightbox.pan_left` |  | in the lightbox, move the view of the image left | `lightbox`: `H` |
| `lightbox.pan_right` |  | in the lightbox, move the view of the image right | `lightbox`: `L` |
| `lightbox.pan_up` |  | in the lightbox, move the view of the image up | `lightbox`: `K` |
| `lightbox.pan_down` |  | in the lightbox, move the view of the image down | `lightbox`: `J` |
| `lightbox.close` |  | close the lightbox | `lightbox`: `Escape`, `q` |
| `floating.toggle` |  | open the floating terminal, a shell over the window that every worktree shares; with it open, give it the keys; with it having them, close it, leaving its shells running ([details](../content.md#the-floating-terminal)) | `global`: `Ctrl+Alt+a` |
| `floating.new` |  | in the floating terminal, open another shell in a tab of its own | `floating`: `Ctrl+Alt+t` |
| `floating.close_tab` |  | in the floating terminal, end the shell of the tab shown; the panel closes with the last | `floating`: `Ctrl+Alt+w` |
| `floating.next` |  | in the floating terminal, show the next tab, going round at the end | `floating`: `Ctrl+Alt+]` |
| `floating.prev` |  | in the floating terminal, show the previous tab, going round at the start | `floating`: `Ctrl+Alt+[` |
| `overlay.close` |  | close what is open over the window: a picker, a menu, the keys help or the sheet adding an environment; a question is answered no ([keys](../keys.md#over-the-window)) | `overlay`: `Escape`, `Ctrl+c`, `q`<br>`confirm`: `n` |
| `overlay.down` |  | over the window, move to the next row of a picker's list, a menu or the sheet adding an environment, going round at the end; scroll the keys help down a line | `overlay`: `Ctrl+n`, `Ctrl+j`, `ArrowDown`, `Tab`, `j` |
| `overlay.up` |  | over the window, move to the previous row, as `overlay.down` moves to the next; scroll the keys help up a line | `overlay`: `Ctrl+p`, `Ctrl+k`, `ArrowUp`, `Shift+Tab`, `k` |
| `overlay.choose` |  | over the window, choose the row moved to in a picker's list, a menu or the sheet adding an environment; a question is answered yes | `overlay`: `Enter`<br>`confirm`: `y` |
| `console.send [<key>]` |  | send the key pressed to the session; with a key named after it, as in `console.send Escape`, send that key instead | `global`: `Ctrl+q .` (`console.send Ctrl+q`)<br>`console`: `Enter`, `Ctrl+Enter` (`console.send Enter`), `ArrowLeft`, `ArrowRight`, `Tab`, `Shift+Tab`, `F2`, `Ctrl+l`, `Ctrl+o`, `Ctrl+t`, `Ctrl+_`, `Ctrl+-` (`console.send Ctrl+_`), `Ctrl+c` (`console.send Escape`), `g Ctrl+c` (`console.send Ctrl+c`) |
| `tab.go <n>` |  | show the content's tab numbered `n`, from 1 to 8, or the last when there are fewer; 9 shows the last however many there are | `global`: `Ctrl+q 1` (`tab.go 1`), `Ctrl+q 2` (`tab.go 2`), `Ctrl+q 3` (`tab.go 3`), `Ctrl+q 4` (`tab.go 4`), `Ctrl+q 5` (`tab.go 5`), `Ctrl+q 6` (`tab.go 6`), `Ctrl+q 7` (`tab.go 7`), `Ctrl+q 8` (`tab.go 8`), `Ctrl+q 9` (`tab.go 9`), `Mod+1` (`tab.go 1`), `Mod+2` (`tab.go 2`), `Mod+3` (`tab.go 3`), `Mod+4` (`tab.go 4`), `Mod+5` (`tab.go 5`), `Mod+6` (`tab.go 6`), `Mod+7` (`tab.go 7`), `Mod+8` (`tab.go 8`), `Mod+9` (`tab.go 9`) |
| `tab.merge <n>` |  | merge the content's tab shown into the tab numbered `n`, as `tab.merge_last` does, from 1 to 8, or the last when there are fewer; 9 is the last however many there are | `global`: `Ctrl+q m 1` (`tab.merge 1`), `Ctrl+q m 2` (`tab.merge 2`), `Ctrl+q m 3` (`tab.merge 3`), `Ctrl+q m 4` (`tab.merge 4`), `Ctrl+q m 5` (`tab.merge 5`), `Ctrl+q m 6` (`tab.merge 6`), `Ctrl+q m 7` (`tab.merge 7`), `Ctrl+q m 8` (`tab.merge 8`), `Ctrl+q m 9` (`tab.merge 9`) |

## Default bindings

```toml
[global]
"Ctrl+q h" = "window.left"
"Ctrl+q ArrowLeft" = "window.left"
"Ctrl+q j" = "window.down"
"Ctrl+q ArrowDown" = "window.down"
"Ctrl+q k" = "window.up"
"Ctrl+q ArrowUp" = "window.up"
"Ctrl+q l" = "window.right"
"Ctrl+q ArrowRight" = "window.right"
"Ctrl+q q" = "window.next"
"Ctrl+q Q" = "window.prev"
"Ctrl+q t" = "window.top"
"Ctrl+q b" = "window.bottom"
"Ctrl+q p" = "window.back"
"Ctrl+q v" = "window.vsplit"
"Ctrl+q s" = "window.split"
"Ctrl+q n" = "window.new"
"Ctrl+q i" = "notices.open"
"Ctrl+q a" = "board.open"
"Ctrl+q c" = "window.quit"
"Ctrl+q o" = "window.only"
"Ctrl+q H" = "window.move_left"
"Ctrl+q J" = "window.move_down"
"Ctrl+q K" = "window.move_up"
"Ctrl+q L" = "window.move_right"
"Ctrl+q x" = "window.exchange"
"Ctrl+q r" = "window.rotate"
"Ctrl+q =" = "window.equalize"
"Ctrl+q <" = "window.narrower"
"Ctrl+q >" = "window.wider"
"Ctrl+q Plus" = "window.taller"
"Ctrl+q -" = "window.shorter"
"Ctrl+q _" = "window.maximize_height"
"Ctrl+q |" = "window.maximize_width"
"Ctrl+q T" = "window.to_tab"
"Ctrl+q e" = "window.edit"
"Ctrl+q m 1" = "tab.merge 1"
"Ctrl+q m 2" = "tab.merge 2"
"Ctrl+q m 3" = "tab.merge 3"
"Ctrl+q m 4" = "tab.merge 4"
"Ctrl+q m 5" = "tab.merge 5"
"Ctrl+q m 6" = "tab.merge 6"
"Ctrl+q m 7" = "tab.merge 7"
"Ctrl+q m 8" = "tab.merge 8"
"Ctrl+q m 9" = "tab.merge 9"
"Ctrl+q M" = "tab.merge_last"
"Ctrl+q m m" = "tab.merge_last"
"Ctrl+q [" = "console.read"
"Ctrl+Tab" = "tab.recent"
"Ctrl+Shift+Tab" = "tab.recent_prev"
"Ctrl+PageDown" = "tab.next"
"Ctrl+PageUp" = "tab.prev"
"Cmd+}" = "tab.next"
"Cmd+{" = "tab.prev"
"Mod+Alt+]" = "tab.next_same"
"Mod+Alt+[" = "tab.prev_same"
"Mod+Alt+w" = "tab.close_files"
"Ctrl+q g t" = "tab.next"
"Ctrl+q g T" = "tab.prev"
"Ctrl+q 1" = "tab.go 1"
"Ctrl+q 2" = "tab.go 2"
"Ctrl+q 3" = "tab.go 3"
"Ctrl+q 4" = "tab.go 4"
"Ctrl+q 5" = "tab.go 5"
"Ctrl+q 6" = "tab.go 6"
"Ctrl+q 7" = "tab.go 7"
"Ctrl+q 8" = "tab.go 8"
"Ctrl+q 9" = "tab.go 9"
"Ctrl+q g Tab" = "tab.last"
"Ctrl+q g <" = "tab.move_prev"
"Ctrl+q g >" = "tab.move_next"
"Ctrl+Shift+PageUp" = "tab.move_prev"
"Ctrl+Shift+PageDown" = "tab.move_next"
"Mod+1" = "tab.go 1"
"Mod+2" = "tab.go 2"
"Mod+3" = "tab.go 3"
"Mod+4" = "tab.go 4"
"Mod+5" = "tab.go 5"
"Mod+6" = "tab.go 6"
"Mod+7" = "tab.go 7"
"Mod+8" = "tab.go 8"
"Mod+9" = "tab.go 9"
"Mod+w" = "tab.close"
"Cmd+W" = "window.close"
"Mod+n" = "window.open"
"Ctrl+q }" = "workspace.next"
"Ctrl+q {" = "workspace.prev"
"Mod+j" = "waiting.next"
"Mod+k" = "waiting.prev"
"Mod+p" = "session.pick"
"Ctrl+q ." = "console.send Ctrl+q"
"Mod+t" = "session.new"
"Cmd+T" = "browser.reopen"
"Mod+Alt+t" = "browser.reopen"
"Mod+s" = "shell.open"
"Mod+e" = "neovim.open"
"Mod+o" = "session.resume"
"Mod+Backspace" = "session.stop"
"Mod+f" = "filter"
"Mod+r" = "refresh"
"Mod+," = "settings.open"
"Mod+=" = "zoom.in"
"Mod+Plus" = "zoom.in"
"Mod+-" = "zoom.out"
"Mod+0" = "zoom.reset"
"Mod+b" = "tree.toggle"
"Mod+d" = "details.toggle"
"Ctrl+Alt+a" = "floating.toggle"
"F1" = "keys.help"

[draft]
"Ctrl+g p" = "session.profile_next"
"Ctrl+g P" = "session.profile_prev"

[tree]
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Enter" = "cursor.go"
"o" = "cursor.go"
"l" = "cursor.go"
"s" = "cursor.split"
"v" = "cursor.vsplit"
"t" = "cursor.tab"
"L" = "cursor.go"
"Shift+Enter" = "cursor.go"
"Ctrl+Enter" = "cursor.go"
"h" = "cursor.fold"
"n" = "session.new"
"$" = "shell.open"
"e" = "neovim.open"
"O" = "session.resume"
"x" = "cursor.close"
"X" = "session.stop"
"R" = "session.rename"
"U" = "session.mark_unread"
"Ctrl+r" = "session.restart"
"M" = "notifications.mute"
"y" = "cursor.copy"
"g x" = "cursor.browse"
"i" = "prompt.write"
"a" = "prompt.write"
"r" = "refresh"
"/" = "filter"
"w" = "session.pick"
"A" = "workspace.add"
"!" = "worktrees.toggle"
"c" = "saved_commands.open"
"W" = "worktree.new"
"I" = "worktree.merge"
"D" = "worktree.delete"
"C" = "worktree.cleanup"
"d" = "worktree.diff"
"b" = "worktree.branch"
"g r" = "worktree.review"
"g e" = "worktree.explain"
"g n" = "worktree.notes"
"p" = "workspace.pin"
"S" = "tree.sort"
"g h s" = "tree.hide_sleeping"
"g h i" = "tree.hide_idle"
"g h m" = "tree.hide_main"
"g c" = "tree.reveal_current"
"Z" = "worktree.sleep"
"N" = "file.new"
"K" = "directory.new"
"}" = "workspace.next"
"{" = "workspace.prev"

"Tab" = "sidebar.next"
"Shift+Tab" = "sidebar.next"
"] ]" = "window.next"
"[ [" = "window.prev"

[agents]
"g t" = "agents.reveal"
"Tab" = "sidebar.next"
"Shift+Tab" = "sidebar.next"
"] ]" = "window.next"
"[ [" = "window.prev"
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Enter" = "cursor.go"
"o" = "cursor.go"
"l" = "agents.expand"
"ArrowRight" = "agents.expand"
"h" = "agents.collapse"
"ArrowLeft" = "agents.collapse"
"s" = "cursor.split"
"v" = "cursor.vsplit"
"t" = "cursor.tab"
"X" = "session.stop"
"R" = "session.rename"
"U" = "session.mark_unread"
"Ctrl+r" = "session.restart"
"n" = "session.new"
"i" = "prompt.write"
"a" = "prompt.write"
"/" = "filter"
"y" = "cursor.copy"
"r" = "refresh"
"w" = "session.pick"

[filer]
"j" = "filer.down"
"ArrowDown" = "filer.down"
"k" = "filer.up"
"ArrowUp" = "filer.up"
"l" = "filer.into"
"ArrowRight" = "filer.into"
"h" = "filer.out"
"ArrowLeft" = "filer.out"
"Enter" = "filer.open"
"!" = "filer.hidden"

[details]
"Escape Escape" = "tree.focus"
"Tab" = "tab.next"
"Shift+Tab" = "tab.prev"
"] ]" = "window.next"
"[ [" = "window.prev"
"j" = "cursor.down"
"ArrowDown" = "cursor.down"
"k" = "cursor.up"
"ArrowUp" = "cursor.up"
"g g" = "cursor.first"
"G" = "cursor.last"
"Escape" = "cursor.fold"
"y" = "cursor.copy"
"a" = "resource.add"
"e" = "resource.edit"
"D" = "resource.remove"
"p" = "resource.keep"
"Backspace" = "cursor.fold"
"s" = "details.stage"
"S" = "details.stage_all"
"X" = "details.discard"
"c" = "details.commit"
"C" = "details.amend"
"u" = "details.pull"
"P" = "details.push"
"F" = "details.force_push"
"R" = "details.create_pull"
"A" = "details.draft_pull"
"M" = "details.generate_commit"
"H" = "details.fix_commit"
"Q" = "details.abort"
"m" = "details.resume"
"b" = "details.base"
"I" = "details.resolve"
"E" = "details.neovim"
"T" = "details.open_diff"
"Y" = "log.copy_hash"
"O" = "log.open_remote"
"x" = "log.explain"
"B" = "log.branch"
"W" = "log.worktree"
"t" = "log.cherry_pick"
"v" = "log.revert"
"1" = "details.tab 1"
"2" = "details.tab 2"
"3" = "details.tab 3"
"4" = "details.tab 4"
"5" = "details.tab 5"
"6" = "details.tab 6"
"7" = "details.tab 7"
"8" = "details.tab 8"
"9" = "details.tab 9"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[console]
"Escape Escape" = "tree.focus"
"] ]" = "window.next"
"[ [" = "window.prev"
"Enter" = "console.send"
"Ctrl+Enter" = "console.send Enter"
"ArrowUp" = "prompt.history_prev"
"ArrowDown" = "prompt.history_next"
"ArrowLeft" = "console.send"
"ArrowRight" = "console.send"
"Tab" = "console.send"
"Shift+Tab" = "console.send"
"F2" = "console.send"
"Ctrl+l" = "console.send"
"Ctrl+p" = "prompt.history_prev"
"Ctrl+n" = "prompt.history_next"
"Ctrl+o" = "console.send"
"Ctrl+t" = "console.send"
"Ctrl+_" = "console.send"
"Ctrl+-" = "console.send Ctrl+_"
"Ctrl+c" = "console.send Escape"
"g Ctrl+c" = "console.send Ctrl+c"
"Ctrl+g Ctrl+o" = "console.send_next"
"Ctrl+g i" = "console.pass_every_key"
"Ctrl+g Ctrl+i" = "console.pass_every_key"
"i" = "prompt.write"
"a" = "prompt.write"
"I" = "prompt.write"
"A" = "prompt.write"
"o" = "prompt.write"
"O" = "prompt.write"
"X" = "session.stop"
"Ctrl+r" = "session.restart"

[console.claude]
"Tab" = "prompt.write"

[prompt]
"Escape Escape" = "tree.focus"
"Mod+i" = "prompt.images"
"F1" = "keys.help"

[viewer]
"Escape Escape" = "tree.focus"
"] ]" = "window.next"
"[ [" = "window.prev"
"j" = "scroll.down"
"ArrowDown" = "scroll.down"
"k" = "scroll.up"
"ArrowUp" = "scroll.up"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"
"g g" = "scroll.top"
"G" = "scroll.bottom"
"Plus" = "viewer.zoom_in"
"=" = "viewer.zoom_in"
"-" = "viewer.zoom_out"
"0" = "viewer.zoom_fit"
"J" = "viewer.page_next"
"K" = "viewer.page_prev"
"Space" = "viewer.play"
"h" = "viewer.back"
"l" = "viewer.forward"
"y" = "viewer.copy_path"
"c" = "review.comment"
"v" = "review.select"
"Enter" = "review.edit"
"d d" = "review.delete"
"r" = "review.resolve"
"s" = "review.send"
"S" = "review.draft"
"] c" = "diff.next_hunk"
"[ c" = "diff.prev_hunk"
"] f" = "diff.next_file"
"[ f" = "diff.prev_file"
"w" = "diff.wrap"
"f" = "diff.files"
"m" = "diff.pictures"
"t" = "diff.change"
"C" = "diff.compare"
"|" = "diff.columns"
"D" = "diff.structural"
"a" = "diff.stage"
"u" = "diff.unstage"
"X" = "diff.discard"
"/" = "diff.search"
"n" = "diff.search_next"
"N" = "diff.search_prev"
"z a" = "diff.unfold"
"V" = "diff.viewed"
"E" = "diff.neovim"
"] n" = "diff.next_note"
"[ n" = "diff.prev_note"
"g x" = "cursor.browse"
"e" = "window.edit"
"q" = "window.quit"

[github]
"n" = "github.next"
"N" = "github.prev"
"c" = "github.comment"
"r" = "github.reply"
"R" = "github.resolve"
"Plus" = "github.react"
"v" = "github.review"
"f" = "github.fix_checks"
"F" = "github.fix_threads"
"M" = "github.merge"
"m s" = "github.merge_squash"
"m m" = "github.merge_commit"
"m r" = "github.merge_rebase"
"m a" = "github.auto_merge"
"m c" = "github.close"
"m d" = "github.ready"

[check]
"r" = "check.rerun"

[conversation]
"i" = "prompt.write"
"a" = "prompt.write"
"I" = "prompt.write"
"A" = "prompt.write"
"o" = "prompt.write"
"O" = "prompt.write"

[board]
"Escape Escape" = "tree.focus"
"] ]" = "window.next"
"[ [" = "window.prev"
"h" = "board.left"
"ArrowLeft" = "board.left"
"l" = "board.right"
"ArrowRight" = "board.right"
"j" = "board.down"
"ArrowDown" = "board.down"
"k" = "board.up"
"ArrowUp" = "board.up"
"g g" = "board.first"
"G" = "board.last"
"Enter" = "board.enter"
"i" = "board.idle"
"o" = "board.group"
"Space" = "board.expand"
"p" = "board.project"
"s" = "board.state"
"x" = "board.clear"
"/" = "board.search"
"q" = "window.quit"

[lightbox]
"l" = "lightbox.next"
"ArrowRight" = "lightbox.next"
"h" = "lightbox.prev"
"ArrowLeft" = "lightbox.prev"
"Plus" = "lightbox.zoom_in"
"=" = "lightbox.zoom_in"
"-" = "lightbox.zoom_out"
"0" = "lightbox.zoom_fit"
"H" = "lightbox.pan_left"
"L" = "lightbox.pan_right"
"K" = "lightbox.pan_up"
"J" = "lightbox.pan_down"
"Escape" = "lightbox.close"
"q" = "lightbox.close"

[floating]
"Ctrl+Alt+t" = "floating.new"
"Ctrl+Alt+w" = "floating.close_tab"
"Ctrl+Alt+]" = "floating.next"
"Ctrl+Alt+[" = "floating.prev"

[overlay]
"Escape" = "overlay.close"
"Ctrl+c" = "overlay.close"
"q" = "overlay.close"
"Ctrl+n" = "overlay.down"
"Ctrl+j" = "overlay.down"
"ArrowDown" = "overlay.down"
"Tab" = "overlay.down"
"j" = "overlay.down"
"Ctrl+p" = "overlay.up"
"Ctrl+k" = "overlay.up"
"ArrowUp" = "overlay.up"
"Shift+Tab" = "overlay.up"
"k" = "overlay.up"
"Enter" = "overlay.choose"
"Ctrl+d" = "scroll.half_down"
"Ctrl+u" = "scroll.half_up"

[confirm]
"y" = "overlay.choose"
"n" = "overlay.close"
```
