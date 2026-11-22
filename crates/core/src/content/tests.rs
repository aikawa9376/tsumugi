use crate::testing::here;
use std::collections::BTreeSet;
use std::path::Path;

use tsumugi_model::{TerminalProgram, Wait};
use tsumugi_protocol::Request;

use super::*;

const CHANGES: usize = 1;
use crate::actions::Action;
use crate::chrome::panels::layout::Side;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{
    SessionBuilder, listed, listing, patch, session, session_tab, shell, show, shows_there,
    snapshot, there,
};
use crate::tree::nav::Item;

fn place(worktree: &str) -> Option<Place> {
    Some(Place::at(&here(), worktree))
}

fn active(run: &Scenario) -> Option<Tab> {
    run.state().content.active().cloned()
}

#[test]
fn a_browser_opens_for_the_selected_environment_without_starting_a_program() {
    let run = Scenario::new()
        .send(Msg::TreeSelect(Some(Item::Environment(there()))))
        .send(Msg::Focus(Area::Sidebar))
        .send(Msg::Window(Action::BrowserOpen));
    let tab = active(&run).unwrap();
    assert!(matches!(&tab, Tab::Browser { environment, .. } if *environment == there()));
    assert!(!shows_prompt(Some(&tab)));
    assert!(
        run.last_effects()
            .iter()
            .all(|e| !matches!(e, Effect::StartTerminal(_)))
    );
}

#[test]
fn opening_a_browser_again_opens_another_page_listed_under_the_worktree_it_was_opened_in() {
    let run = listed_with([session("a").cwd("/r")])
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console))
        .send(Msg::Window(Action::BrowserOpen))
        .send(Msg::Window(Action::BrowserOpen));
    let pages = run.state().content.pages();
    assert_eq!(pages.len(), 2);
    assert_ne!(pages[0].1.id, pages[1].1.id);
    assert!(
        pages
            .iter()
            .all(|(e, p)| *e == here() && p.worktree == Path::new("/r"))
    );
    let rows = run.state().tree.rows();
    let browsers = rows
        .iter()
        .filter(|row| matches!(row, Item::Browser(..)))
        .count();
    assert!(browsers >= 2, "{rows:?}");
}

#[test]
fn a_page_stays_listed_under_its_worktree_while_another_worktree_is_shown() {
    let (a, b) = (Tab::browser(here(), "/r/a"), Tab::browser(here(), "/r/b"));
    let run = two_worktrees()
        .send(show_at(a.clone(), "/r/a"))
        .send(show_at(b.clone(), "/r/b"));
    assert_eq!(shown(&run), std::slice::from_ref(&b));
    let (Tab::Browser { id: a_id, .. }, Tab::Browser { id: b_id, .. }) = (&a, &b) else {
        unreachable!()
    };
    let listed: Vec<_> = run
        .state()
        .content
        .pages()
        .into_iter()
        .map(|(environment, page)| (environment, page.worktree, page.id))
        .collect();
    assert_eq!(
        listed,
        [
            (here(), "/r/a".into(), *a_id),
            (here(), "/r/b".into(), *b_id)
        ]
    );
    // Coming back shows the page the worktree left.
    let run = run.send(show_at(session_tab("a"), "/r/a"));
    assert_eq!(shown(&run), [a, session_tab("a")]);
}

#[test]
fn a_page_put_away_with_its_worktree_is_not_the_one_shown_in_a_window_of_the_same_number() {
    // Each worktree's layout numbers its windows from 1, so the number alone
    // would let a page kept hidden speak for the page shown.
    let (a, b) = (Tab::browser(here(), "/r/a"), Tab::browser(here(), "/r/b"));
    let (Tab::Browser { id: a_id, .. }, Tab::Browser { id: b_id, .. }) = (&a, &b) else {
        unreachable!()
    };
    let (a_id, b_id) = (*a_id, *b_id);
    let run = two_worktrees()
        .send(show_at(a.clone(), "/r/a"))
        .send(show_at(b, "/r/b"));
    let content = &run.state().content;
    let window = content.windows.focused_window().unwrap();
    assert!(content.shows_page(window, b_id));
    assert!(!content.shows_page(window, a_id));
    let run = run.send(show_at(a.clone(), "/r/a"));
    let content = &run.state().content;
    assert!(content.shows_page(content.windows.focused_window().unwrap(), a_id));
    assert!(!content.shows_page(window, b_id));
}

#[test]
fn browser_pages_close_independently() {
    let first = Tab::browser(here(), "/r");
    let second = Tab::browser(here(), "/r");
    let run = Scenario::new()
        .send(show(first.clone()))
        .send(show(second))
        .send(Msg::Focus(Area::Console));
    assert_eq!(shown(&run).len(), 2);
    let run = run.send(Msg::Window(Action::TabClose));
    assert_eq!(shown(&run), [first]);
}

#[test]
fn a_browser_opens_new_work_in_the_worktree_it_hangs_under() {
    let run = listed_with([session("a").cwd("/r")])
        .send(show(Tab::browser(here(), "/r")))
        .send(Msg::Focus(Area::Console));
    assert!(run.state().content.place.is_none());
    let run = run.send(Msg::Window(Action::ShellOpen));
    assert!(
        run.last_effects().iter().any(
            |e| matches!(e, Effect::StartTerminal(ask) if ask.place == Place::at(&here(), "/r"))
        )
    );
}

fn two_worktrees() -> Scenario {
    Scenario::new().send(opened(None)).event(
        snapshot().listed(
            listed()
                .session(session("a").cwd("/r/a"))
                .session(session("b").cwd("/r/b"))
                .terminal(shell(1, "/r/a"))
                .terminal(shell(2, "/r/b")),
        ),
    )
}

fn show_at(tab: Tab, worktree: &str) -> Msg {
    Msg::Show {
        thing: tab,
        place: place(worktree),
        how: None,
    }
}

fn tabs_shown(run: &Scenario) -> usize {
    run.state().content.windows.tabs().len()
}

#[test]
fn selecting_another_worktree_shows_its_own_layout_and_the_last_worktree_s_comes_back() {
    let run = two_worktrees()
        .send(show_at(Tab::Terminal(here(), 1), "/r/a"))
        .send(Msg::Window(Action::WindowVsplit))
        .send(show_at(session_tab("a"), "/r/a"))
        .send(Msg::Window(Action::WindowToTab));
    assert_eq!(tabs_shown(&run), 2);
    let run = run.send(show_at(Tab::Terminal(here(), 2), "/r/b"));
    // Worktree b has a layout of its own, which a's did not leak into.
    assert_eq!(shown(&run), [Tab::Terminal(here(), 2)]);
    assert_eq!(tabs_shown(&run), 1);
    let run = run.send(show_at(Tab::Terminal(here(), 1), "/r/a"));
    assert_eq!(tabs_shown(&run), 2);
    assert!(shown(&run).contains(&session_tab("a")));
    assert!(!shown(&run).contains(&Tab::Terminal(here(), 2)));
    // And b's comes back as it was left.
    let run = run.send(show_at(Tab::Terminal(here(), 2), "/r/b"));
    assert_eq!(tabs_shown(&run), 1);
}

#[test]
fn a_layout_comes_back_with_its_splits_and_the_window_focused() {
    let run = two_worktrees()
        .send(show_at(Tab::Terminal(here(), 1), "/r/a"))
        .send(Msg::Window(Action::WindowVsplit))
        .send(show_at(session_tab("a"), "/r/a"));
    let before = run.state().content.windows.clone();
    let run = run
        .send(show_at(Tab::Terminal(here(), 2), "/r/b"))
        .send(show_at(session_tab("a"), "/r/a"));
    assert_eq!(run.state().content.windows, before);
}

#[test]
fn selecting_the_worktree_already_selected_keeps_its_layout() {
    let run = two_worktrees()
        .send(show_at(Tab::Terminal(here(), 1), "/r/a"))
        .send(Msg::Window(Action::WindowVsplit))
        .send(show_at(session_tab("a"), "/r/a"));
    let before = run.state().content.windows.clone();
    let run = run.send(show_at(Tab::Terminal(here(), 1), "/r/a"));
    assert_eq!(tabs_shown(&run), before.tabs().len());
}

#[test]
fn what_a_layout_put_away_shows_still_counts_as_shown_to_the_host() {
    let run = two_worktrees()
        .send(show_at(Tab::Terminal(here(), 1), "/r/a"))
        .send(show_at(Tab::Terminal(here(), 2), "/r/b"));
    assert_eq!(shown(&run), [Tab::Terminal(here(), 2)]);
    let sight = crate::settle::sight(run.state(), &here());
    assert_eq!(sight.terminals, BTreeSet::from([1, 2]));
}

#[test]
fn a_terminal_that_ends_while_its_layout_is_put_away_is_not_in_it_when_it_comes_back() {
    let run = two_worktrees()
        .send(show_at(Tab::Terminal(here(), 1), "/r/a"))
        .send(Msg::Window(Action::WindowVsplit))
        .send(show_at(session_tab("a"), "/r/a"))
        .send(show_at(Tab::Terminal(here(), 2), "/r/b"))
        .send(Msg::ShellClosed {
            environment: here(),
            id: 1,
        })
        .send(show_at(session_tab("a"), "/r/a"));
    assert_eq!(shown(&run), [session_tab("a")]);
}

#[test]
fn a_worktree_with_no_layout_yet_shows_what_is_opened_in_it_alone() {
    let run = two_worktrees()
        .send(show_at(session_tab("a"), "/r/a"))
        .send(show_at(session_tab("b"), "/r/b"));
    assert_eq!(shown(&run), [session_tab("b")]);
    assert_eq!(tabs_shown(&run), 1);
}

#[test]
fn a_layout_of_a_forgotten_environment_is_not_brought_back() {
    let far = Tab::Session(there(), "f".into());
    let run = crate::testing::shows_there(two_worktrees(), [session("f").cwd("/r/f")])
        .send(Msg::Show {
            thing: far.clone(),
            place: Some(Place::at(&there(), "/r/f")),
            how: None,
        })
        .send(show_at(Tab::Terminal(here(), 2), "/r/b"))
        .send(Msg::EnvironmentRemoved(there()))
        .send(Msg::Show {
            thing: Tab::Terminal(here(), 1),
            place: place("/r/a"),
            how: None,
        });
    assert_eq!(shown(&run), [Tab::Terminal(here(), 1)]);
    assert!(!run.state().content.shows_in_any_layout(&far));
}

#[test]
fn a_layout_holding_a_file_that_is_gone_comes_back_without_failing() {
    let gone = Tab::Viewer(here(), "/r/a/removed.png".into());
    let run = two_worktrees()
        .send(show_at(gone.clone(), "/r/a"))
        .send(show_at(Tab::Terminal(here(), 2), "/r/b"))
        .send(show_at(Tab::Terminal(here(), 1), "/r/a"));
    assert_eq!(shown(&run), [gone, Tab::Terminal(here(), 1)]);
}

fn viewer(name: &str) -> Tab {
    Tab::Viewer(here(), format!("/w/{name}").into())
}

fn three_kinds() -> Scenario {
    Scenario::new()
        .send(show(viewer("a.png")))
        .send(show(Tab::Terminal(here(), 1)))
        .send(show(viewer("b.png")))
        .send(show(Tab::Terminal(here(), 2)))
        .send(show(viewer("c.png")))
        .send(Msg::Focus(Area::Console))
}

#[test]
fn the_next_and_previous_tab_of_the_same_kind_pass_the_tabs_of_other_kinds() {
    let run = three_kinds().send(Msg::Window(Action::TabNextSame));
    assert_eq!(run.state().content.active(), Some(&viewer("a.png")));
    let run = run.send(Msg::Window(Action::TabNextSame));
    assert_eq!(run.state().content.active(), Some(&viewer("b.png")));
    let run = run
        .send(Msg::Window(Action::TabPrevSame))
        .send(Msg::Window(Action::TabPrevSame));
    assert_eq!(run.state().content.active(), Some(&viewer("c.png")));
}

#[test]
fn closing_the_file_tabs_leaves_what_is_not_a_file_open() {
    let run = three_kinds().send(Msg::Window(Action::TabCloseFiles));
    assert_eq!(
        shown(&run),
        [Tab::Terminal(here(), 1), Tab::Terminal(here(), 2)]
    );
    assert_eq!(
        run.state().content.active(),
        Some(&Tab::Terminal(here(), 2))
    );
}

#[test]
fn closing_the_file_tabs_closes_the_file_windows_of_a_tab_that_holds_others() {
    let run = three_kinds()
        .send(Msg::Window(Action::WindowVsplit))
        .send(show(Tab::Terminal(here(), 9)))
        .send(Msg::Window(Action::TabCloseFiles));
    assert!(shown(&run).contains(&Tab::Terminal(here(), 9)));
    assert!(!shown(&run).iter().any(|t| matches!(t, Tab::Viewer(..))));
}

fn shown(run: &Scenario) -> Vec<Tab> {
    run.state()
        .content
        .windows
        .shown()
        .into_iter()
        .cloned()
        .collect()
}

/// The window opened, having kept the content showing `kept` alone.
fn opened(kept: Option<Tab>) -> Msg {
    let mut windows = Content::default();
    if let Some(tab) = kept {
        windows.open(tab);
    }
    opened_with(&windows)
}

fn opened_with(windows: &Content<Tab>) -> Msg {
    Msg::ContentOpened {
        kept: windows.kept(kept_name),
        scratch: "/scratch".into(),
    }
}

/// The window as it stands once the first listing came: what it kept
/// had its chance to come back.
fn listed_with(sessions: impl IntoIterator<Item = SessionBuilder>) -> Scenario {
    Scenario::new().send(opened(None)).event(listing(sessions))
}

fn live(ids: &[&str]) -> Msg {
    Msg::PanesLive(ids.iter().map(|id| (here(), id.to_string())).collect())
}

#[test]
fn conversation_is_a_session_s_display_and_keeps_its_tab_and_prompt() {
    let a = session_tab("a");
    let b = session_tab("b");
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(a.clone()))
        .send(Msg::SessionConversation {
            tab: a.clone(),
            shown: true,
        });
    assert_eq!(active(&run), Some(a.clone()));
    assert_eq!(shown(&run), std::slice::from_ref(&a));
    assert!(shows_prompt(run.state().content.active()));
    assert!(run.state().content.conversation(&a));
    let run = run.send(show(b.clone()));
    assert!(!run.state().content.conversation(&b));
    let run = run.send(show(a.clone()));
    assert!(run.state().content.conversation(&a));
    assert!(
        crate::settle::sight(run.state(), &here())
            .conversations
            .contains("a")
    );
    let run = run.send(Msg::SessionConversation {
        tab: a.clone(),
        shown: false,
    });
    assert!(!run.state().content.conversation(&a));
    assert!(
        crate::settle::sight(run.state(), &here())
            .conversations
            .is_empty()
    );
}

#[test]
fn conversation_is_not_shown_after_its_session_continues_in_another_provider_thread() {
    let tab = session_tab("a");
    let run = listed_with([session("a").session_id("old")])
        .send(show(tab.clone()))
        .tell(tsumugi_protocol::DetailsChange::Transcript {
            session: "a".into(),
            transcript: tsumugi_protocol::TranscriptRead {
                id: "old".into(),
                messages: vec![],
            },
        });
    assert!(conversation_read(&run.state().hosts, &tab).is_some());
    let run = run.tell(tsumugi_protocol::TreeChange::Listed {
        listed: Box::new(listed().session(session("a").session_id("new")).build()),
    });
    assert!(conversation_read(&run.state().hosts, &tab).is_none());
}

#[test]
fn a_window_follows_its_session_once_it_is_taken_over() {
    let own = session("own").cwd("/r").session_id("pid-1");
    let run = listed_with([session("pid-1").cwd("/r")])
        .send(show(session_tab("pid-1")))
        .event(patch(2).listed(listed().session(own)));
    assert_eq!(shown(&run), [Tab::Session(here(), "own".into())]);
}

#[test]
fn a_session_that_ends_gives_its_windows_up_and_is_said_to_have_ended() {
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(session_tab("a")))
        .event(patch(2).listed(listed().session(session("b").cwd("/r"))));
    // The worktree's first session shows once nothing else is open.
    assert_eq!(shown(&run), [Tab::Session(here(), "b".into())]);
    assert_eq!(
        run.state().chrome.status.line.as_deref(),
        Some("a-name ended")
    );
}

#[test]
fn a_session_live_here_stays_though_it_is_not_listed() {
    // (what holds it, whether it stays shown)
    let cases: Vec<(Vec<Msg>, bool)> = vec![(vec![], false), (vec![live(&["a"])], true)];
    for (held, stays) in cases {
        let run = listed_with([session("a").cwd("/r")])
            .send(show(session_tab("a")))
            .send_all(held.clone())
            .event(patch(2).listed(listed()));
        assert_eq!(
            shown(&run).contains(&Tab::Session(here(), "a".into())),
            stays,
            "{held:?}"
        );
    }
}

#[test]
fn a_session_no_longer_live_is_given_up_and_said_to_have_ended_once_told() {
    let run = listed_with([session("a").cwd("/r")])
        .send(show(session_tab("a")))
        .send(live(&["a"]))
        .event(patch(2).listed(listed()));
    assert_eq!(shown(&run), [Tab::Session(here(), "a".into())]);
    let gone = run.send(live(&[]));
    assert_eq!(shown(&gone), Vec::<Tab>::new());
    assert_eq!(
        gone.state().chrome.status.line.as_deref(),
        Some("a-name ended")
    );
}

#[test]
fn the_panes_kept_are_those_of_what_a_new_listing_lists() {
    let run = listed_with([session("a").cwd("/r")]);
    let kept = run
        .last_effects()
        .iter()
        .filter(|e| matches!(e, Effect::KeepPanes { .. }))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        kept,
        [Effect::KeepPanes {
            environment: here(),
            sessions: BTreeSet::from(["a".to_string()]),
            terminals: BTreeSet::new(),
        }]
    );
    // A pane started since the listing stays until the host lists anew.
    let told = run.send(live(&["new"]));
    assert!(
        !told
            .last_effects()
            .iter()
            .any(|e| matches!(e, Effect::KeepPanes { .. }))
    );
}

#[test]
fn the_tab_kept_from_the_last_run_comes_back_once_listed() {
    let a = Tab::Session(here(), "a".into());
    // (opened before the listing, what shows after it)
    let cases = [
        (None, Some(a.clone())),
        (
            Some(Tab::Session(here(), "b".into())),
            Some(Tab::Session(here(), "b".into())),
        ),
    ];
    for (before, after) in cases {
        let run = Scenario::new()
            .send(opened(Some(a.clone())))
            .send_all(before.clone().map(show))
            .event(listing([session("a"), session("b")]));
        assert_eq!(active(&run), after, "{before:?}");
        if before.is_none() {
            assert_eq!(run.state().content.place, place("/r/a"));
            assert_eq!(
                run.state().tree.selected,
                Some(Item::Gathered(Box::new(Item::Session(here(), "a".into()))))
            );
        }
    }
}

#[test]
fn a_content_kept_before_environments_comes_back_in_the_window_s_environment() {
    let here = EnvironmentId::new("here");
    let earlier = r#"{"tabs":[{"root":{"window":"session:a"},"focus":0}],"current":0}"#;
    let run = Scenario::new()
        .send(Msg::Opened {
            home: "/home".into(),
            environment: here.clone(),
        })
        .send(Msg::ContentOpened {
            kept: read_kept(earlier),
            scratch: "/scratch".into(),
        })
        .event(listing([session("a")]));
    assert_eq!(active(&run), Some(Tab::Session(here.clone(), "a".into())));
    assert_eq!(
        run.state().tree.selected,
        Some(Item::Gathered(Box::new(Item::Session(here, "a".into()))))
    );
}

#[test]
fn a_kept_tab_comes_back_when_the_sessions_were_listed_before_the_window_opened() {
    let kept = [Tab::Session(here(), "a".into()), Tab::Terminal(here(), 7)];
    for tab in kept {
        let run = Scenario::new()
            .event(snapshot().listed(listed().session(session("a")).terminal(shell(7, "/r/a"))))
            .send(opened(Some(tab.clone())));
        assert_eq!(active(&run), Some(tab.clone()), "{tab:?}");
    }
}

#[test]
fn the_prompt_of_a_kept_tab_that_comes_back_is_that_session_s() {
    let run = Scenario::new()
        .event(snapshot().listed(listed().session(session("a"))))
        .send(opened(Some(Tab::Session(here(), "a".into()))));
    let told: Vec<&Option<Tab>> = run
        .effects()
        .iter()
        .filter_map(|e| match e {
            Effect::PromptShow(tab) => Some(tab),
            _ => None,
        })
        .collect();
    assert_eq!(told, [&Some(Tab::Session(here(), "a".into()))]);
}

#[test]
fn a_kept_tab_not_listed_at_first_is_given_up() {
    let run = Scenario::new()
        .send(opened(Some(Tab::Session(here(), "a".into()))))
        .event(snapshot().listed(listed()))
        .event(patch(2).listed(listed().session(session("a").cwd("/r"))));
    assert_eq!(active(&run), None);
}

fn kept_content(run: &Scenario) -> Vec<Kept<String>> {
    run.last_effects()
        .iter()
        .filter_map(|e| match e {
            Effect::KeepContent(k) => Some(k.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_content_is_kept_for_the_next_run_as_it_changes_once_the_kept_content_had_its_chance() {
    let shows = |run: &Scenario| vec![run.state().content.windows.kept(kept_name)];
    let before = Scenario::new()
        .send(opened(None))
        .send(show(session_tab("a")));
    assert!(
        !before
            .effects()
            .iter()
            .any(|e| matches!(e, Effect::KeepContent(_)))
    );
    let listed = before.event(listing([session("a").cwd("/r")]));
    assert_eq!(kept_content(&listed), shows(&listed));
    let split = listed.send(Msg::Window(Action::WindowVsplit));
    assert_eq!(kept_content(&split), shows(&split));
    assert_eq!(shows(&split)[0].tabs.len(), 1);
    let border = split.state().content.windows.current().unwrap().borders()[0].clone();
    let dragging = split
        .send(Msg::WindowDragStart {
            border,
            from: 500.0,
            px: 1000.0,
        })
        .send(Msg::WindowDragMove(600.0));
    assert!(kept_content(&dragging).is_empty(), "kept once dropped");
    let dropped = dragging.send(Msg::WindowDragEnd);
    assert_eq!(kept_content(&dropped), shows(&dropped));
    let closed = dropped.send(Msg::Window(Action::TabClose));
    assert_eq!(kept_content(&closed), [Kept::default()]);
}

#[test]
fn the_content_kept_from_the_last_run_comes_back_once_listed_without_what_cannot() {
    let mut was = Content::default();
    was.open(Tab::Session(here(), "a".into()));
    was.split(Axis::Row, Tab::Session(here(), "gone".into()));
    was.split(Axis::Column, Tab::Terminal(here(), 7));
    was.open_tab(Tab::Session(here(), "b".into()));
    was.open_tab(Tab::Session(here(), "gone".into()));
    was.go_to_tab(0);
    let run = Scenario::new().send(opened_with(&was)).event(
        snapshot().listed(
            listed()
                .sessions([session("a"), session("b")])
                .terminal(shell(7, "/r/a")),
        ),
    );
    let mut ended = was.clone();
    ended.close_showing(|t| *t == Tab::Session(here(), "gone".into()));
    let windows = &run.state().content.windows;
    assert_eq!(windows.kept(kept_name), ended.kept(kept_name));
    assert_eq!(windows.tabs().len(), 2);
    assert_eq!(active(&run), Some(Tab::Terminal(here(), 7)));
    assert_eq!(run.state().content.place, place("/r/a"));
}

#[test]
fn a_kept_content_of_which_nothing_can_come_back_opens_empty() {
    let mut was = Content::default();
    was.open(Tab::Session(here(), "gone".into()));
    was.open_tab(Tab::Terminal(here(), 9));
    let run = Scenario::new()
        .send(opened_with(&was))
        .event(listing([session("a")]));
    assert!(run.state().content.windows.is_empty());
}

#[test]
fn a_session_that_starts_waiting_is_told_of_after_the_first_listing() {
    let waits = session("a").cwd("/r").waiting(Wait::Permission);
    let notifying = || {
        Scenario::new().event(crate::config::testing::configured(
            "notify_waiting = true\n",
        ))
    };
    let first = notifying()
        .send(opened(None))
        .event(listing([waits.clone()]));
    assert!(
        !first
            .effects()
            .iter()
            .any(|e| matches!(e, Effect::NotifyWaiting(_)))
    );
    let later = notifying()
        .send(opened(None))
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).listed(listed().session(waits)));
    assert_eq!(
        later
            .last_effects()
            .iter()
            .filter(|e| matches!(e, Effect::NotifyWaiting(_)))
            .cloned()
            .collect::<Vec<_>>(),
        [Effect::NotifyWaiting(vec![(
            "a-name".into(),
            "permission prompt".into()
        )])]
    );
}

#[test]
fn a_session_starting_to_wait_in_a_muted_workspace_is_not_told_of_but_still_shows_it() {
    let waits = session("a").cwd("/r").waiting(Wait::Permission);
    let run = Scenario::new()
        .send(opened(None))
        .send(Msg::NotificationsMute {
            environment: here(),
            path: "/r".into(),
        })
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).listed(listed().session(waits)));
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::NotifyWaiting(_)))
    );
    let state = run.state();
    let tree = state.hosts.own().work.as_ref().unwrap().tree.tree();
    let (_, _, a) = tree.find("a").unwrap();
    assert!(matches!(
        state.tree.completions.state(a),
        tsumugi_model::State::Waiting(_)
    ));
}

#[test]
fn the_middle_follows_what_the_focused_window_shows() {
    let run = listed_with([session("a"), session("b")]).send(show(session_tab("a")));
    assert_eq!(run.state().content.place, place("/r/a"));
    let b = run.send(show(session_tab("b")));
    assert_eq!(b.state().content.place, place("/r/b"));
    // A tab whose thing is placed nowhere leaves the middle where it is.
    let nowhere = b.send(show(Tab::Viewer(here(), "/elsewhere/a.png".into())));
    assert_eq!(nowhere.state().content.place, place("/r/b"));
}

#[test]
fn a_session_shown_before_it_is_listed_is_placed_once_it_is() {
    let run = listed_with([])
        .send(Msg::Show {
            thing: Tab::Session(here(), "new".into()),
            place: place("/r"),
            how: None,
        })
        .event(patch(2).listed(listed().session(session("new").cwd("/r/w"))));
    assert_eq!(run.state().content.place, place("/r/w"));
}

#[test]
fn the_session_last_shown_in_a_workspace_is_remembered() {
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(session_tab("a")))
        .send(show(session_tab("b")));
    assert_eq!(
        run.state()
            .content
            .last_shown
            .get(std::path::Path::new("/r")),
        Some(&"b".to_string())
    );
}

#[test]
fn a_draft_no_window_shows_is_given_up_with_its_options() {
    let run = Scenario::new().send(Msg::Draft {
        worktree: "/w".into(),
        write: false,
    });
    let draft = active(&run).unwrap();
    assert!(run.state().content.drafts.options.contains_key(&draft));
    let closed = run.send(Msg::Window(Action::TabClose));
    assert!(closed.state().content.drafts.options.is_empty());
}

#[test]
fn a_draft_takes_the_options_chosen_with_nothing_open() {
    let chosen = DraftOptions {
        model: Some("m".into()),
        ..Default::default()
    };
    let run = Scenario::new()
        .send(Msg::DraftOptions {
            tab: None,
            options: chosen.clone(),
        })
        .send(Msg::Draft {
            worktree: "/w".into(),
            write: true,
        });
    let draft = active(&run).unwrap();
    assert_eq!(
        run.state().content.drafts.options.get(&draft),
        Some(&chosen)
    );
    assert_eq!(run.state().content.drafts.next, None);
    assert_eq!(run.state().content.focus, Area::Prompt);
    assert!(run.last_effects().contains(&Effect::StartWriting));
    assert_eq!(run.state().content.place, place("/w"));
}

#[test]
fn closing_the_last_window_of_a_draft_hands_its_prompt_on() {
    // (shown besides the draft, where its prompt goes)
    let cases = [
        (None, None),
        (
            Some(Tab::Session(here(), "a".into())),
            Some(Tab::Session(here(), "a".into())),
        ),
        (Some(Tab::Terminal(here(), 1)), None),
    ];
    for (besides, to) in cases {
        let run = Scenario::new()
            .send_all(besides.clone().map(show))
            .send_all(besides.iter().map(|_| Msg::Window(Action::WindowSplit)))
            .send(Msg::Draft {
                worktree: "/w".into(),
                write: false,
            })
            .send(Msg::Window(Action::WindowQuit));
        assert!(
            run.last_effects()
                .contains(&Effect::PromptClose(to.clone())),
            "{besides:?}: {:?}",
            run.last_effects()
        );
    }
}

#[test]
fn hiding_the_panel_with_the_keys_gives_them_to_the_console() {
    // (the keys' area, the panel hidden, where the keys are after)
    let cases = [
        (Area::Sidebar, Side::Sidebar, Area::Console),
        (Area::Details, Side::Details, Area::Console),
        (Area::Sidebar, Side::Details, Area::Sidebar),
        (Area::Prompt, Side::Sidebar, Area::Prompt),
    ];
    for (area, side, after) in cases {
        let run = Scenario::new()
            .send(Msg::Focus(area))
            .send(Msg::PanelToggle(side));
        assert_eq!(run.state().content.focus, after, "{area:?} {side:?}");
    }
}

#[test]
fn a_panel_hidden_from_the_start_leaves_the_keys_where_they_are() {
    let layout = crate::chrome::panels::layout::Layout {
        show_sidebar: false,
        ..Default::default()
    };
    let run = Scenario::new().send(Msg::PanelsKept(layout));
    assert_eq!(run.state().content.focus, Area::Sidebar);
}

#[test]
fn the_focused_window_is_marked_only_while_the_content_has_the_keys() {
    let shown = || Scenario::new().send(show(session_tab("a")));
    let window = shown().state().content.windows.focused_window();
    assert!(window.is_some());
    // (the keys' area, whether the focused window is marked)
    let cases = [
        (Area::Sidebar, false),
        (Area::Details, false),
        (Area::Console, true),
        (Area::Prompt, true),
    ];
    for (area, marked) in cases {
        let at = shown().send(Msg::Focus(area));
        let expected = if marked { window } else { None };
        assert_eq!(at.state().content.marked_window(), expected, "{area:?}");
    }
    // Coming back from the tree marks the window the keys left.
    let back = shown()
        .send(Msg::Focus(Area::Sidebar))
        .send(Msg::Focus(Area::Console));
    assert_eq!(back.state().content.marked_window(), window);
}

#[test]
fn a_shell_shown_takes_the_keys_from_the_prompt() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Prompt))
        .send(show(Tab::Terminal(here(), 1)));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn coming_into_the_prompt_asks_the_console_for_what_was_typed() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console));
    assert!(!run.last_effects().contains(&Effect::ConsoleLive));
    let into = run.send(Msg::Focus(Area::Prompt));
    assert!(into.last_effects().contains(&Effect::ConsoleLive));
    let again = into.send(Msg::Focus(Area::Prompt));
    assert!(!again.last_effects().contains(&Effect::ConsoleLive));
}

#[test]
fn the_window_back_key_returns_to_where_the_keys_were() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console))
        .send(Msg::Focus(Area::Details))
        .send(Msg::Window(Action::WindowBack));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn the_window_keys_leave_a_hidden_panel_alone() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Console))
        .send(Msg::PanelToggle(Side::Sidebar))
        .send(Msg::Window(Action::WindowLeft));
    assert_eq!(run.state().content.focus, Area::Console);
}

#[test]
fn the_window_keys_split_close_and_go_through_tabs() {
    let a = Tab::Session(here(), "a".into());
    let split = || {
        Scenario::new()
            .send(show(a.clone()))
            .send(Msg::Window(Action::WindowVsplit))
    };
    let two = split();
    assert_eq!(
        two.state()
            .content
            .windows
            .current()
            .unwrap()
            .windows()
            .len(),
        2
    );
    let one = split().send(Msg::Window(Action::WindowOnly));
    assert_eq!(
        one.state()
            .content
            .windows
            .current()
            .unwrap()
            .windows()
            .len(),
        1
    );
    let tabs = two
        .send(Msg::Window(Action::WindowToTab))
        .send(Msg::Window(Action::TabGo(1)));
    assert_eq!(tabs.state().content.windows.current_index(), 0);
    let next = tabs.send(Msg::Window(Action::TabNext));
    assert_eq!(next.state().content.windows.current_index(), 1);
}

#[test]
fn the_tab_keys_step_the_details_sheets_while_the_details_have_the_keys() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Focus(Area::Details))
        .send(Msg::Window(Action::TabNext));
    assert_eq!(run.state().details.drill.tab(), CHANGES);
}

#[test]
fn a_new_window_splits_off_a_draft_in_the_middle_s_worktree() {
    // (the middle, where the draft runs)
    let cases = [(place("/w"), "/w"), (None, "/scratch")];
    for (middle, at) in cases {
        let run = Scenario::new()
            .send(opened(None))
            .send(Msg::Show {
                thing: Tab::Terminal(here(), 1),
                place: middle.clone(),
                how: None,
            })
            .send(Msg::Window(Action::WindowNew));
        let draft = active(&run);
        assert!(
            matches!(&draft, Some(Tab::Draft { worktree, .. }) if worktree == std::path::Path::new(at)),
            "{middle:?}: {draft:?}"
        );
        assert_eq!(run.state().content.windows.shown().len(), 2);
        assert_eq!(run.state().content.focus, Area::Prompt);
    }
}

#[test]
fn a_new_session_or_shell_starts_in_the_selected_row_s_worktree_from_the_tree() {
    let base = || {
        listed_with([session("a"), session("b")])
            .send(show(session_tab("a")))
            .send(Msg::TreeSelect(Some(Item::Session(here(), "b".into()))))
    };
    // (where the keys are, the worktree)
    let cases = [(Area::Sidebar, "/r/b"), (Area::Console, "/r/a")];
    for (area, at) in cases {
        let shell = base()
            .send(Msg::Focus(area))
            .send(Msg::Window(Action::ShellOpen));
        assert_eq!(
            shell.last_effects(),
            [Effect::StartTerminal(TerminalAsk {
                place: place(at).unwrap(),
                runs: TerminalProgram::Shell,
                file: None,
                opening: Opening::Existing,
                focus: true,
            })],
            "{area:?}"
        );
        let new = base()
            .send(Msg::Focus(area))
            .send(Msg::Window(Action::SessionNew));
        assert!(
            matches!(active(&new), Some(Tab::Draft { worktree, .. }) if worktree == std::path::Path::new(at)),
            "{area:?}"
        );
    }
}

#[test]
fn opening_a_row_shows_what_it_stands_for_and_may_take_the_keys() {
    let run = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item: Item::Session(here(), "a".into()),
        how: Some(Opening::Existing),
        take_keys: false,
    });
    assert_eq!(active(&run), Some(Tab::Session(here(), "a".into())));
    assert_eq!(run.state().content.focus, Area::Sidebar);
    assert_eq!(
        run.state().tree.selected,
        Some(Item::Session(here(), "a".into()))
    );
    let keys = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item: Item::Session(here(), "a".into()),
        how: Some(Opening::Existing),
        take_keys: true,
    });
    assert_eq!(keys.state().content.focus, Area::Console);
}

#[test]
fn a_row_clicked_opens_as_cursor_go_opens_it() {
    let item = Item::Session(here(), "a".into());
    let clicked = listed_with([session("a").cwd("/r")]).send(Msg::RowClicked(item.clone()));
    let gone = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item,
        how: Some(Opening::Existing),
        take_keys: true,
    });
    assert_eq!(active(&clicked), active(&gone));
    assert_eq!(clicked.state().content.focus, Area::Console);
    assert_eq!(clicked.last_effects(), gone.last_effects());
}

#[test]
fn a_worktree_s_row_clicked_folds_and_unfolds_it_and_opens_nothing() {
    let run = listed_with([session("a").cwd("/r")]);
    let row = Item::Worktree(
        here(),
        crate::tree::listed(run.state().hosts.work(&here()))
            .worktrees()
            .next()
            .unwrap()
            .worktree
            .path
            .clone(),
    );
    let unfolded = run.send(Msg::RowClicked(row.clone()));
    assert!(unfolded.state().tree.folds.is_open(&row));
    let folded = unfolded.send(Msg::RowClicked(row.clone()));
    assert!(!folded.state().tree.folds.is_open(&row));
    assert_eq!(active(&folded), None);
    assert_eq!(folded.state().tree.selected, Some(row));
}

#[test]
fn going_into_an_unfolded_worktree_s_row_opens_its_session() {
    let run = listed_with([session("a").cwd("/r")]);
    let row = Item::Worktree(
        here(),
        crate::tree::listed(run.state().hosts.work(&here()))
            .worktrees()
            .next()
            .unwrap()
            .worktree
            .path
            .clone(),
    );
    let gone = run
        .send(Msg::TreeFold {
            row: row.clone(),
            open: true,
        })
        .send(Msg::OpenRow {
            item: row,
            how: Some(Opening::Existing),
            take_keys: true,
        });
    assert_eq!(active(&gone), Some(Tab::Session(here(), "a".into())));
}

#[test]
fn opening_a_file_s_row_starts_a_neovim_on_it() {
    let file = Item::File {
        environment: here(),
        worktree: "/r".into(),
        path: "/r/a.rs".into(),
        dir: false,
    };
    let run = listed_with([session("a").cwd("/r")]).send(Msg::OpenRow {
        item: file,
        how: Some(Opening::Tab),
        take_keys: true,
    });
    assert_eq!(
        run.last_effects()
            .iter()
            .filter(|e| matches!(e, Effect::StartTerminal(_)))
            .cloned()
            .collect::<Vec<_>>(),
        [Effect::StartTerminal(TerminalAsk {
            place: place("/r").unwrap(),
            runs: TerminalProgram::Neovim,
            file: Some("/r/a.rs".into()),
            opening: Opening::Tab,
            focus: true,
        })]
    );
}

#[test]
fn a_session_is_shown_wherever_it_runs_and_its_row_selected() {
    let run = listed_with([session("a"), session("b")])
        .send(show(session_tab("a")))
        .send(Msg::ShowSession(here(), "b".into()));
    assert_eq!(active(&run), Some(Tab::Session(here(), "b".into())));
    assert_eq!(run.state().content.place, place("/r/b"));
    assert_eq!(
        run.state().tree.selected,
        Some(Item::Gathered(Box::new(Item::Session(here(), "b".into()))))
    );
    let next = run.send(Msg::Window(Action::SessionNext));
    assert_eq!(active(&next), Some(Tab::Session(here(), "a".into())));
}

#[test]
fn a_closed_shell_gives_its_windows_up_to_another_thing_of_its_worktree() {
    let run = Scenario::new()
        .send(opened(None))
        .event(
            snapshot().listed(
                listed()
                    .session(session("a").cwd("/r"))
                    .terminal(shell(1, "/r")),
            ),
        )
        .send(Msg::Show {
            thing: Tab::Terminal(here(), 1),
            place: place("/r"),
            how: None,
        })
        .send(Msg::ShellClosed {
            environment: here(),
            id: 1,
        });
    assert_eq!(active(&run), Some(Tab::Session(here(), "a".into())));
    assert!(run.last_effects().contains(&Effect::Host {
        environment: here(),
        request: Request::TerminalClosed { id: 1 },
    }));
    assert!(
        run.last_effects()
            .contains(&Effect::PromptClose(Some(Tab::Session(here(), "a".into()))))
    );
}

#[test]
fn a_terminal_removed_from_the_listing_closes_its_background_tabs_and_splits() {
    let run = Scenario::new()
        .send(opened(None))
        .event(snapshot().listed(listed().session(session("a")).terminal(shell(7, "/r"))))
        .event_from(
            there(),
            snapshot().listed(listed().terminal(shell(7, "/other"))),
        )
        .send(show(Tab::Terminal(here(), 7)))
        .send(Msg::Window(Action::WindowVsplit))
        .send(Msg::Show {
            thing: session_tab("a"),
            place: None,
            how: Some(Opening::Tab),
        })
        .send(Msg::Show {
            thing: Tab::Terminal(there(), 7),
            place: None,
            how: Some(Opening::Tab),
        });
    let run = run.event(patch(2).listed(listed().session(session("a"))));
    assert!(!shown(&run).contains(&Tab::Terminal(here(), 7)));
    assert!(shown(&run).contains(&session_tab("a")));
    assert_eq!(active(&run), Some(Tab::Terminal(there(), 7)));
}

#[test]
fn a_terminal_missing_after_reconnect_gives_its_windows_to_its_worktree() {
    let run = Scenario::new()
        .send(opened(None))
        .event(
            snapshot().listed(
                listed()
                    .session(session("a").cwd("/r"))
                    .terminal(shell(7, "/r")),
            ),
        )
        .send(show(Tab::Terminal(here(), 7)))
        .event(
            snapshot()
                .seq(2)
                .listed(listed().session(session("a").cwd("/r"))),
        );
    assert_eq!(shown(&run), [session_tab("a")]);
}

#[test]
fn a_successful_command_closes_its_windows_and_keeps_its_output_available() {
    let mut command = shell(7, "/r");
    command.program = TerminalProgram::Command;
    let run = Scenario::new()
        .send(opened(None))
        .event(snapshot().listed(listed().terminal(command.clone())))
        .send(show(Tab::Terminal(here(), 7)));
    command.exit = Some(0);
    let run = run.event(patch(2).listed(listed().terminal(command.clone())));
    assert!(shown(&run).is_empty());
    assert!(
        run.state()
            .hosts
            .work(&here())
            .unwrap()
            .tree
            .listed
            .as_ref()
            .unwrap()
            .terminals
            .iter()
            .any(|t| t.id == 7 && t.exit == Some(0))
    );
    assert!(!run.last_effects().iter().any(|e| matches!(
        e,
        Effect::Host {
            request: Request::EndTerminal { .. } | Request::TerminalClosed { .. },
            ..
        }
    )));
    let run = run
        .send(show(Tab::Terminal(here(), 7)))
        .event(patch(3).listed(listed().terminal(command)));
    assert_eq!(shown(&run), [Tab::Terminal(here(), 7)]);
    let run = run.event(patch(4).listed(listed()));
    assert!(shown(&run).is_empty());
}

#[test]
fn successful_commands_keep_their_background_tabs_and_splits_when_auto_close_is_off() {
    let mut command = shell(7, "/r");
    command.program = TerminalProgram::Command;
    let run = Scenario::new()
        .send(opened(None))
        .event(
            snapshot()
                .listed(listed().terminal(command.clone()))
                .with(|w| {
                    w.config.settings = Some(crate::config::testing::settings_file(
                        "close_successful_commands = false\n",
                    ));
                }),
        )
        .send(show(Tab::Terminal(here(), 7)))
        .send(Msg::Window(Action::WindowVsplit))
        .send(Msg::Show {
            thing: Tab::draft(here(), "/r"),
            place: None,
            how: Some(Opening::Tab),
        });
    let before = run.state().content.windows.clone();
    command.exit = Some(0);
    let run = run.event(patch(2).listed(listed().terminal(command)));
    assert_eq!(run.state().content.windows, before);
    assert!(!run.last_effects().iter().any(|e| matches!(
        e,
        Effect::Host {
            request: Request::EndTerminal { .. } | Request::TerminalClosed { .. },
            ..
        }
    )));
    let run = run.event(patch(3).listed(listed()));
    assert!(!shown(&run).contains(&Tab::Terminal(here(), 7)));
}

#[test]
fn each_environments_commands_follow_its_own_auto_close_setting() {
    for keep_in in [here(), there()] {
        let mut command = shell(7, "/r");
        command.program = TerminalProgram::Command;
        let mut run = Scenario::new().send(opened(None));
        for environment in [here(), there()] {
            let on = environment != keep_in;
            run = run
                .event_from(
                    environment.clone(),
                    snapshot()
                        .listed(listed().terminal(command.clone()))
                        .with(|w| {
                            w.config.settings = Some(crate::config::testing::settings_file(
                                &format!("close_successful_commands = {on}\n"),
                            ));
                        }),
                )
                .send(Msg::Show {
                    thing: Tab::Terminal(environment, 7),
                    place: None,
                    how: Some(Opening::Tab),
                });
        }
        command.exit = Some(0);
        for environment in [here(), there()] {
            run = run.event_from(
                environment,
                patch(2).listed(listed().terminal(command.clone())),
            );
        }
        assert_eq!(shown(&run), [Tab::Terminal(keep_in, 7)]);
    }
}

#[test]
fn a_failed_command_keeps_its_tab_until_the_terminal_is_stopped() {
    for code in [1, 3, 143] {
        let mut command = shell(7, "/r");
        command.program = TerminalProgram::Command;
        let run = Scenario::new()
            .send(opened(None))
            .event(snapshot().listed(listed().terminal(command.clone())))
            .send(show(Tab::Terminal(here(), 7)));
        command.exit = Some(code);
        let run = run.event(patch(2).listed(listed().terminal(command)));
        assert_eq!(shown(&run), [Tab::Terminal(here(), 7)]);
        let run = run.event(patch(3).listed(listed()));
        assert!(shown(&run).is_empty());
    }
}

#[test]
fn successful_commands_close_background_tabs_and_splits_only_in_their_environment() {
    let mut command = shell(7, "/r");
    command.program = TerminalProgram::Command;
    let run = Scenario::new()
        .send(opened(None))
        .event(snapshot().listed(listed().terminal(command.clone())))
        .event_from(
            there(),
            snapshot().listed(listed().terminal(command.clone())),
        )
        .send(show(Tab::Terminal(here(), 7)))
        .send(Msg::Window(Action::WindowVsplit))
        .send(Msg::Show {
            thing: Tab::Terminal(there(), 7),
            place: None,
            how: Some(Opening::Tab),
        });
    command.exit = Some(0);
    let run = run.event(patch(2).listed(listed().terminal(command)));
    assert_eq!(shown(&run), [Tab::Terminal(there(), 7)]);
    assert_eq!(active(&run), Some(Tab::Terminal(there(), 7)));
}

#[test]
fn commands_finishing_together_do_not_reopen_each_other_as_fallbacks() {
    for show_second in [false, true] {
        let mut first = shell(7, "/r");
        first.program = TerminalProgram::Command;
        let mut second = shell(8, "/r");
        second.program = TerminalProgram::Command;
        let run = Scenario::new()
            .send(opened(None))
            .event(snapshot().listed(listed().terminals([first.clone(), second.clone()])))
            .send(Msg::Show {
                thing: Tab::Terminal(here(), 7),
                place: place("/r"),
                how: None,
            });
        let run = if show_second {
            run.send(Msg::Show {
                thing: Tab::Terminal(here(), 8),
                place: place("/r"),
                how: Some(Opening::Tab),
            })
        } else {
            run
        };
        first.exit = Some(0);
        second.exit = Some(0);
        let run = run.event(patch(2).listed(listed().terminals([first, second])));
        assert!(shown(&run).is_empty());
    }
}

#[test]
fn a_successful_command_returns_to_the_session_in_its_worktree() {
    let mut command = shell(7, "/r");
    command.program = TerminalProgram::Command;
    let run = Scenario::new()
        .send(opened(None))
        .event(
            snapshot().listed(
                listed()
                    .session(session("a").cwd("/r"))
                    .terminal(command.clone()),
            ),
        )
        .send(Msg::Show {
            thing: Tab::Terminal(here(), 7),
            place: place("/r"),
            how: None,
        });
    command.exit = Some(0);
    let run =
        run.event(patch(2).listed(listed().session(session("a").cwd("/r")).terminal(command)));
    assert_eq!(shown(&run), [session_tab("a")]);
}

#[test]
fn an_interrupted_terminal_keeps_its_tab_until_it_is_let_go() {
    let old = tsumugi_model::daemon::hex(7);
    let run = Scenario::new()
        .send(opened(None))
        .event(snapshot().listed(listed().terminal(shell(7, "/r"))))
        .send(show(Tab::Terminal(here(), 7)))
        .event(patch(2).listed(listed().interrupted(interrupted(&old))));
    assert_eq!(shown(&run), [Tab::Terminal(here(), 7)]);
    let run = run.event(patch(3).listed(listed()));
    assert!(shown(&run).is_empty());
}

#[test]
fn dragging_a_border_resizes_the_windows_beside_it() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Window(Action::WindowVsplit));
    let border = run.state().content.windows.current().unwrap().borders()[0].clone();
    let dragged = run
        .send(Msg::WindowDragStart {
            border: border.clone(),
            from: 500.0,
            px: 1000.0,
        })
        .send(Msg::WindowDragMove(600.0));
    let moved = dragged.state().content.windows.current().unwrap().borders()[0].at;
    assert!((moved - (border.at + 0.1)).abs() < 1e-9, "{moved}");
    let dropped = dragged.send(Msg::WindowDragEnd);
    assert_eq!(dropped.state().content.drag, None);
}

fn window_showing(run: &Scenario, name: &str) -> WindowId {
    let thing = Tab::Session(here(), name.into());
    run.state()
        .content
        .windows
        .tabs()
        .iter()
        .flat_map(|t| t.windows())
        .find(|w| w.shows == thing)
        .map(|w| w.id)
        .unwrap()
}

#[test]
fn a_window_dragged_onto_a_side_of_another_moves_there_once_let_go() {
    let run = Scenario::new()
        .send(opened(None))
        .send(show(session_tab("a")))
        .event(listing([session("a").cwd("/r"), session("b").cwd("/r")]))
        .send(Msg::Show {
            thing: Tab::Session(here(), "b".into()),
            place: place("/r"),
            how: Some(Opening::Tab),
        });
    let (a, b) = (window_showing(&run, "a"), window_showing(&run, "b"));
    let dragging = run
        .send(Msg::WindowMoveStart {
            window: b,
            tab: None,
            x: 10.0,
            y: 10.0,
        })
        .send(Msg::WindowMoveOver {
            x: 300.0,
            y: 10.0,
            over: Some(DropAt::Side(a, Dir::Left)),
        });
    assert_eq!(
        dragging.state().content.windows.tabs().len(),
        2,
        "moved once let go"
    );
    assert!(kept_content(&dragging).is_empty(), "kept once dropped");
    let dropped = dragging.send(Msg::WindowMoveEnd);
    let windows = &dropped.state().content.windows;
    assert_eq!(windows.tabs().len(), 1);
    assert_eq!(windows.focused(), Some(&Tab::Session(here(), "b".into())));
    assert_eq!(dropped.state().content.moving, None);
    assert_eq!(
        kept_content(&dropped),
        [dropped.state().content.windows.kept(kept_name)]
    );
}

#[test]
fn a_window_dragged_over_a_tab_shows_it_and_let_go_elsewhere_stays() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Show {
            thing: Tab::Session(here(), "b".into()),
            place: place("/w"),
            how: Some(Opening::Tab),
        });
    let b = window_showing(&run, "b");
    let over_tab = run
        .send(Msg::WindowMoveStart {
            window: b,
            tab: None,
            x: 10.0,
            y: 10.0,
        })
        .send(Msg::WindowMoveOver {
            x: 50.0,
            y: 10.0,
            over: Some(DropAt::Tab(0)),
        });
    assert_eq!(over_tab.state().content.windows.current_index(), 0);
    let away = over_tab
        .send(Msg::WindowMoveOver {
            x: 400.0,
            y: 400.0,
            over: None,
        })
        .send(Msg::WindowMoveEnd);
    assert_eq!(away.state().content.windows.tabs().len(), 2);
}

#[test]
fn a_tab_dragged_over_another_takes_its_place_without_showing_it() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Show {
            thing: Tab::Session(here(), "b".into()),
            place: place("/w"),
            how: Some(Opening::Tab),
        });
    let b = window_showing(&run, "b");
    let over = run
        .send(Msg::WindowMoveStart {
            window: b,
            tab: Some(1),
            x: 10.0,
            y: 10.0,
        })
        .send(Msg::WindowMoveOver {
            x: 50.0,
            y: 10.0,
            over: Some(DropAt::Tab(0)),
        });
    assert_eq!(over.state().content.windows.current_index(), 1);
    let dropped = over.send(Msg::WindowMoveEnd);
    let windows = &dropped.state().content.windows;
    assert_eq!(windows.tabs().len(), 2);
    assert_eq!(windows.current_index(), 0);
    assert_eq!(windows.focused(), Some(&Tab::Session(here(), "b".into())));
}

#[test]
fn a_window_pressed_and_let_go_without_moving_stays() {
    let run = Scenario::new()
        .send(show(session_tab("a")))
        .send(Msg::Window(Action::WindowVsplit));
    let id = run.state().content.windows.focused_window().unwrap();
    let a = run.state().content.windows.current().unwrap().windows()[0].id;
    let clicked = run
        .send(Msg::WindowMoveStart {
            window: id,
            tab: None,
            x: 10.0,
            y: 10.0,
        })
        .send(Msg::WindowMoveOver {
            x: 11.0,
            y: 10.0,
            over: Some(DropAt::Side(a, Dir::Left)),
        })
        .send(Msg::WindowMoveEnd);
    assert_eq!(
        clicked.state().content.windows.current().unwrap().windows()[1].id,
        id
    );
}

#[test]
fn the_details_describe_the_focused_window_s_worktree_and_nothing_while_nothing_is_open() {
    let run = Scenario::new().send(Msg::Show {
        thing: Tab::Session(here(), "s".into()),
        place: place("/w"),
        how: None,
    });
    assert_eq!(run.state().content.described(), place("/w").as_ref());
    let closed = run.send(Msg::Window(Action::TabClose));
    assert_eq!(closed.state().content.described(), None);
}

#[test]
fn the_host_reads_the_worktree_in_the_middle_while_it_is_there() {
    let watched = |run: &Scenario| {
        run.effects()
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::Host {
                    request: Request::Watch(sight),
                    ..
                } => Some(sight.place.clone()),
                _ => None,
            })
            .flatten()
    };
    let run = Scenario::new().event(listing([session("a")]));
    assert_eq!(watched(&run), None);
    let run = run.send(show(session_tab("a")));
    assert_eq!(
        watched(&run),
        Some(tsumugi_protocol::HostPath::local("/r/a"))
    );
}

#[test]
fn the_host_is_told_the_terminals_any_window_shows_so_it_can_quit_those_none_shows() {
    let watched = |run: &Scenario| {
        run.effects()
            .iter()
            .rev()
            .find_map(|e| match e {
                Effect::Host {
                    request: Request::Watch(sight),
                    ..
                } => Some(sight.terminals.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    let terminal = |id| Msg::Show {
        thing: Tab::Terminal(here(), id),
        place: place("/r"),
        how: Some(Opening::Tab),
    };
    let run = Scenario::new()
        .send(terminal(1))
        .send(terminal(2))
        .send(show(session_tab("a")));
    assert_eq!(watched(&run), BTreeSet::from([1, 2]));
    let closed = run
        .send(Msg::Window(Action::TabPrev))
        .send(Msg::Window(Action::TabClose));
    assert_eq!(watched(&closed), BTreeSet::from([1]));
}

#[test]
fn a_session_that_starts_waiting_is_not_told_of_when_the_settings_say_so() {
    let waits = session("a").cwd("/r").waiting(Wait::Permission);
    let run = Scenario::new()
        .event(crate::config::testing::configured(
            "notify_waiting = false\n",
        ))
        .send(opened(None))
        .event(listing([session("a").cwd("/r")]))
        .event(patch(2).listed(listed().session(waits)));
    assert!(
        !run.effects()
            .iter()
            .any(|e| matches!(e, Effect::NotifyWaiting(_)))
    );
}

#[test]
fn a_prompt_goes_to_the_session_s_agent_or_that_of_the_profile_chosen_for_a_draft() {
    use tsumugi_model::Agent;
    let codex = session("c").cwd("/r").agent(Agent::Codex);
    let draft = |dir: &str| Tab::Draft {
        environment: here(),
        worktree: dir.into(),
        id: Default::default(),
    };
    let run = Scenario::new()
        .event(crate::config::testing::configured(
            "profile = \"review\"\n[[profiles]]\nid = \"review\"\nagent = \"codex\"\n[[profiles]]\nid = \"work\"\nagent = \"claude\"\n",
        ))
        .send(opened(None))
        .event(listing([codex]));
    let state = run.state();
    assert_eq!(
        agent_of(state, Some(&Tab::Session(here(), "c".into()))),
        Agent::Codex
    );
    assert_eq!(agent_of(state, Some(&draft("/r"))), Agent::Codex);
    assert_eq!(agent_of(state, None), Agent::Codex);
    let run = run.send(Msg::DraftProfile {
        tab: Some(draft("/r")),
        profile: "work".parse().unwrap(),
    });
    let state = run.state();
    assert_eq!(agent_of(state, Some(&draft("/r"))), Agent::Claude);
    assert_eq!(agent_of(state, Some(&draft("/other"))), Agent::Claude);
    assert_eq!(agent_of(state, None), Agent::Claude);
    assert_eq!(
        agent_of(state, Some(&Tab::Session(here(), "c".into()))),
        Agent::Codex
    );
}

#[test]
fn a_file_asked_to_be_edited_starts_a_neovim_on_it_in_its_folder() {
    let run = listed_with([session("a").cwd("/r")])
        .send(Msg::EditFile(here(), "/c/prompt/nvim/init.lua".into()));
    assert_eq!(
        run.last_effects(),
        [Effect::StartTerminal(TerminalAsk {
            place: place("/c/prompt/nvim").unwrap(),
            runs: TerminalProgram::Neovim,
            file: Some("/c/prompt/nvim/init.lua".into()),
            opening: Opening::Existing,
            focus: true,
        })]
    );
}

#[test]
fn a_viewer_of_a_work_machine_s_file_is_refused_and_one_of_this_computer_s_opens() {
    let run = shows_there(listed_with([session("a").cwd("/r")]), [session("b")])
        .send(Msg::OnWorkMachine(there()))
        .send(show(session_tab("a")));
    let refused = run.send(show(Tab::Viewer(there(), "/w/plot.png".into())));
    assert_eq!(active(&refused), Some(session_tab("a")));
    assert_eq!(
        refused.last_effects(),
        [Effect::Notice(
            "Remote file previews are unavailable".into()
        )]
    );
    let here_too = shows_there(listed_with([session("a").cwd("/r")]), [session("b")])
        .send(show(Tab::Viewer(there(), "/w/plot.png".into())));
    assert_eq!(
        active(&here_too),
        Some(Tab::Viewer(there(), "/w/plot.png".into()))
    );
}

#[test]
fn a_file_a_document_links_to_opens_by_its_kind_in_the_worktree_it_is_in() {
    let run = || listed_with([session("a").cwd("/r/a")]).send(show(session_tab("a")));
    let picture = run().send(Msg::OpenFile(here(), "/r/a/docs/shot.png".into()));
    assert_eq!(
        active(&picture),
        Some(Tab::Viewer(here(), "/r/a/docs/shot.png".into()))
    );
    assert_eq!(picture.state().content.place, place("/r/a"));
    let source = run().send(Msg::OpenFile(here(), "/r/a/src/main.rs".into()));
    assert_eq!(
        source.last_effects(),
        [Effect::StartTerminal(TerminalAsk {
            place: place("/r/a").unwrap(),
            runs: TerminalProgram::Neovim,
            file: Some("/r/a/src/main.rs".into()),
            opening: Opening::Existing,
            focus: true,
        })]
    );
    let elsewhere = run().send(Msg::OpenFile(here(), "/tmp/notes/b.rs".into()));
    assert!(elsewhere.last_effects().iter().any(|e| matches!(
        e,
        Effect::StartTerminal(TerminalAsk { place, .. }) if place.worktree == std::path::Path::new("/tmp/notes")
    )));
}

#[test]
fn a_file_opened_in_a_tab_or_a_split_opens_there_by_its_kind() {
    let run = || listed_with([session("a").cwd("/r/a")]).send(show(session_tab("a")));
    let picture = run().send(Msg::OpenFileIn(
        here(),
        "/r/a/docs/shot.png".into(),
        Opening::Tab,
    ));
    assert_eq!(
        active(&picture),
        Some(Tab::Viewer(here(), "/r/a/docs/shot.png".into()))
    );
    assert_eq!(picture.state().content.windows.tabs().len(), 2);
    let source = run().send(Msg::OpenFileIn(
        here(),
        "/r/a/src/main.rs".into(),
        Opening::Tab,
    ));
    assert!(source.last_effects().iter().any(|e| matches!(
        e,
        Effect::StartTerminal(TerminalAsk { opening: Opening::Tab, file: Some(f), .. }) if f == std::path::Path::new("/r/a/src/main.rs")
    )));
}

#[test]
fn a_file_asked_to_be_edited_in_another_environment_starts_its_neovim_there() {
    let run = shows_there(listed_with([session("a").cwd("/r")]), [session("b")])
        .send(Msg::EditFile(there(), "/c/prompt/nvim/init.lua".into()));
    assert!(run.last_effects().iter().any(|e| matches!(
        e,
        Effect::StartTerminal(TerminalAsk { place, .. }) if place.environment == there()
    )));
}

#[test]
fn a_file_asked_to_be_edited_shows_the_neovim_already_on_it_with_the_keys() {
    let nvim = tsumugi_model::TerminalSession {
        program: TerminalProgram::Neovim,
        file: Some("/c/prompt/nvim/init.lua".into()),
        ..shell(4, "/c/prompt/nvim")
    };
    let run = Scenario::new()
        .event(snapshot().listed(listed().session(session("a").cwd("/r")).terminal(nvim)))
        .send(Msg::EditFile(here(), "/c/prompt/nvim/init.lua".into()));
    assert_eq!(active(&run), Some(Tab::Terminal(here(), 4)));
    assert_eq!(run.state().content.focus, Area::Console);
    assert!(
        !run.last_effects()
            .iter()
            .any(|e| matches!(e, Effect::StartTerminal(_)))
    );
}

#[test]
fn a_new_session_or_shell_from_another_environment_s_row_starts_in_that_environment() {
    let base = || {
        shows_there(listed_with([session("a")]), [session("b")])
            .send(show(session_tab("a")))
            .send(Msg::TreeSelect(Some(Item::Session(there(), "b".into()))))
            .send(Msg::Focus(Area::Sidebar))
    };
    let shell = base().send(Msg::Window(Action::ShellOpen));
    assert_eq!(
        shell.last_effects(),
        [Effect::StartTerminal(TerminalAsk {
            place: Place::at(&there(), "/r/b"),
            runs: TerminalProgram::Shell,
            file: None,
            opening: Opening::Existing,
            focus: true,
        })]
    );
    let new = base().send(Msg::Window(Action::SessionNew));
    assert!(
        matches!(active(&new), Some(Tab::Draft { environment, worktree, .. })
            if environment == there() && worktree == std::path::Path::new("/r/b")),
        "{:?}",
        active(&new)
    );
}

#[test]
fn a_shell_started_beside_another_environment_s_tab_starts_in_that_environment() {
    let run = shows_there(listed_with([session("a")]), [session("b")])
        .send(Msg::ShowSession(there(), "b".into()))
        .send(Msg::Focus(Area::Console))
        .send(Msg::Window(Action::NeovimOpen));
    assert_eq!(
        run.last_effects(),
        [Effect::StartTerminal(TerminalAsk {
            place: Place::at(&there(), "/r/b"),
            runs: TerminalProgram::Neovim,
            file: None,
            opening: Opening::Existing,
            focus: true,
        })]
    );
}

#[test]
fn a_shell_of_another_environment_closing_is_told_to_that_environment_s_host() {
    let run = shows_there(listed_with([session("a")]), [session("b")]).send(Msg::ShellClosed {
        environment: there(),
        id: 1,
    });
    assert!(run.last_effects().contains(&Effect::Host {
        environment: there(),
        request: Request::TerminalClosed { id: 1 },
    }));
}

#[test]
fn a_terminal_opened_in_another_environment_is_told_to_that_environment_s_host() {
    let terminal = shell(4, "/r/b");
    let run = shows_there(listed_with([session("a")]), [session("b")]).send(Msg::TerminalOpened {
        environment: there(),
        terminal: terminal.clone(),
    });
    assert_eq!(
        run.last_effects(),
        [Effect::Host {
            environment: there(),
            request: Request::TerminalOpened { terminal },
        }]
    );
}

/// A content kept with a tab of the window's own environment and, after it,
/// one of `there()`, the latter's tab current.
fn kept_across_environments() -> Content<Tab> {
    let mut was = Content::default();
    was.open(Tab::Session(here(), "a".into()));
    was.open_tab(Tab::Session(there(), "b".into()));
    was
}

#[test]
fn the_tabs_kept_of_an_environment_shown_beside_the_own_come_back_once_it_is_listed() {
    let was = kept_across_environments();
    let own_listed = listed_with_kept(&was);
    assert_eq!(shown(&own_listed), [Tab::Session(here(), "a".into())]);
    let run = shows_there(own_listed, [session("b")]);
    assert_eq!(
        run.state().content.windows.kept(kept_name),
        was.kept(kept_name)
    );
    assert_eq!(active(&run), Some(Tab::Session(there(), "b".into())));
}

#[test]
fn a_kept_tab_of_another_environment_does_not_come_back_once_the_content_was_changed() {
    let changed = listed_with_kept(&kept_across_environments()).send(Msg::Window(Action::TabClose));
    let run = shows_there(changed, [session("b")]);
    assert_eq!(shown(&run), Vec::<Tab>::new());
}

#[test]
fn a_kept_tab_of_another_environment_its_listing_leaves_out_is_given_up() {
    let run = shows_there(
        listed_with_kept(&kept_across_environments()),
        [session("c")],
    );
    assert_eq!(shown(&run), [Tab::Session(here(), "a".into())]);
}

/// The window opened having kept `was`, once its own environment listed
/// the session `a`.
fn listed_with_kept(was: &Content<Tab>) -> Scenario {
    Scenario::new()
        .send(opened_with(was))
        .event(listing([session("a")]))
}

#[test]
fn a_session_of_another_environment_that_ends_gives_its_windows_up_unless_live_here() {
    // (live here, stays shown)
    for (held, stays) in [(false, false), (true, true)] {
        let run = shows_there(listed_with([session("a")]), [session("b")])
            .send(Msg::ShowSession(there(), "b".into()))
            .send(Msg::PanesLive(if held {
                BTreeSet::from([(there(), "b".to_string())])
            } else {
                BTreeSet::new()
            }))
            .event_from(there(), patch(2).listed(listed()));
        assert_eq!(
            shown(&run).contains(&Tab::Session(there(), "b".into())),
            stays,
            "{held}"
        );
    }
}

#[test]
fn a_live_session_of_one_environment_does_not_hold_a_like_named_one_of_another() {
    let run = shows_there(listed_with([session("a")]), [session("a")])
        .send(Msg::ShowSession(there(), "a".into()))
        .send(live(&["a"]))
        .event_from(there(), patch(2).listed(listed()));
    assert_eq!(shown(&run), Vec::<Tab>::new());
}

#[test]
fn the_panes_kept_of_each_environment_are_those_its_own_listing_lists() {
    let run = shows_there(listed_with([session("a")]), [session("b")]);
    let kept: Vec<Effect> = run
        .last_effects()
        .iter()
        .filter(|e| matches!(e, Effect::KeepPanes { .. }))
        .cloned()
        .collect();
    assert_eq!(
        kept,
        [Effect::KeepPanes {
            environment: there(),
            sessions: BTreeSet::from(["b".to_string()]),
            terminals: BTreeSet::new(),
        }]
    );
}

#[test]
fn a_login_terminal_asked_from_another_environment_s_draft_opens_in_that_environment() {
    let profile: tsumugi_model::profile::ProfileId = "claude".parse().unwrap();
    let run = shows_there(listed_with([session("a")]), [session("b")]).send(Msg::OpenLogin {
        environment: there(),
        profile: profile.clone(),
    });
    assert_eq!(
        run.last_effects(),
        [Effect::OpenLogin {
            environment: there(),
            profile,
        }]
    );
}

/// What the terminal host took with it, as the work host lists it: the
/// session or the shell the daemon knew as `id`.
fn interrupted(id: &str) -> tsumugi_model::Interrupted {
    tsumugi_model::Interrupted {
        id: id.into(),
        name: format!("{id}-name"),
        cwd: "/r".into(),
        started_ms: 1,
        what: tsumugi_model::InterruptedKind::Session {
            agent: tsumugi_model::Agent::Claude,
            profile: tsumugi_model::profile::ProfileId::builtin(tsumugi_model::Agent::Claude),
            conversation: Some("c1".into()),
        },
    }
}

#[test]
fn a_tab_of_a_session_the_terminal_host_took_stays_until_what_stands_for_it_takes_its_place() {
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(session_tab("a")))
        .event(
            patch(2).listed(
                listed()
                    .session(session("b").cwd("/r"))
                    .interrupted(interrupted("a")),
            ),
        );
    assert_eq!(shown(&run), [Tab::Session(here(), "a".into())]);
    assert_eq!(run.state().chrome.status.line.as_deref(), None);

    let back = run.event(
        patch(3).listed(
            listed()
                .session(session("b").cwd("/r"))
                .session(session("c").cwd("/r").restored_from("a")),
        ),
    );

    assert_eq!(shown(&back), [Tab::Session(here(), "c".into())]);
}

#[test]
fn a_tab_of_a_session_let_go_is_given_up() {
    let run = listed_with([session("a").cwd("/r")])
        .send(show(session_tab("a")))
        .event(patch(2).listed(listed().interrupted(interrupted("a"))))
        .event(patch(3).listed(listed()));
    assert_eq!(shown(&run), Vec::<Tab>::new());
}

#[test]
fn a_shell_s_tab_takes_the_shell_opened_again_for_it() {
    let old = tsumugi_model::daemon::hex(7);
    let mut again = shell(9, "/r");
    again.restored_from = Some(old.clone());
    let run = listed_with([session("a").cwd("/r")])
        .send(show(Tab::Terminal(here(), 7)))
        .event(
            patch(2).listed(
                listed()
                    .session(session("a").cwd("/r"))
                    .interrupted(interrupted(&old)),
            ),
        )
        .event(patch(3).listed(listed().session(session("a").cwd("/r")).terminal(again)));
    assert_eq!(shown(&run), [Tab::Terminal(here(), 9)]);
}

#[test]
fn a_kept_tab_of_what_the_terminal_host_took_comes_back_and_takes_what_stands_for_it() {
    let a = Tab::Session(here(), "a".into());
    let waiting = Scenario::new()
        .send(opened(Some(a.clone())))
        .event(snapshot().listed(listed().interrupted(interrupted("a"))));
    assert_eq!(active(&waiting), Some(a.clone()));
    let back = waiting.event(patch(2).listed(listed().session(session("c").restored_from("a"))));
    assert_eq!(active(&back), Some(Tab::Session(here(), "c".into())));

    let brought_before = Scenario::new()
        .send(opened(Some(a)))
        .event(snapshot().listed(listed().session(session("c").restored_from("a"))));
    assert_eq!(
        active(&brought_before),
        Some(Tab::Session(here(), "c".into()))
    );
}

/// The work of a worktree `/r/a` with a session, and a Neovim by id 4 on
/// `/r/a/README.md` when `editing`.
fn document_work(editing: bool) -> Scenario {
    let nvim = tsumugi_model::TerminalSession {
        program: TerminalProgram::Neovim,
        file: Some("/r/a/README.md".into()),
        ..shell(4, "/r/a")
    };
    let listed = listed().session(session("a").cwd("/r/a"));
    let listed = if editing {
        listed.terminal(nvim)
    } else {
        listed
    };
    Scenario::new()
        .send(opened(None))
        .event(snapshot().listed(listed))
}

fn readme() -> Tab {
    Tab::Viewer(here(), "/r/a/README.md".into())
}

fn edit() -> Msg {
    Msg::Window(Action::WindowEdit)
}

fn focused_window(run: &Scenario) -> WindowId {
    run.state().content.windows.focused_window().unwrap()
}

fn terminals_seen(run: &Scenario) -> BTreeSet<u128> {
    crate::settle::sight(run.state(), &here()).terminals
}

#[test]
fn a_document_s_viewer_switches_in_its_window_to_the_neovim_on_it() {
    let run = document_work(true).send(show(readme()));
    let window = focused_window(&run);
    let editing = run.send(edit());
    assert_eq!(active(&editing), Some(Tab::Terminal(here(), 4)));
    assert_eq!(focused_window(&editing), window);
    assert_eq!(editing.state().content.windows.tabs().len(), 1);
    assert_eq!(editing.state().content.focus, Area::Console);
}

#[test]
fn a_document_with_no_neovim_starts_one_for_its_window_once() {
    let run = document_work(false).send(show(readme()));
    let window = focused_window(&run);
    let asked = run.send(edit());
    assert_eq!(
        asked.last_effects(),
        [Effect::StartTerminal(TerminalAsk {
            place: place("/r/a").unwrap(),
            runs: TerminalProgram::Neovim,
            file: Some("/r/a/README.md".into()),
            opening: Opening::InWindow(window),
            focus: true,
        })]
    );
    let again = asked.send(edit());
    assert!(again.last_effects().is_empty());
    let started = again.send(Msg::Show {
        thing: Tab::Terminal(here(), 9),
        place: place("/r/a"),
        how: Some(Opening::InWindow(window)),
    });
    assert_eq!(active(&started), Some(Tab::Terminal(here(), 9)));
    assert_eq!(started.state().content.windows.tabs().len(), 1);
}

#[test]
fn a_neovim_started_after_its_window_moved_on_opens_beside_what_it_shows_now() {
    let run = document_work(false).send(Msg::Show {
        thing: readme(),
        place: None,
        how: Some(Opening::Existing),
    });
    let window = focused_window(&run);
    let moved_on = run.send(edit()).send(Msg::Show {
        thing: session_tab("a"),
        place: None,
        how: Some(Opening::InWindow(window)),
    });
    let started = moved_on.send(Msg::Show {
        thing: Tab::Terminal(here(), 9),
        place: place("/r/a"),
        how: Some(Opening::InWindow(window)),
    });
    assert_eq!(
        started.state().content.windows.shows(window),
        Some(&session_tab("a"))
    );
    assert!(shown(&started).contains(&Tab::Terminal(here(), 9)));
}

#[test]
fn a_neovim_that_could_not_start_lets_the_document_ask_for_one_again() {
    let run = document_work(false).send(show(readme()));
    let window = focused_window(&run);
    let failed = run.send(edit()).send(Msg::TerminalFailed {
        opening: Opening::InWindow(window),
    });
    assert!(
        failed
            .send(edit())
            .last_effects()
            .iter()
            .any(|e| matches!(e, Effect::StartTerminal(_)))
    );
}

#[test]
fn a_neovim_on_a_document_switches_back_to_its_viewer_and_runs_on_hidden() {
    let run = document_work(true).send(show(Tab::Terminal(here(), 4)));
    let viewing = run.send(edit());
    assert_eq!(active(&viewing), Some(readme()));
    assert_eq!(terminals_seen(&viewing), BTreeSet::from([4]));
    let back = viewing.send(edit());
    assert_eq!(active(&back), Some(Tab::Terminal(here(), 4)));
}

#[test]
fn a_hidden_neovim_is_let_go_once_its_window_closes() {
    let closed = document_work(true)
        .send(show(Tab::Terminal(here(), 4)))
        .send(edit())
        .send(Msg::Window(Action::WindowQuit));
    assert!(terminals_seen(&closed).is_empty());
}

#[test]
fn only_a_document_and_a_neovim_on_one_switch() {
    let picture = document_work(true)
        .send(show(Tab::Viewer(here(), "/r/a/shot.png".into())))
        .send(edit());
    assert_eq!(
        active(&picture),
        Some(Tab::Viewer(here(), "/r/a/shot.png".into()))
    );
    assert!(picture.last_effects().is_empty());
    let session = document_work(true)
        .send(show(session_tab("a")))
        .send(edit());
    assert_eq!(active(&session), Some(session_tab("a")));
}

#[test]
fn a_diff_tab_keeps_its_work_machine_and_repository_watched_without_a_prompt() {
    use tsumugi_model::repository::{Rev, Spot as RepositorySpot};
    let spot = RepositorySpot::Files(Rev::WorkingCopy);
    let run = Scenario::new().send(Msg::OpenDiff {
        environment: there(),
        worktree: "/remote/repo".into(),
        spot: spot.clone(),
        neovim: false,
    });
    let tab = Tab::Diff {
        environment: there(),
        worktree: "/remote/repo".into(),
        spot: Box::new(spot.clone()),
    };
    assert_eq!(run.state().content.active(), Some(&tab));
    assert!(!shows_prompt(Some(&tab)));
    assert_eq!(
        crate::tree::place_of_tab(&Default::default(), &[], &tab)
            .unwrap()
            .worktree,
        PathBuf::from("/remote/repo")
    );
    assert_eq!(
        crate::settle::sight(run.state(), &there()).repositories,
        [tsumugi_protocol::RepositorySight {
            worktree: "/remote/repo".into(),
            spot
        }]
    );
    assert!(
        crate::settle::sight(run.state(), &here())
            .repositories
            .is_empty()
    );
}

#[test]
fn a_diff_tab_on_a_file_keeps_the_list_of_files_beside_it_read() {
    use tsumugi_model::repository::{Change, Rev, Spot as RepositorySpot};
    let change = Change {
        status: " M".into(),
        path: "a.rs".into(),
        from: None,
    };
    let spot = RepositorySpot::Lines(Rev::Commit("abc".into()), change);
    let run = Scenario::new().send(Msg::OpenDiff {
        environment: there(),
        worktree: "/remote/repo".into(),
        spot: spot.clone(),
        neovim: false,
    });
    let read = |spot| tsumugi_protocol::RepositorySight {
        worktree: "/remote/repo".into(),
        spot,
    };
    let read_now = crate::settle::sight(run.state(), &there()).repositories;
    for wanted in [
        read(spot),
        read(RepositorySpot::Files(Rev::Commit("abc".into()))),
    ] {
        assert_eq!(read_now.iter().filter(|r| **r == wanted).count(), 1);
    }
}

#[test]
fn how_a_diff_is_drawn_is_kept_for_the_worktree_across_its_tabs() {
    use crate::diff_view::Choice;
    let choose = |choice| Msg::DiffView {
        environment: there(),
        worktree: "/remote/repo".into(),
        choice,
    };
    let run = Scenario::new()
        .send(choose(Choice::Wrap))
        .send(choose(Choice::Files));
    let views = &run.state().content.diff_views;
    let kept = views.of(&there(), std::path::Path::new("/remote/repo"));
    assert!(kept.wrap && !kept.files);
    assert_eq!(
        views.of(&there(), std::path::Path::new("/remote/other")),
        Default::default()
    );
}

#[test]
fn opening_a_diff_in_neovim_targets_the_work_machine_and_the_opened_commit_file() {
    use tsumugi_model::repository::{Change, Rev, Spot as RepositorySpot};
    let spot = RepositorySpot::Diff(
        Rev::Commit("abc123".into()),
        Change {
            status: "R".into(),
            path: "new name.rs".into(),
            from: Some("old name.rs".into()),
        },
    );
    let run = Scenario::new().send(Msg::OpenDiff {
        environment: there(),
        worktree: "/remote/repo".into(),
        spot: spot.clone(),
        neovim: true,
    });
    assert!(run.effects().iter().any(|effect| matches!(effect,
        Effect::StartDiffEditor { place, spot: target }
        if place.environment == there() && place.worktree == std::path::Path::new("/remote/repo") && target == &spot
    )));
    assert!(run.state().content.active().is_none());
}

#[test]
fn a_session_whose_program_exited_keeps_its_tab_and_says_why_it_rests() {
    let a = session_tab("a");
    let run = listed_with([session("a").cwd("/r"), session("b").cwd("/r")])
        .send(show(a.clone()))
        .event(
            patch(2).listed(
                listed()
                    .session(session("a").cwd("/r").rest(tsumugi_model::Rest::Exited))
                    .session(session("b").cwd("/r")),
            ),
        );

    assert_eq!(shown(&run), std::slice::from_ref(&a));
    let hosts = &run.state().hosts;
    assert_eq!(rest_of(hosts, &a), Some(tsumugi_model::Rest::Exited));
    assert_eq!(rest_of(hosts, &session_tab("b")), None);
    assert!(!waits_to_come_back(hosts, &a));
}
