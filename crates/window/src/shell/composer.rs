//! The dialog asking for a new worktree: its name, where it starts, and the
//! branch it works on. It only draws: the view state holds it, and its keys
//! go there.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::overlay::hints;
use tsumugi_core::worktree::{Composer, Field, Starts};
use tsumugi_model::StartPoint;

use crate::bridge::AppCtx;
use crate::components::{Dialog, InlineField, KeyHint, ListRow, PlaceMark};
use crate::icons::Icon;

/// What a place to start from is, said beside its name.
fn kind(point: &StartPoint) -> String {
    match point {
        StartPoint::Head => "the checkout's commit".into(),
        StartPoint::Local { .. } => "branch".into(),
        StartPoint::Remote { remote, .. } => format!("{remote}, fetched first"),
        StartPoint::Rev { .. } => "revision".into(),
    }
}

#[component]
pub(crate) fn ComposerDialog() -> Element {
    let app = use_context::<AppCtx>();
    let Some(composer) = app.overlays.read().composer.clone() else {
        return rsx! {};
    };
    let hints = hints::composer(&app.chrome.read().keymap);
    let rows = composer.rows();
    let Composer {
        project,
        field,
        name,
        query,
        branch,
        cursor,
        starts,
        ..
    } = composer;
    let field_of = |which: Field, class: &'static str, value: String, placeholder: &'static str| {
        rsx! {
            InlineField {
                class,
                value,
                placeholder,
                focus: field == which,
                oninput: move |text| app.dispatch(Msg::ComposerText { field: which, text }),
                onfocus: move |()| app.dispatch(Msg::ComposerFocus(which)),
                onkey: move |k| app.dispatch(Msg::ComposerKey(k)),
            }
        }
    };
    rsx! {
        Dialog {
            kind: "picker-dialog composer-dialog",
            onclose: move |_| app.dispatch(Msg::ComposerKey(tsumugi_model::Key::new("Escape"))),
            icon: Icon::GitBranch,
            title: "New worktree of {project}",
            label: "Name",
            div { class: "filter-field composer-field",
                {field_of(Field::Name, "filter-field-input", name, "Name")}
            }
            div { class: "composer-label", "Start from" }
            div { class: "filter-field composer-field",
                {field_of(Field::Start, "filter-field-input", query, "A branch, or a commit to type")}
            }
            div { class: "list-rows",
                if let Starts::Failed(why) = &starts {
                    ListRow { kind: "dim", "Branches could not be listed: {why}" }
                }
                if matches!(starts, Starts::Reading) {
                    ListRow { kind: "dim", "Reading the branches…" }
                }
                for (i, row) in rows.into_iter().enumerate() {
                    ListRow {
                        key: "{row.point:?}",
                        selected: i == cursor,
                        onclick: move |_| app.dispatch(Msg::ComposerCursor(i)),
                        PlaceMark { icon: Icon::GitBranch }
                        span { class: "list-row-name",
                            "{row.point.name()}"
                            span { class: "picker-place", "{row.subject}" }
                        }
                        span { class: "list-row-meta", "{kind(&row.point)}" }
                    }
                }
            }
            div { class: "composer-label", "Branch (named after the directory when blank)" }
            div { class: "filter-field composer-field",
                {field_of(Field::Branch, "filter-field-input", branch, "New branch")}
            }
            div { class: "picker-keys",
                for hint in hints {
                    KeyHint { key: "{hint.keys}", keys: hint.keys.clone(), does: hint.does }
                }
            }
        }
    }
}
