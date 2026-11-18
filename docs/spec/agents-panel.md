# The agents panel

The left sidebar keeps Tree and Agents visible together. Focusing a section
gives it most of the room while the other stays available. The Find field
stays in place above both and searches the focused section. Each section
keeps its own query, cursor and folds when focus changes.

Tree keeps the view by directory; Agents shows sessions across connected
environments independently of the tree's folds. Sessions can be opened with
the keyboard or a click. Agent rows offer the same context menu as session
rows in Tree, with actions applied to the row the menu was opened on.

Agents groups sessions by the parent that started them, followed by its
children. It shows each session's reported state in words as well as a mark,
and keeps the family's activity visible when its children are folded. A
parent no longer listed remains identifiable above its children, without
pretending that it is still running. Relationships come from Tsumugi's
session records; a shared directory does not imply a parent relationship.

The panel summarizes activity across its sessions. Filtering finds sessions
without losing the parents that explain where they came from. Environment
connection status remains separate from agent status, so a disconnected
environment's last report is not presented as a fresh connection.
