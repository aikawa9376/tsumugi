//! The conversation and console are two views of the same session.

use super::{ContentView, Tab};
use crate::effect::Effect;
use crate::state::ViewState;

pub fn conversation_read<'a>(
    hosts: &'a crate::host::Hosts,
    tab: &Tab,
) -> Option<&'a tsumugi_protocol::TranscriptRead> {
    let Tab::Session(environment, id) = tab else {
        return None;
    };
    let session = hosts.session(environment, id)?.1;
    hosts
        .work(environment)?
        .details
        .sessions
        .get(id)?
        .transcript
        .as_ref()
        .filter(|read| read.id == session.session_id)
}

impl ContentView {
    pub fn conversation(&self, tab: &Tab) -> bool {
        self.conversations.contains(tab)
    }

    pub fn reading_conversation(&self) -> bool {
        self.active().is_some_and(|tab| self.conversation(tab))
    }
}

pub(crate) fn show_conversation(state: &mut ViewState, tab: Tab, shown: bool) -> Vec<Effect> {
    if !matches!(tab, Tab::Session(..)) || !state.content.windows.shows_anywhere(&tab) {
        return Vec::new();
    }
    if shown {
        state.content.conversations.insert(tab);
        state.console.direct = false;
        state.console.send_next = false;
        state.console.pending.clear();
        if state.console.reading {
            state.console.reading = false;
            return vec![Effect::ConsoleLive];
        }
    } else {
        state.content.conversations.remove(&tab);
    }
    Vec::new()
}
