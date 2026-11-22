use tsumugi_model::conversation::{Message, Role};
use tsumugi_model::{SessionId, State, Wait};

use super::*;
use crate::actions::Action;
use crate::content::Tab;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::{here, listing, session, session_tab, there};

const MINUTE: u64 = 60_000;

fn perform(run: Scenario, action: Action) -> Scenario {
    run.send(Msg::Perform {
        action,
        key: None,
        probe: Box::default(),
    })
}

fn tick(run: Scenario, now_ms: u64) -> Scenario {
    run.send(Msg::BoardClock(now_ms))
}

fn lanes(run: &Scenario) -> Vec<(Lane, Vec<String>)> {
    let state = run.state();
    state
        .board
        .lanes(&state.hosts, &state.tree.completions)
        .into_iter()
        .map(|l| (l.lane, l.cards.into_iter().map(|c| c.id).collect()))
        .collect()
}

fn ids(run: &Scenario, lane: Lane) -> Vec<String> {
    lanes(run)
        .into_iter()
        .find(|(l, _)| *l == lane)
        .map(|(_, cards)| cards)
        .unwrap_or_default()
}

fn selected(run: &Scenario) -> Option<String> {
    run.state().board.selected().map(|(_, id)| id.clone())
}

fn transcript(
    id: &str,
    session_id: &str,
    messages: &[(Role, &str)],
) -> tsumugi_protocol::DetailsChange {
    tsumugi_protocol::DetailsChange::Transcript {
        session: id.into(),
        transcript: tsumugi_protocol::TranscriptRead {
            id: session_id.into(),
            messages: messages
                .iter()
                .map(|(role, text)| Message {
                    role: *role,
                    text: (*text).into(),
                    at_ms: None,
                    thinking: false,
                    tool: None,
                })
                .collect(),
        },
    }
}

#[test]
fn a_state_stands_in_the_lane_of_what_it_asks_of_the_user() {
    let quiet = 0;
    for (state, unread, lane) in [
        (State::Waiting(Wait::Permission), false, Lane::NeedsYou),
        (State::Waiting(Wait::Input), false, Lane::NeedsYou),
        (State::Failed, false, Lane::NeedsYou),
        (State::Starting, false, Lane::Working),
        (State::Working, false, Lane::Working),
        (State::Background, false, Lane::Working),
        (State::Done, true, Lane::Done),
        (State::Done, false, Lane::Done),
        (State::Idle, false, Lane::Idle),
    ] {
        assert_eq!(lane_of(&state, unread, quiet), lane, "{state:?} {unread}");
    }
}

#[test]
fn a_finished_session_goes_idle_once_it_has_been_quiet_for_half_an_hour_and_not_before() {
    let done = State::Done;
    assert_eq!(lane_of(&done, false, IDLE_AFTER_MS - 1), Lane::Done);
    assert_eq!(lane_of(&done, false, IDLE_AFTER_MS), Lane::Idle);
    // One the user has not seen stays where it asks to be looked at.
    assert_eq!(lane_of(&done, true, IDLE_AFTER_MS * 10), Lane::Done);
    // Work and waiting are never quiet.
    assert_eq!(
        lane_of(&State::Working, false, IDLE_AFTER_MS * 10),
        Lane::Working
    );
    assert_eq!(
        lane_of(&State::Waiting(Wait::Input), false, IDLE_AFTER_MS * 10),
        Lane::NeedsYou
    );
}

#[test]
fn the_board_has_a_lane_for_what_needs_the_user_what_works_and_what_is_done_and_idle_ones_only_on_request()
 {
    let run = Scenario::new().event(listing([
        session("w").state(State::Working),
        session("n").waiting(Wait::Permission),
        session("d").state(State::Done),
        session("i").state(State::Idle),
    ]));
    assert_eq!(
        lanes(&run),
        vec![
            (Lane::NeedsYou, vec!["n".to_string()]),
            (Lane::Working, vec!["w".to_string()]),
            (Lane::Done, vec!["d".to_string()]),
        ]
    );
    let run = perform(run, Action::BoardIdle);
    assert_eq!(ids(&run, Lane::Idle), vec!["i".to_string()]);
    assert_eq!(lanes(&run).len(), 4);
    let run = perform(run, Action::BoardIdle);
    assert_eq!(lanes(&run).len(), 3);
}

#[test]
fn a_card_moves_to_the_lane_of_its_new_state_and_the_cursor_goes_with_it() {
    let run = Scenario::new().event(listing([
        session("a").state(State::Working),
        session("b").state(State::Working),
    ]));
    let run = perform(run, Action::BoardDown);
    assert_eq!(selected(&run).as_deref(), Some("b"));
    let run = run.tell(tsumugi_protocol::TreeChange::Listed {
        listed: Box::new(
            tsumugi_protocol::testing::listed()
                .session(session("a").state(State::Working))
                .session(session("b").waiting(Wait::Permission))
                .build(),
        ),
    });
    assert_eq!(ids(&run, Lane::NeedsYou), vec!["b".to_string()]);
    assert_eq!(ids(&run, Lane::Working), vec!["a".to_string()]);
    // Neither the cursor nor what it is on is lost to the move.
    assert_eq!(selected(&run).as_deref(), Some("b"));
}

#[test]
fn a_cursor_whose_card_is_gone_stays_near_where_it_was() {
    let run = Scenario::new().event(listing([
        session("a").state(State::Working),
        session("b").state(State::Working),
        session("c").state(State::Working),
    ]));
    let run = perform(perform(run, Action::BoardDown), Action::BoardDown);
    assert_eq!(selected(&run).as_deref(), Some("c"));
    let run = run.tell(tsumugi_protocol::TreeChange::Listed {
        listed: Box::new(
            tsumugi_protocol::testing::listed()
                .session(session("a").state(State::Working))
                .session(session("b").state(State::Working))
                .build(),
        ),
    });
    assert_eq!(selected(&run).as_deref(), Some("b"));
}

#[test]
fn the_cursor_starts_on_the_first_card_and_moves_by_lane_with_h_and_l_and_by_card_with_j_and_k() {
    let run = Scenario::new().event(listing([
        session("n").waiting(Wait::Input),
        session("w1").state(State::Working),
        session("w2").state(State::Working),
        session("d").state(State::Done),
    ]));
    assert_eq!(selected(&run).as_deref(), Some("n"));
    let run = perform(run, Action::BoardRight);
    assert_eq!(
        selected(&run).as_deref(),
        Some(ids(&run, Lane::Working)[0].as_str())
    );
    let run = perform(run, Action::BoardDown);
    assert_eq!(
        selected(&run).as_deref(),
        Some(ids(&run, Lane::Working)[1].as_str())
    );
    // A lane with fewer cards takes the cursor to its last.
    let run = perform(run, Action::BoardRight);
    assert_eq!(selected(&run).as_deref(), Some("d"));
    // The last lane is where it stops.
    let run = perform(run, Action::BoardRight);
    assert_eq!(selected(&run).as_deref(), Some("d"));
    let run = perform(run, Action::BoardLeft);
    assert_eq!(
        selected(&run).as_deref(),
        Some(ids(&run, Lane::Working)[0].as_str())
    );
    let run = perform(run, Action::BoardUp);
    assert_eq!(
        selected(&run).as_deref(),
        Some(ids(&run, Lane::Working)[0].as_str())
    );
    let run = perform(run, Action::BoardLast);
    assert_eq!(
        selected(&run).as_deref(),
        Some(ids(&run, Lane::Working)[1].as_str())
    );
    let run = perform(run, Action::BoardFirst);
    assert_eq!(
        selected(&run).as_deref(),
        Some(ids(&run, Lane::Working)[0].as_str())
    );
}

#[test]
fn a_lane_without_cards_is_passed_over_by_h_and_l() {
    let run = Scenario::new().event(listing([
        session("n").waiting(Wait::Input),
        session("d").state(State::Done),
    ]));
    let run = perform(run, Action::BoardRight);
    assert_eq!(selected(&run).as_deref(), Some("d"));
    let run = perform(run, Action::BoardLeft);
    assert_eq!(selected(&run).as_deref(), Some("n"));
}

#[test]
fn enter_opens_the_session_of_the_card() {
    let run = Scenario::new().event(listing([
        session("a").state(State::Working),
        session("b").state(State::Working),
    ]));
    let run = perform(perform(run, Action::BoardDown), Action::BoardEnter);
    assert_eq!(
        run.state().content.active(),
        Some(&session_tab("b")),
        "{:?}",
        run.state().content.active()
    );
}

#[test]
fn a_card_shows_the_last_words_of_the_user_and_of_the_agent_from_the_conversation() {
    let run = Scenario::new()
        .event(listing([session("a")
            .session_id("sid")
            .state(State::Working)]))
        .send(crate::testing::show(session_tab("a")))
        .tell(transcript(
            "a",
            "sid",
            &[
                (Role::User, "first question"),
                (Role::Assistant, "first answer"),
                (Role::User, "fix the build"),
                (Role::Assistant, "looking at it"),
            ],
        ));
    let state = run.state();
    let board = state.board.lanes(&state.hosts, &state.tree.completions);
    let card = &board[1].cards[0];
    assert_eq!(card.last_user.as_deref(), Some("fix the build"));
    assert_eq!(card.last_agent.as_deref(), Some("looking at it"));
}

#[test]
fn the_cards_of_a_board_that_is_shown_ask_the_host_to_read_their_conversations() {
    let run = Scenario::new().event(listing([
        session("a").state(State::Working),
        session("b").state(State::Done),
    ]));
    let sight = |run: &Scenario| crate::settle::sight(run.state(), &here()).conversations;
    assert!(sight(&run).is_empty());
    let run = run.send(crate::testing::show(Tab::Board(here())));
    assert_eq!(
        sight(&run),
        ["a".to_string(), "b".to_string()].into_iter().collect()
    );
}

#[test]
fn a_session_quiet_for_half_an_hour_since_its_last_input_is_idle_with_the_minutes_told() {
    let started = 10 * MINUTE;
    let run = Scenario::new().event(listing([session("a")
        .state(State::Done)
        .started_ms(started)
        .last_input_ms(started + MINUTE)]));
    // Seen before, the user has nothing left to look at in it.
    let run = run.send(crate::testing::show(session_tab("a")));
    let run = tick(run, started + 20 * MINUTE);
    assert_eq!(ids(&run, Lane::Done), vec!["a".to_string()]);
    let run = tick(run, started + MINUTE + IDLE_AFTER_MS);
    let state = run.state();
    let board = state.board.lanes(&state.hosts, &state.tree.completions);
    // Idle is hidden until asked for.
    assert!(board.iter().all(|l| l.cards.is_empty()));
    let run = perform(run, Action::BoardIdle);
    let state = run.state();
    let board = state.board.lanes(&state.hosts, &state.tree.completions);
    let idle = board.iter().find(|l| l.lane == Lane::Idle).unwrap();
    assert_eq!(idle.cards[0].quiet_ms, IDLE_AFTER_MS);
    assert_eq!(idle.cards[0].quiet(), "30m idle");
}

#[test]
fn a_state_the_board_sees_change_counts_from_the_change() {
    let run = Scenario::new().event(listing([session("a").state(State::Working).started_ms(1)]));
    let run = tick(run, 100 * MINUTE);
    let run = run.tell(tsumugi_protocol::TreeChange::Listed {
        listed: Box::new(
            tsumugi_protocol::testing::listed()
                .session(session("a").state(State::Done).started_ms(1))
                .build(),
        ),
    });
    let run = run.send(crate::testing::show(session_tab("a")));
    let run = tick(run, 100 * MINUTE + 29 * MINUTE);
    assert_eq!(ids(&run, Lane::Done), vec!["a".to_string()]);
    let run = tick(run, 100 * MINUTE + 30 * MINUTE);
    assert!(ids(&run, Lane::Done).is_empty());
}

#[test]
fn the_newest_change_leads_its_lane() {
    let run = Scenario::new().event(listing([
        session("old").state(State::Working),
        session("new").state(State::Working),
    ]));
    let run = tick(run, 10 * MINUTE);
    let run = run.tell(tsumugi_protocol::TreeChange::Listed {
        listed: Box::new(
            tsumugi_protocol::testing::listed()
                .session(session("old").state(State::Working))
                .session(session("new").state(State::Done))
                .build(),
        ),
    });
    let run = tick(run, 11 * MINUTE).tell(tsumugi_protocol::TreeChange::Listed {
        listed: Box::new(
            tsumugi_protocol::testing::listed()
                .session(session("old").state(State::Done))
                .session(session("new").state(State::Done))
                .build(),
        ),
    });
    assert_eq!(
        ids(&run, Lane::Done),
        vec!["old".to_string(), "new".to_string()]
    );
}

#[test]
fn the_query_finds_cards_by_name_agent_project_and_what_was_said() {
    for (query, found) in [
        ("", vec!["a", "b"]),
        ("BUILD", vec!["a"]),
        ("docs", vec!["b"]),
        ("linker", vec!["a"]),
        ("claude", vec!["a", "b"]),
        ("nothing like it", vec![]),
    ] {
        let run = Scenario::new()
            .event(listing([
                session("a")
                    .name("fix-build")
                    .session_id("sa")
                    .state(State::Working),
                session("b").name("write-docs").state(State::Working),
            ]))
            .send(crate::testing::show(session_tab("a")))
            .tell(transcript(
                "a",
                "sa",
                &[(Role::Assistant, "the linker fails")],
            ))
            .send(Msg::BoardQuery(query.into()));
        let mut all: Vec<_> = lanes(&run).into_iter().flat_map(|(_, c)| c).collect();
        all.sort();
        assert_eq!(all, found, "{query:?}");
    }
}

#[test]
fn a_project_chip_keeps_only_that_project_s_cards_and_the_chips_count_what_each_holds() {
    let run = Scenario::new().event(listing([
        session("a").cwd("/one/x").state(State::Working),
        session("b").cwd("/one/y").state(State::Working),
        session("c").cwd("/two/z").state(State::Done),
    ]));
    let state = run.state();
    let projects = state.board.projects(&state.hosts, &state.tree.completions);
    let named: Vec<_> = projects
        .iter()
        .map(|p| (p.name.as_str(), p.count))
        .collect();
    assert_eq!(named.iter().map(|(_, n)| n).sum::<usize>(), 3);
    let first = projects[0].name.clone();
    let run = run.send(Msg::BoardProject(first.clone()));
    let state = run.state();
    let kept: usize = state
        .board
        .lanes(&state.hosts, &state.tree.completions)
        .iter()
        .map(|l| l.cards.len())
        .sum();
    assert_eq!(kept, projects[0].count);
    let run = run.send(Msg::BoardProject(first));
    let state = run.state();
    let all: usize = state
        .board
        .lanes(&state.hosts, &state.tree.completions)
        .iter()
        .map(|l| l.cards.len())
        .sum();
    assert_eq!(all, 3);
}

#[test]
fn the_project_and_state_keys_step_through_their_chips_and_the_clear_key_drops_every_filter() {
    let run = Scenario::new().event(listing([
        session("a").cwd("/one/x").state(State::Working),
        session("c").cwd("/two/z").state(State::Done),
    ]));
    let run = perform(run, Action::BoardProject);
    assert_eq!(run.state().board.projects.len(), 1);
    let run = perform(run, Action::BoardState);
    assert_eq!(run.state().board.states.len(), 1);
    let run = run.send(Msg::BoardQuery("x".into()));
    let run = perform(run, Action::BoardClear);
    assert!(run.state().board.projects.is_empty());
    assert!(run.state().board.states.is_empty());
    assert!(run.state().board.query.is_empty());
}

#[test]
fn a_state_chip_keeps_only_that_lane() {
    let run = Scenario::new().event(listing([
        session("a").state(State::Working),
        session("c").state(State::Done),
    ]));
    let run = run.send(Msg::BoardLane(Lane::Done));
    assert_eq!(ids(&run, Lane::Done), vec!["c".to_string()]);
    assert!(ids(&run, Lane::Working).is_empty());
}

#[test]
fn a_filter_that_hides_the_card_under_the_cursor_moves_the_cursor_to_what_is_left() {
    let run = Scenario::new().event(listing([
        session("a").name("alpha").state(State::Working),
        session("b").name("beta").state(State::Working),
    ]));
    let run = perform(run, Action::BoardDown);
    assert_eq!(selected(&run).as_deref(), Some("b"));
    let run = run.send(Msg::BoardQuery("alpha".into()));
    assert_eq!(selected(&run).as_deref(), Some("a"));
}

#[test]
fn grouped_by_opener_the_workers_of_a_session_fold_under_its_card() {
    let leader = SessionId::new();
    let run = Scenario::new().event(listing([
        session("lead").delegation(leader).state(State::Working),
        session("w1").parent(leader).state(State::Working),
        session("w2").parent(leader).waiting(Wait::Input),
        session("solo").state(State::Done),
    ]));
    // Apart, every session has its card.
    assert_eq!(ids(&run, Lane::Working).len(), 2);
    let run = perform(run, Action::BoardGroup);
    assert_eq!(ids(&run, Lane::Working), vec!["lead".to_string()]);
    assert!(ids(&run, Lane::NeedsYou).is_empty());
    let state = run.state();
    let board = state.board.lanes(&state.hosts, &state.tree.completions);
    let card = &board[1].cards[0];
    let mut children: Vec<_> = card
        .children
        .iter()
        .map(|c| (c.id.as_str(), c.lane))
        .collect();
    children.sort();
    assert_eq!(
        children,
        vec![("w1", Lane::Working), ("w2", Lane::NeedsYou)]
    );
    // What the workers ask of the user shows on the opener's card.
    assert_eq!(card.asking, 1);
}

#[test]
fn a_worker_whose_opener_is_not_listed_keeps_its_own_card_when_grouped() {
    let gone = SessionId::new();
    let run = Scenario::new().event(listing([session("w").parent(gone).state(State::Working)]));
    let run = perform(run, Action::BoardGroup);
    assert_eq!(ids(&run, Lane::Working), vec!["w".to_string()]);
}

#[test]
fn a_session_of_another_environment_is_a_card_of_the_board_too() {
    let run = crate::testing::shows_there(
        Scenario::new().event(listing([session("a").state(State::Working)])),
        [session("t").state(State::Working)],
    );
    let state = run.state();
    let board = state.board.lanes(&state.hosts, &state.tree.completions);
    let environments: Vec<_> = board[1]
        .cards
        .iter()
        .map(|c| c.environment.clone())
        .collect();
    assert!(environments.contains(&here()));
    assert!(environments.contains(&there()));
}

#[test]
fn opening_the_board_shows_one_tab_for_it() {
    let run = Scenario::new().event(listing([session("a").state(State::Working)]));
    let run = perform(run, Action::BoardOpen);
    assert_eq!(run.state().content.active(), Some(&Tab::Board(here())));
    let run = perform(run, Action::BoardOpen);
    let shown = run.state().content.windows.shown();
    assert_eq!(
        shown.iter().filter(|t| matches!(t, Tab::Board(_))).count(),
        1
    );
}
