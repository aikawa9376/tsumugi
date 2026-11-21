//! The list of what needed the user, to go through.

use dioxus::prelude::*;
use tsumugi_core::msg::Msg;
use tsumugi_core::notices::{Listing, Row, rows};
use tsumugi_model::time::{ago, now_ms};
use tsumugi_model::{NoticeKind, Standing};

use super::{PickerKeys, QueryField};
use crate::bridge::AppCtx;
use crate::components::{Dialog, ListRow, PlaceMark};
use crate::icons::Icon;

#[component]
pub(super) fn Notices(list: Listing) -> Element {
    let app = use_context::<AppCtx>();
    let hosts = app.hosts.read();
    let notices = app.notices.read();
    let rows = rows(&notices, &hosts, &list.list.query);
    let now = now_ms();
    rsx! {
        Dialog { kind: "picker-dialog", onclose: move |_| app.dispatch(Msg::PickerClose), icon: Icon::Bell, title: "What needed you",
            div { class: "filter-field",
                QueryField { query: list.list.query.clone(), placeholder: "Sessions, workspaces and reasons" }
            }
            div { class: "list-rows",
                if rows.is_empty() {
                    ListRow { kind: "dim",
                        if list.list.query.is_empty() { "Nothing needed you yet" } else { "Nothing matches" }
                    }
                }
                for (i, row) in rows.into_iter().enumerate() {
                    NoticeRow { key: "{row.environment}-{row.notice.id}", row, selected: i == list.list.cursor, at: i, now }
                }
            }
            PickerKeys {}
        }
    }
}

#[component]
fn NoticeRow(row: Row, selected: bool, at: usize, now: u64) -> Element {
    let app = use_context::<AppCtx>();
    let icon = match row.notice.kind {
        NoticeKind::Waiting => Icon::Bell,
        NoticeKind::Failed => Icon::AlertTriangle,
        NoticeKind::Done => Icon::Check,
    };
    let standing = match row.standing {
        Standing::Current => "now",
        Standing::Past => "past",
        Standing::Ended => "ended",
    };
    let mark = if row.read { "" } else { "● " };
    rsx! {
        ListRow {
            selected,
            kind: if row.read { "dim" } else { "" },
            onclick: move |_| app.dispatch(Msg::PickerChoose(at)),
            PlaceMark { icon }
            span { class: "list-row-name",
                "{mark}{row.notice.name}"
                span { class: "picker-place", "{row.notice.workspace} · {row.notice.kind.word()} · {row.notice.reason}" }
            }
            span { class: "list-row-meta",
                "{standing} · {ago(now.saturating_sub(row.notice.at_ms))}"
            }
        }
    }
}
