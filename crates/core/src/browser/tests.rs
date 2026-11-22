use crate::actions::Action;
use crate::content::Tab;
use crate::effect::Effect;
use crate::msg::Msg;
use crate::scenario::Scenario;
use crate::testing::here;

fn perform(run: Scenario, action: Action) -> Scenario {
    run.send(Msg::Perform {
        action,
        key: None,
        probe: Box::default(),
    })
}

fn asks(run: &Scenario) -> Vec<Effect> {
    run.last_effects()
        .iter()
        .filter(|e| !matches!(e, Effect::Host { .. }))
        .cloned()
        .collect()
}

fn on_a_page() -> Scenario {
    Scenario::new().send(Msg::Window(Action::BrowserOpen))
}

#[test]
fn a_search_is_asked_of_the_browser_page_shown() {
    let run = perform(on_a_page(), Action::BrowserFind);

    assert!(matches!(
        run.state().content.active(),
        Some(Tab::Browser { .. })
    ));
    assert!(matches!(asks(&run)[..], [Effect::FindInPage(_)]));
}

#[test]
fn the_filter_key_searches_the_page_while_a_page_has_the_keys() {
    let run = perform(on_a_page(), Action::Filter);

    assert!(matches!(asks(&run)[..], [Effect::FindInPage(_)]));
}

#[test]
fn nothing_but_a_browser_page_is_searched() {
    let run = perform(Scenario::new(), Action::BrowserFind);

    assert!(matches!(asks(&run)[..], [Effect::Notice(_)]));
}

#[test]
fn reopening_asks_for_the_page_closed_last_in_the_worktree_the_keys_are_in() {
    let run = perform(on_a_page(), Action::BrowserReopen);

    let Some(Tab::Browser { worktree, .. }) = run.state().content.active().cloned() else {
        panic!("a page is shown");
    };
    assert_eq!(
        asks(&run),
        [Effect::ReopenPage {
            environment: here(),
            worktree
        }]
    );
}

#[test]
fn a_page_is_reopened_without_a_page_shown() {
    let run = perform(Scenario::new(), Action::BrowserReopen);

    assert!(matches!(asks(&run)[..], [Effect::ReopenPage { .. }]));
}
