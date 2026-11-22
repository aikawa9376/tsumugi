//! The session's human conversation, drawn in the document viewer's frame.

use std::sync::atomic::{AtomicU64, Ordering};

use arto_frontend::Libraries;
use dioxus::prelude::*;
use tsumugi_core::content::{Tab, WindowId};
use tsumugi_core::msg::Msg;
use tsumugi_core::viewers::{Effect, MarkdownState, ViewerState};

use crate::bridge::AppCtx;
use crate::console::Hub;
use crate::viewers::{self, frame_script, frame_scroll, page_frame, theme_message};

static NEXT: AtomicU64 = AtomicU64::new(1);

#[component]
pub(crate) fn ConversationScreen(
    tab: Tab,
    window: WindowId,
    commands: ReadSignal<Option<(WindowId, viewers::Command)>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let palette = use_context::<Hub>().palette;
    let theme = use_memo(move || crate::themes::document(&palette.read()));
    let prefix = use_hook(|| {
        format!(
            "tsumugi-conversation-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    });
    let media_id = format!("{prefix}-media");
    let box_id = format!("{prefix}-box");
    let nonce = use_hook(|| {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(tsumugi_model::time::now_ms());
        format!("{:016x}", h.finish())
    });
    let mut frame_doc = use_signal(|| None::<String>);
    let mut state = use_signal(|| ViewerState::Markdown(MarkdownState::default()));
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));
    let read = use_memo({
        let tab = tab.clone();
        move || tsumugi_core::content::conversation_read(&app.hosts.read(), &tab).cloned()
    });
    use_effect({
        let (media_id, nonce) = (media_id.clone(), nonce.clone());
        move || {
            let read = read();
            let conversation = read.as_ref().map(|r| r.id.as_str()).unwrap_or_default();
            let messages = read
                .as_ref()
                .map(|r| r.messages.as_slice())
                .unwrap_or_default();
            let fragment = viewers::conversation_fragment(messages);
            let libraries = Libraries::for_fragment(&fragment);
            if frame_doc.peek().is_none() {
                frame_doc.set(Some(viewers::conversation_document(
                    &fragment,
                    libraries,
                    &theme.peek(),
                    &nonce,
                    conversation,
                )));
            } else {
                let scripts: Vec<_> = viewers::library_names(libraries)
                    .into_iter()
                    .map(|n| format!("{}{}/{n}", viewers::ORIGIN, viewers::ARTO_PREFIX))
                    .collect();
                let message =
                    serde_json::json!({"markdown":fragment,"scripts":scripts,"conversation":conversation}).to_string();
                document::eval(&viewers::conversation_update_script(&media_id, &message));
            }
        }
    });
    use_effect({
        let media_id = media_id.clone();
        move || {
            document::eval(&frame_script(&media_id, &theme_message(&theme())));
        }
    });
    use_hook({
        let media_id = media_id.clone();
        let environment = tab.environment().clone();
        move || {
            spawn(async move {
                let mut heard = document::eval(&format!(
                "window.addEventListener('message', e => {{
                    const f=document.getElementById({media_id:?});
                    if(!f || e.source!==f.contentWindow)return;
                    const m=e.data;
                    if(m && (m.tsumugiConversationCopy===1 || m.tsumugiViewerOpen===1 || m.tsumugiConversationFocus===1))dioxus.send(m);
                }}); await new Promise(() => {{}});"
            ));
                while let Ok(m) = heard.recv::<serde_json::Value>().await {
                    if m["tsumugiConversationFocus"] == 1 {
                        app.dispatch(Msg::WindowFocus(window));
                        app.dispatch(Msg::Focus(tsumugi_core::content::Area::Console));
                    } else if m["tsumugiConversationCopy"] == 1 {
                        if let Some(text) = m["text"].as_str() {
                            let _ = crate::clipboard::copy_to_clipboard(text);
                        }
                    } else if let Some(url) = m["url"].as_str()
                        && (url.starts_with("https://") || url.starts_with("http://"))
                    {
                        app.dispatch(Msg::OpenLink {
                            environment: environment.clone(),
                            url: url.to_string(),
                            modifier: m["modifier"] == true,
                        });
                    }
                }
            })
        }
    });
    use_effect({
        let media_id = media_id.clone();
        move || {
            let Some(cmd) = command() else {
                return;
            };
            if cmd.seq == last.get() {
                return;
            }
            last.set(cmd.seq);
            match state.write().apply(cmd.action) {
                Effect::Scroll(s) => {
                    document::eval(&frame_script(&media_id, &frame_scroll(s)));
                }
                Effect::Close => {
                    app.dispatch(Msg::Window(tsumugi_core::actions::Action::WindowQuit))
                }
                Effect::Reload => app.dispatch(Msg::Relist),
                _ => {}
            }
        }
    });
    use_effect({
        let media_id = media_id.clone();
        move || {
            if let ViewerState::Markdown(s) = state() {
                document::eval(&frame_script(
                    &media_id,
                    &format!("{{\"zoom\":{}}}", s.zoom.factor()),
                ));
            }
        }
    });
    let zoom = match state() {
        ViewerState::Markdown(s) => s.zoom.factor(),
        _ => 1.0,
    };
    let answering = {
        let hosts = app.hosts.read();
        match &tab {
            Tab::Session(environment, id) => {
                hosts
                    .session(environment, id)
                    .is_some_and(|(_, s)| s.state.takes_every_key())
                    || (app.content.read().active() == Some(&tab) && app.console.read().dialog)
            }
            _ => false,
        }
    };
    rsx! {
        div { class: "conversation-screen",
            style { {viewers::CSS} }
            div { class: "conversation-heading",
                span { "Conversation" }
                span { class: "conversation-hint", "You and the assistant" }
                button {
                    onclick: move |_| app.dispatch(Msg::SessionConversation { tab: tab.clone(), shown: false }),
                    if answering { "Answer in console" } else { "Console" }
                }
            }
            div { id: "{box_id}", class: "viewer-box",
                if let Some(doc) = frame_doc() {
                    {page_frame(media_id, box_id.clone(), zoom, 0.0, Some(theme.read().scheme), doc)}
                }
            }
        }
    }
}
