# The agent board

The board is a window of the content that shows every session of every
connected environment as a card, in the column of what it asks of you: Needs
You for a session waiting on you or failed, Working, Done for one that
finished, and Idle for one that finished, was seen and has been quiet for half
an hour. Idle is hidden until asked for, with how long each card has been idle
told on it. A card names its session and agent, quotes the last words of you and
of the agent from the conversation, says what a waiting session waits on, and
tells its worktree, project and how long ago it came to stand as it does.
Needs You cards are tinted amber and Done cards green.

`board.open` opens it, or goes to it where it is open. The keys move over it as
Neovim does: `h` and `l` between lanes, `j` and `k` between cards, `Enter`
opens the card's session. [Every key](reference/keys.md) can be rebound.

The cursor stays on its session when the session changes lane, and a search or
filter that hides it moves it to the card nearest where it was. Within a lane
the card that changed most recently comes first.

The board is searched by what a card shows, and narrowed to projects and to
lanes by chips; each chip toggles, and one key clears them all. A session
opened by another, as a supervised worker is, can be folded under the card of
its opener, with the workers that need you counted on it; a worker whose opener
is not listed keeps a card of its own. Pointing at a card opens its session as
the Agents panel does.

Relations between sessions come from Tsumugi's session records, not from where
they run.

The board draws and drags nothing beyond this, and does not pop out of the
window.
