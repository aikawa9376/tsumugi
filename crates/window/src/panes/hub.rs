//! The hub of the panes the window shows, made once: their daemon, the
//! panes themselves, the theme's palette they are drawn in, and the wakeups
//! that redraw them.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use tokio::sync::mpsc;
use tsumugi_protocol::Colours;
use tsumugi_term::Palette;

use super::{close_shell, keep_drawn};
use crate::bridge::AppCtx;
use crate::console::{Hub, Panes};
use crate::content::Tab;
use crate::shell::Local;

/// The palette of `colours`, as the host read them.
pub(crate) fn palette_of(colours: &Colours) -> Palette {
    match colours {
        Colours::Theme { text } => Palette::from_ghostty(text).unwrap_or_default(),
        Colours::Own | Colours::Unreadable { .. } => Palette::default(),
    }
}

/// The hub, made once for the component that calls it and given to the
/// components below it.
pub(crate) fn use_hub(app: AppCtx, local: Local) -> Hub {
    // The theme's palette, which a new theme replaces.
    let palette = use_signal(|| {
        let host = app.host.peek();
        let theme = host
            .work
            .as_ref()
            .and_then(|w| w.config.configured.as_ref());
        theme.map_or_else(Palette::default, |c| palette_of(&c.theme.colours))
    });
    let hub = use_hook(|| {
        let (wake, woken) = mpsc::unbounded_channel::<Tab>();
        let panes: Panes = Rc::new(RefCell::new(HashMap::new()));
        let drawn = Rc::new(tokio::sync::Notify::new());
        spawn(redraw_woken(
            app,
            local,
            panes.clone(),
            drawn.clone(),
            woken,
        ));
        Hub {
            daemons: Rc::new(move |environment| app.daemon_of(environment)),
            panes,
            #[cfg(unix)]
            guis: Rc::new(RefCell::new(HashMap::new())),
            attaching: Rc::new(RefCell::new(HashSet::new())),
            wake,
            drawn,
            palette,
        }
    });
    #[cfg(unix)]
    use_hook({
        let hub = hub.clone();
        move || {
            spawn(super::gui::follow(app, hub));
        }
    });
    use_context_provider({
        let hub = hub.clone();
        move || hub
    });
    hub
}

/// The longest a console's redraws are held back: one frame.
const FRAME: Duration = Duration::from_millis(16);

/// The tabs woken for the next redraw. A wakeup after a quiet frame is
/// answered at once, so a key's echo is not held back; wakeups that follow
/// within a frame of the last redraw wait out the rest of it and come
/// together.
async fn next_wakeups(
    woken: &mut mpsc::UnboundedReceiver<Tab>,
    drawn_at: &mut Option<tokio::time::Instant>,
) -> Option<Vec<Tab>> {
    let first = woken.recv().await?;
    if let Some(at) = *drawn_at {
        tokio::time::sleep_until(at + FRAME).await;
    }
    let mut woke = vec![first];
    while let Ok(t) = woken.try_recv() {
        woke.push(t);
    }
    *drawn_at = Some(tokio::time::Instant::now());
    Some(woke)
}

/// Coalesce wakeups into at most one redraw a frame, and only for the panes
/// on screen; another pane is drawn afresh when it is shown. The images a
/// console drew are kept, and a shell that has exited has its windows
/// closed.
async fn redraw_woken(
    app: AppCtx,
    local: Local,
    panes: Panes,
    drawn: Rc<tokio::sync::Notify>,
    mut woken: mpsc::UnboundedReceiver<Tab>,
) {
    let mut frame = local.frame;
    let mut drawn_at = None;
    while let Some(woke) = next_wakeups(&mut woken, &mut drawn_at).await {
        for tab in &woke {
            if let Tab::Session(environment, id) = tab {
                let drawn = panes
                    .borrow()
                    .get(tab)
                    .map(|p| p.take_drawn())
                    .unwrap_or_default();
                if !drawn.is_empty() {
                    keep_drawn(local.gallery, environment.clone(), id.clone(), drawn);
                }
            }
            let remote = app.remote_status_of(tab.environment()).is_some();
            let gone = panes.borrow().get(tab).is_some_and(|p| {
                if remote {
                    p.program_exited()
                } else {
                    p.exited()
                }
            });
            if let (Tab::Terminal(environment, id), true) = (tab, gone) {
                close_shell(app, &panes, environment, *id);
            }
        }
        let on_screen = app
            .content
            .peek()
            .windows
            .current()
            .is_some_and(|t| t.windows().iter().any(|w| woke.contains(&w.shows)));
        if on_screen {
            *frame.write() += 1;
            drawn.notify_one();
        }
    }
}

impl crate::shell::Ui {
    /// Draw every console open in `colours`: what is drawn, and what a
    /// program asking for its colours is told.
    pub(crate) fn repaint(&self, colours: &Colours) {
        let palette = palette_of(colours);
        for pane in self.hub.panes.borrow().values() {
            pane.set_palette(&palette);
        }
        let mut signal = self.hub.palette;
        signal.set(palette);
    }
}

/// Tell the host each time the screen of the session shown is drawn anew:
/// an agent reports no status for a dialog it opens itself, so the host
/// looks at the screen.
pub(crate) fn use_redraws_told(app: AppCtx, local: Local) {
    let active = use_memo(move || app.content.read().active().cloned());
    use_effect(move || {
        let _ = (local.frame)();
        if let Some(tab @ Tab::Session(_, id)) = &*active.read() {
            let agent = app.agent_of(Some(tab));
            app.tell_host_in(
                tab.environment(),
                tsumugi_protocol::Request::Redrawn {
                    session: id.clone(),
                    agent,
                },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    fn tab(n: u128) -> Tab {
        Tab::Terminal(tsumugi_model::EnvironmentId::new("here"), n)
    }

    #[tokio::test(start_paused = true)]
    async fn a_wakeup_after_a_quiet_frame_is_redrawn_at_once() {
        let (wake, mut woken) = mpsc::unbounded_channel();
        let mut drawn_at = None;
        wake.send(tab(1)).unwrap();
        let start = Instant::now();
        let woke = next_wakeups(&mut woken, &mut drawn_at).await.unwrap();
        assert_eq!(woke, vec![tab(1)]);
        assert_eq!(start.elapsed(), Duration::ZERO);
        tokio::time::sleep(FRAME * 3).await;
        wake.send(tab(1)).unwrap();
        let start = Instant::now();
        next_wakeups(&mut woken, &mut drawn_at).await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn wakeups_within_a_frame_of_a_redraw_wait_it_out_and_come_together() {
        let (wake, mut woken) = mpsc::unbounded_channel();
        let mut drawn_at = None;
        wake.send(tab(1)).unwrap();
        next_wakeups(&mut woken, &mut drawn_at).await.unwrap();
        let drawn = Instant::now();
        tokio::time::sleep(Duration::from_millis(4)).await;
        wake.send(tab(1)).unwrap();
        wake.send(tab(2)).unwrap();
        let woke = next_wakeups(&mut woken, &mut drawn_at).await.unwrap();
        assert_eq!(woke, vec![tab(1), tab(2)]);
        assert_eq!(drawn.elapsed(), FRAME);
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_channel_ends_the_redraws() {
        let (wake, mut woken) = mpsc::unbounded_channel::<Tab>();
        drop(wake);
        assert!(next_wakeups(&mut woken, &mut None).await.is_none());
    }
}
