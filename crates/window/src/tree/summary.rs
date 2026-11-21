//! Counts beside a tree heading, read from the same states as its rows.

use dioxus::prelude::*;
use tsumugi_core::completion::Completions;
use tsumugi_core::tree::nav::{self, Heading, Item};
use tsumugi_model::{State, TerminalProgram, Tree, Wait};

use crate::components::glyph;
use crate::icons::{Icon, IconView};

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Summary {
    sessions: usize,
    terminals: usize,
    neovims: usize,
    states: Vec<(State, usize)>,
}

impl Summary {
    pub(super) fn is_empty(&self) -> bool {
        self.sessions == 0 && self.terminals == 0 && self.neovims == 0
    }

    fn count(&mut self, state: &State) {
        if glyph(state).is_none() {
            return;
        }
        let state = match state {
            State::Waiting(_) => State::Waiting(Wait::Input),
            other => other.clone(),
        };
        if let Some((_, count)) = self.states.iter_mut().find(|(s, _)| *s == state) {
            *count += 1;
        } else {
            self.states.push((state, 1));
        }
    }

    fn label(&self) -> String {
        let mut label = format!(
            "{} sessions, {} terminals, {} Neovims",
            self.sessions, self.terminals, self.neovims
        );
        for (state, count) in &self.states {
            label.push_str(&format!(", {count} {}", state.word()));
        }
        label
    }
}

/// The caller supplies the full tree for an environment, the filtered tree
/// for rows under it. Folds do not affect the work counted here.
pub(super) fn summarize(tree: &Tree, marks: &Completions, row: &Item) -> Option<Summary> {
    let heading = match row {
        Item::Environment(_) | Item::Workspace(..) | Item::Worktree(..) => None,
        Item::Across(_, _, heading) | Item::Heading(_, _, heading)
            if !matches!(heading, Heading::Files | Heading::Browser) =>
        {
            Some(*heading)
        }
        _ => return None,
    };
    let mut summary = Summary::default();
    for workspace in &tree.workspaces {
        if matches!(row, Item::Workspace(_, root) | Item::Across(_, root, _) if *root != workspace.root)
        {
            continue;
        }
        for worktree in &workspace.worktrees {
            if matches!(row, Item::Worktree(_, path) | Item::Heading(_, path, _) if *path != worktree.worktree.path)
            {
                continue;
            }
            if heading.is_none() || heading == Some(Heading::Sessions) {
                for session in &worktree.sessions {
                    summary.sessions += 1;
                    summary.count(marks.state(session));
                }
            }
            if heading != Some(Heading::Sessions) {
                let listed = heading.map(|h| nav::under(row.environment(), worktree, h, &[]));
                for terminal in &worktree.terminals {
                    if listed.as_ref().is_some_and(|rows| {
                        !rows.contains(&Item::Terminal(row.environment().clone(), terminal.id))
                    }) {
                        continue;
                    }
                    match terminal.program {
                        TerminalProgram::Neovim => summary.neovims += 1,
                        TerminalProgram::Shell | TerminalProgram::Command => summary.terminals += 1,
                    }
                    if let Some(state) = marks.terminal_state(terminal) {
                        summary.count(state);
                    }
                }
            }
        }
    }
    summary.states.sort_by_key(|(s, _)| match s {
        State::Waiting(_) => 0,
        State::Failed => 1,
        State::Working => 2,
        State::Done => 3,
        State::Background => 4,
        State::Starting => 5,
        State::Idle => 6,
    });
    Some(summary)
}

#[component]
pub(super) fn WorkSummary(summary: Summary) -> Element {
    let label = summary.label();
    rsx! {
        if !summary.is_empty() {
            span { class: "tree-summary", role: "img", "aria-label": label.clone(), title: label,
                if summary.sessions > 0 {
                    span { class: "tree-count tree-total", title: "{summary.sessions} sessions",
                        IconView { icon: Icon::Message, size: 12 }
                        "{summary.sessions}"
                    }
                }
                if summary.terminals > 0 {
                    span { class: "tree-count tree-total", title: "{summary.terminals} terminals",
                        IconView { icon: Icon::Terminal2, size: 12 }
                        "{summary.terminals}"
                    }
                }
                if summary.neovims > 0 {
                    span { class: "tree-count tree-total", title: "{summary.neovims} Neovims",
                        IconView { icon: Icon::FilePencil, size: 12 }
                        "{summary.neovims}"
                    }
                }
                for (state, count) in &summary.states {
                    if let Some((class, icon, word)) = glyph(state) {
                        span { class: "tree-count {class}", title: "{count} {word}",
                            IconView {
                                icon: if *state == State::Starting { Icon::Loader } else { icon },
                                size: 12,
                            }
                            "{count}"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_core::completion::{Completions, Viewed};
    use tsumugi_core::tree::nav::{self, Heading, Item};
    use tsumugi_model::testing::{session, shell, tree};
    use tsumugi_model::{EnvironmentId, State, TerminalProgram, Wait};

    fn environment() -> EnvironmentId {
        EnvironmentId::unbound()
    }

    #[test]
    fn idle_work_keeps_its_totals_without_redundant_state_counts() {
        let mut command = shell(1, "/api");
        command.state = Some(State::Done);
        let tree = tree()
            .session(session("idle").cwd("/api").state(State::Idle))
            .session(session("seen").cwd("/api").state(State::Done))
            .terminal(command)
            .build();
        let marks = Completions::default()
            .updated(&tree, Some(Viewed::Session("seen")))
            .updated(&tree, Some(Viewed::Terminal(1)));
        for row in [
            Item::Environment(environment()),
            Item::Workspace(environment(), "/api".into()),
            Item::Worktree(environment(), "/api".into()),
            Item::Heading(environment(), "/api".into(), Heading::Sessions),
            Item::Across(environment(), "/api".into(), Heading::Terminals),
        ] {
            let summary = summarize(&tree, &marks, &row).unwrap();
            assert!(!summary.is_empty());
            assert!(summary.states.is_empty(), "{row:?}: {summary:?}");
            assert!(!summary.label().contains("idle"));
        }
        let summary = summarize(&tree, &marks, &Item::Environment(environment())).unwrap();
        assert_eq!((summary.sessions, summary.terminals), (2, 1));
    }

    #[test]
    fn parent_summaries_count_each_session_once_across_worktrees() {
        let tree = tree()
            .session(session("run").cwd("/api").state(State::Working))
            .session(
                session("wait")
                    .cwd("/api/topic")
                    .state(State::Waiting(Wait::Input)),
            )
            .session(session("elsewhere").cwd("/web").state(State::Failed))
            .checkout("/api", "/api", "main")
            .checkout("/api/topic", "/api", "topic")
            .build();
        let marks = Completions::default();
        let summary = summarize(&tree, &marks, &Item::Environment(environment())).unwrap();
        assert_eq!(summary.sessions, 3);
        assert_eq!(summary.terminals, 0);
        assert_eq!(
            summary.states,
            vec![
                (State::Waiting(Wait::Input), 1),
                (State::Failed, 1),
                (State::Working, 1)
            ]
        );
        for row in [
            Item::Workspace(environment(), "/api".into()),
            Item::Across(environment(), "/api".into(), Heading::Sessions),
        ] {
            let summary = summarize(&tree, &marks, &row).unwrap();
            assert_eq!(summary.sessions, 2);
            assert_eq!(
                summary.states,
                vec![(State::Waiting(Wait::Input), 1), (State::Working, 1)]
            );
        }
        let summary = summarize(
            &tree,
            &marks,
            &Item::Worktree(environment(), "/api/topic".into()),
        )
        .unwrap();
        assert_eq!(summary.sessions, 1);
        assert_eq!(summary.states, vec![(State::Waiting(Wait::Input), 1)]);
    }

    #[test]
    fn headings_use_the_filtered_scope_but_an_environment_can_use_all_work() {
        let full = tree()
            .session(
                session("keep")
                    .name("keep")
                    .cwd("/api")
                    .state(State::Working),
            )
            .session(
                session("hide")
                    .name("hide")
                    .cwd("/api")
                    .state(State::Failed),
            )
            .build();
        let narrowed = nav::narrowed(&full, "keep");
        let marks = Completions::default();
        let row = Item::Heading(environment(), "/api".into(), Heading::Sessions);
        let summary = summarize(&narrowed, &marks, &row).unwrap();
        assert_eq!(summary.sessions, 1);
        assert_eq!(summary.states, vec![(State::Working, 1)]);
        assert_eq!(
            summarize(&full, &marks, &Item::Environment(environment()))
                .unwrap()
                .sessions,
            2
        );
    }

    #[test]
    fn summaries_group_wait_reasons_and_keep_special_states_distinct() {
        let tree = tree()
            .session(
                session("permission")
                    .cwd("/api")
                    .state(State::Waiting(Wait::Permission)),
            )
            .session(
                session("input")
                    .cwd("/api")
                    .state(State::Waiting(Wait::Input)),
            )
            .session(session("starting").cwd("/api").state(State::Starting))
            .session(session("background").cwd("/api").state(State::Background))
            .session(session("idle").cwd("/api").state(State::Idle))
            .session(session("done").cwd("/api").state(State::Done))
            .build();
        let marks = Completions::default().updated(&tree, Some(Viewed::Session("done")));
        let summary = summarize(&tree, &marks, &Item::Environment(environment())).unwrap();
        assert_eq!(summary.sessions, 6);
        assert_eq!(
            summary.states,
            vec![
                (State::Waiting(Wait::Input), 2),
                (State::Background, 1),
                (State::Starting, 1)
            ]
        );
        let unseen = summarize(
            &tree,
            &Completions::default(),
            &Item::Environment(environment()),
        )
        .unwrap();
        assert!(unseen.states.contains(&(State::Done, 1)));
    }

    #[test]
    fn terminal_headings_count_only_their_rows_and_use_acknowledged_states() {
        let mut running = shell(1, "/api");
        running.state = Some(State::Working);
        let mut done = shell(2, "/api");
        done.state = Some(State::Done);
        let mut editor = shell(3, "/api");
        editor.program = TerminalProgram::Neovim;
        editor.file = Some("/api/readme.md".into());
        editor.state = Some(State::Failed);
        let tree = tree()
            .terminal(running)
            .terminal(done)
            .terminal(editor)
            .terminal(shell(4, "/api"))
            .build();
        let marks = Completions::default().updated(&tree, Some(Viewed::Terminal(2)));
        let row = Item::Heading(environment(), "/api".into(), Heading::Terminals);
        let summary = summarize(&tree, &marks, &row).unwrap();
        assert_eq!(summary.sessions, 0);
        assert_eq!(summary.terminals, 3);
        assert_eq!(summary.states, vec![(State::Working, 1)]);
        let all = summarize(&tree, &marks, &Item::Environment(environment())).unwrap();
        assert_eq!(all.terminals, 3);
        assert_eq!(all.neovims, 1);
        assert!(all.states.contains(&(State::Failed, 1)));
    }

    #[test]
    fn neovims_do_not_count_as_terminals_in_parent_summaries() {
        let mut editor = shell(2, "/api");
        editor.program = TerminalProgram::Neovim;
        let mut on_file = editor.clone();
        on_file.id = 3;
        on_file.file = Some("/api/readme.md".into());
        let tree = tree()
            .session(session("agent").cwd("/api").state(State::Working))
            .terminal(shell(1, "/api"))
            .terminal(editor)
            .terminal(on_file)
            .build();
        let summary = summarize(
            &tree,
            &Completions::default(),
            &Item::Environment(environment()),
        )
        .unwrap();
        assert_eq!(summary.sessions, 1);
        assert_eq!(summary.terminals, 1);
        assert_eq!(summary.states, vec![(State::Working, 1)]);
        assert_eq!(
            summary.label(),
            "1 sessions, 1 terminals, 2 Neovims, 1 working"
        );
    }

    #[test]
    fn each_program_heading_counts_only_its_kind_and_state() {
        let mut terminal = shell(1, "/api");
        terminal.state = Some(State::Failed);
        let mut editor = shell(2, "/api");
        editor.program = TerminalProgram::Neovim;
        editor.state = Some(State::Working);
        let tree = tree()
            .session(
                session("agent")
                    .cwd("/api")
                    .state(State::Waiting(Wait::Input)),
            )
            .terminal(terminal)
            .terminal(editor)
            .build();
        for (heading, counts, states) in [
            (
                Heading::Sessions,
                (1, 0, 0),
                vec![(State::Waiting(Wait::Input), 1)],
            ),
            (Heading::Terminals, (0, 1, 0), vec![(State::Failed, 1)]),
            (Heading::Neovim, (0, 0, 1), vec![(State::Working, 1)]),
        ] {
            for row in [
                Item::Heading(environment(), "/api".into(), heading),
                Item::Across(environment(), "/api".into(), heading),
            ] {
                let summary = summarize(&tree, &Completions::default(), &row).unwrap();
                assert_eq!(
                    (summary.sessions, summary.terminals, summary.neovims),
                    counts
                );
                assert_eq!(summary.states, states);
            }
        }
        let all = summarize(
            &tree,
            &Completions::default(),
            &Item::Environment(environment()),
        )
        .unwrap();
        assert_eq!((all.sessions, all.terminals, all.neovims), (1, 1, 1));
        assert_eq!(all.states.iter().map(|(_, count)| count).sum::<usize>(), 3);
    }

    #[test]
    fn empty_scopes_have_no_badges_and_leaves_and_files_have_no_summary() {
        let tree = tree().build();
        let marks = Completions::default();
        assert!(
            summarize(&tree, &marks, &Item::Environment(environment()))
                .unwrap()
                .is_empty()
        );
        for row in [
            Item::Heading(environment(), "/api".into(), Heading::Files),
            Item::Session(environment(), "missing".into()),
        ] {
            assert!(summarize(&tree, &marks, &row).is_none());
        }
    }
}
