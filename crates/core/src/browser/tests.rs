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

mod links {
    use super::*;
    use crate::config::testing::configured;
    use crate::content::Opening;
    use crate::settings::BrowserSettings;

    const URL: &str = "https://example.com/a?b=1";

    fn routed(settings: &str, url: &str, modifier: bool) -> Vec<Effect> {
        let run = Scenario::new()
            .event(configured(settings))
            .send(Msg::OpenLink {
                environment: here(),
                url: url.into(),
                modifier,
            });
        asks(&run)
    }

    fn in_the_system() -> Vec<Effect> {
        vec![Effect::Open {
            environment: here(),
            target: URL.into(),
        }]
    }

    fn in_the_app() -> Vec<Effect> {
        vec![Effect::OpenAddress {
            environment: here(),
            worktree: Default::default(),
            url: URL.into(),
            how: Opening::Existing,
        }]
    }

    #[test]
    fn a_link_opens_in_the_system_browser_unless_the_settings_say_otherwise() {
        assert_eq!(routed("", URL, false), in_the_system());
        assert_eq!(
            routed("[browser]\nopen_links_in_app = true\n", URL, false),
            in_the_app()
        );
    }

    #[test]
    fn the_modifier_sends_one_link_to_the_system_browser_while_links_open_in_the_app() {
        assert_eq!(
            routed("[browser]\nopen_links_in_app = true\n", URL, true),
            in_the_system()
        );
    }

    #[test]
    fn the_modifier_does_nothing_for_the_system_browser_until_it_is_set_to_invert() {
        assert_eq!(routed("", URL, true), in_the_system());
        assert_eq!(
            routed("[browser]\nmodifier_inverts = true\n", URL, true),
            in_the_app()
        );
        assert_eq!(
            routed(
                "[browser]\nopen_links_in_app = true\nmodifier_inverts = true\n",
                URL,
                true
            ),
            in_the_system()
        );
    }

    #[test]
    fn only_web_addresses_are_opened_from_a_link() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "mailto:a@b",
            "",
        ] {
            assert!(routed("[browser]\nopen_links_in_app = true\n", url, false).is_empty());
            assert!(routed("", url, false).is_empty());
        }
    }

    #[test]
    fn the_destination_is_told_by_the_setting_and_the_modifier() {
        use crate::browser::{Destination, destination};
        let settings = |app, invert| BrowserSettings {
            open_links_in_app: app,
            modifier_inverts: invert,
            ..BrowserSettings::default()
        };
        for (app, invert, modifier, want) in [
            (false, false, false, Destination::System),
            (false, false, true, Destination::System),
            (false, true, false, Destination::System),
            (false, true, true, Destination::App),
            (true, false, false, Destination::App),
            (true, false, true, Destination::System),
            (true, true, false, Destination::App),
            (true, true, true, Destination::System),
        ] {
            assert_eq!(
                destination(&settings(app, invert), modifier),
                want,
                "app {app}, invert {invert}, modifier {modifier}"
            );
        }
    }
}
