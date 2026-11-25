use crate::actions::Action;
use crate::content::Area;
use crate::details::Key;
use crate::details::panel::tests::{PINNED_TAB, pinned_in, resource};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::{Holder, holder};
use crate::scenario::Scenario;
use crate::testing::here;
use crate::tree::Place;

/// A tab listing what the project keeps, then the links and files the
/// session came across.
const TAB: &str =
    "[[panel.session.tabs]]\ntitle = \"Mine\"\ncomponents = [\"pinned\", \"links\", \"files\"]\n";

fn perform(run: Scenario, action: Action) -> Scenario {
    run.send(Msg::Perform {
        action,
        key: None,
        probe: Box::default(),
    })
}

fn down(run: Scenario, times: usize) -> Scenario {
    run.send_all((0..times).map(|_| Msg::DetailsKey(Key::Down)))
}

fn asks(run: &Scenario) -> Vec<Effect> {
    run.last_effects()
        .iter()
        .filter(|e| !matches!(e, Effect::Host { .. }))
        .cloned()
        .collect()
}

#[test]
fn adding_opens_the_sheet_on_the_project_the_panel_shows() {
    let run = perform(
        pinned_in(PINNED_TAB, vec![resource("/w", "/etc/a.conf")]),
        Action::ResourceAdd,
    );

    let sheet = run.state().overlays.resources.as_ref().unwrap();
    assert_eq!(sheet.place, Place::at(&here(), "/w"));
    assert_eq!(sheet.editing, None);
    assert_eq!(holder(run.state()), Some(Holder::Resources));
}

#[test]
fn adding_with_nothing_shown_says_so_and_opens_nothing() {
    let run = perform(
        Scenario::new().send(Msg::Focus(Area::Details)),
        Action::ResourceAdd,
    );

    assert!(run.state().overlays.resources.is_none());
    assert!(matches!(asks(&run)[..], [Effect::Notice(_)]));
}

#[test]
fn editing_the_pinned_resource_the_cursor_is_on_opens_the_sheet_on_it() {
    let kept = resource("/w", "/etc/a.conf");
    let run = perform(
        down(pinned_in(PINNED_TAB, vec![kept.clone()]), 1),
        Action::ResourceEdit,
    );

    let sheet = run.state().overlays.resources.as_ref().unwrap();
    assert_eq!(sheet.editing, Some(kept));
}

#[test]
fn editing_where_the_cursor_is_not_on_a_pinned_resource_opens_nothing() {
    let run = perform(
        pinned_in(PINNED_TAB, vec![resource("/w", "/etc/a.conf")]),
        Action::ResourceEdit,
    );

    assert!(run.state().overlays.resources.is_none());
}

#[test]
fn removing_the_pinned_resource_the_cursor_is_on_asks_the_host_to_drop_it() {
    let kept = resource("/w", "/etc/a.conf");
    let run = perform(
        down(pinned_in(PINNED_TAB, vec![kept.clone()]), 1),
        Action::ResourceRemove,
    );

    assert_eq!(
        asks(&run),
        [Effect::DropResource {
            environment: here(),
            id: kept.id
        }]
    );
}

#[test]
fn a_link_the_cursor_is_on_is_kept_as_an_address_and_a_file_as_a_path() {
    let pinned = || pinned_in(TAB, vec![resource("/w", "/etc/a.conf")]);

    // Rows: Pinned, the resource, Links, the address, Files, the file.
    let link = perform(down(pinned(), 3), Action::ResourceKeep);
    let file = perform(down(pinned(), 5), Action::ResourceKeep);

    assert_eq!(
        asks(&link),
        [Effect::KeepResource {
            environment: here(),
            dir: "/w".into(),
            target: "https://a.dev".into(),
            label: None,
        }]
    );
    assert_eq!(
        asks(&file),
        [Effect::KeepResource {
            environment: here(),
            dir: "/w".into(),
            target: "/w/a.rs".into(),
            label: None,
        }]
    );
}

#[test]
fn what_the_project_keeps_already_is_said_to_be_kept_and_not_asked_again() {
    let run = perform(
        down(pinned_in(TAB, vec![resource("/w", "https://a.dev")]), 3),
        Action::ResourceKeep,
    );

    assert!(matches!(asks(&run)[..], [Effect::Notice(_)]));
}

#[test]
fn escape_puts_the_sheet_away() {
    let run = perform(
        pinned_in(PINNED_TAB, vec![resource("/w", "/etc/a.conf")]),
        Action::ResourceAdd,
    )
    .send(Msg::HeldKey(tsumugi_model::Key::new("Escape")));

    assert!(run.state().overlays.resources.is_none());
}
