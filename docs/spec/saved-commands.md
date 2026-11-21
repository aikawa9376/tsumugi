# Saved commands

Saved commands are the shell commands you type again and again for a project:
`just ci`, a development server, a setup script. Each has a label and the
text of a command, which may span several lines. Open them with Saved
Commands (`saved_commands.open`, [keys](reference/keys.md)).

They belong to the project, the workspace holding the directory, so every
worktree of a repository, every window and every session of the project sees
the same ones. Tsumugi's work host keeps them, and they outlive windows and
sessions. A project on a work machine keeps its commands on that machine
([remote execution](remote-execution.md)).

## Using them

The list opens for the worktree selected, or the one the focused window
shows, and is searched by typing: a label or any part of the command. The
keys move through it, run the command moved to, add a command, and edit or
delete it; deleting asks first. Saving a command runs nothing.

Choosing a command first shows what will happen: its text, the directory it
runs in and the machine it runs on. It starts only when you confirm.

A command runs in a new command terminal in the worktree selected when you
choose it, not the one it was saved from: a command saved from one worktree
runs in another when that is the one selected. It runs on the machine of that
worktree's environment. The text is given to the shell as it is written, so
spaces and non-ASCII characters in the directory or the text, and several
lines, need nothing extra.

The terminal is a command terminal as any other
([terminals](content.md#terminals)): its output stays until it is stopped,
it says the status the command ended with, a failed command keeps its
windows open, and a success closes them unless Settings keeps them. The
terminal opens without taking the keys, so what you were writing in a prompt
goes on. Running a command again is choosing it again; nothing runs a saved
command but that choice, and none comes from files in a repository.

A label and a command are both required. A label is at most 256 bytes and a
command at most 16384.
