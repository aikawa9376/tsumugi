//! The agent's browser, as the work host carries it. The pages render in the
//! window, so a command an agent gives is handed to a window as an event and
//! waits for the window's answer; with no window to hand it to, it fails at
//! once and says so.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::oneshot;
use tsumugi_protocol::{BrowserCall, BrowserCommand, BrowserResult, Event};

use crate::work::WindowId;

/// How long a window may take to answer a command: a page that is still
/// loading is waited for by the window itself, within this.
pub(crate) const WAIT: Duration = Duration::from_secs(60);

type Answer = Result<BrowserResult, String>;

#[derive(Debug)]
struct Waiting {
    window: WindowId,
    done: oneshot::Sender<Answer>,
}

/// The commands handed to this host's windows that have not been answered.
#[derive(Debug, Default)]
pub(crate) struct Relay {
    next: AtomicU64,
    waiting: Mutex<HashMap<u64, Waiting>>,
}

impl Relay {
    /// Hand `command`, for the session working in `dir`, to the oldest of
    /// `windows` through `tell`, and wait up to `wait` for its answer.
    pub(crate) async fn call(
        &self,
        windows: &[WindowId],
        tell: impl FnOnce(WindowId, Event),
        dir: PathBuf,
        command: BrowserCommand,
        wait: Duration,
    ) -> Answer {
        let Some(&window) = windows.iter().min() else {
            return Err(format!(
                "No Tsumugi window is open for this environment, and its browser pages render in the window. Open Tsumugi, then run `{}` again.",
                command.name()
            ));
        };
        let call = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (done, answered) = oneshot::channel();
        self.waiting
            .lock()
            .expect("the browser calls' lock")
            .insert(call, Waiting { window, done });
        tell(window, Event::Browser(BrowserCall { call, dir, command }));
        let outcome = tokio::time::timeout(wait, answered).await;
        self.waiting
            .lock()
            .expect("the browser calls' lock")
            .remove(&call);
        match outcome {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err("The window went away before the browser answered".into()),
            Err(_) => Err(format!(
                "The window did not answer within {} seconds",
                wait.as_secs().max(1)
            )),
        }
    }

    /// How many commands wait for a window's answer.
    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.waiting.lock().expect("the browser calls' lock").len()
    }

    /// The window answered the command numbered `call` with `result`.
    /// Whether anything was waiting for it: an answer to a command that timed
    /// out, or that never was, is dropped.
    pub(crate) fn answered(&self, call: u64, result: BrowserResult) -> bool {
        let waiting = self
            .waiting
            .lock()
            .expect("the browser calls' lock")
            .remove(&call);
        waiting.is_some_and(|waiting| waiting.done.send(Ok(result)).is_ok())
    }

    /// `window` closed: what it was handed and had not answered never will
    /// be.
    pub(crate) fn window_left(&self, window: WindowId) {
        let mut waiting = self.waiting.lock().expect("the browser calls' lock");
        let gone: Vec<u64> = waiting
            .iter()
            .filter(|(_, waiting)| waiting.window == window)
            .map(|(call, _)| *call)
            .collect();
        for call in gone {
            if let Some(waiting) = waiting.remove(&call) {
                let _ = waiting.done.send(Err(
                    "The window closed before the browser answered; open it and ask again".into(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn goto() -> BrowserCommand {
        BrowserCommand::Goto {
            url: "http://localhost:3000".into(),
        }
    }

    #[tokio::test]
    async fn with_no_window_open_a_command_fails_at_once_and_says_why() {
        let relay = Relay::default();
        let told = Mutex::new(Vec::new());
        let failed = relay
            .call(
                &[],
                |window, event| told.lock().unwrap().push((window, event)),
                "/work".into(),
                goto(),
                WAIT,
            )
            .await
            .unwrap_err();
        assert!(failed.contains("No Tsumugi window is open"), "{failed}");
        assert!(failed.contains("goto"), "{failed}");
        assert!(told.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_command_goes_to_the_oldest_window_and_waits_for_its_answer() {
        let relay = Arc::new(Relay::default());
        let told = Arc::new(Mutex::new(Vec::new()));
        let calling = {
            let (relay, told) = (relay.clone(), told.clone());
            tokio::spawn(async move {
                relay
                    .call(
                        &[WindowId(7), WindowId(3), WindowId(5)],
                        |window, event| told.lock().unwrap().push((window, event)),
                        "/work".into(),
                        goto(),
                        WAIT,
                    )
                    .await
            })
        };
        let call = loop {
            if let Some((window, Event::Browser(call))) = told.lock().unwrap().first().cloned() {
                assert_eq!(window, WindowId(3));
                assert_eq!(call.dir, PathBuf::from("/work"));
                assert_eq!(call.command, goto());
                break call.call;
            }
            tokio::task::yield_now().await;
        };
        let done = BrowserResult::Done {
            value: serde_json::json!({"url": "http://localhost:3000/"}),
        };
        assert!(relay.answered(call, done.clone()));
        assert_eq!(calling.await.unwrap(), Ok(done));
        // The answer was taken once; a second one has nothing to wake.
        assert!(!relay.answered(call, BrowserResult::Failed { text: "x".into() }));
    }

    #[tokio::test]
    async fn an_answer_to_a_command_nobody_waits_for_is_dropped() {
        let relay = Relay::default();
        assert!(!relay.answered(
            41,
            BrowserResult::Done {
                value: serde_json::Value::Null
            }
        ));
    }

    #[tokio::test]
    async fn a_window_that_closes_fails_what_it_was_handed_and_no_other_window_s() {
        let relay = Arc::new(Relay::default());
        let calls = |window: u64| {
            let relay = relay.clone();
            tokio::spawn(async move {
                relay
                    .call(&[WindowId(window)], |_, _| {}, "/work".into(), goto(), WAIT)
                    .await
            })
        };
        let (first, second) = (calls(1), calls(2));
        while relay.waiting.lock().unwrap().len() < 2 {
            tokio::task::yield_now().await;
        }
        relay.window_left(WindowId(1));
        let failed = first.await.unwrap().unwrap_err();
        assert!(failed.contains("window closed"), "{failed}");
        assert!(!second.is_finished());
        relay.window_left(WindowId(2));
        assert!(second.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn a_window_that_never_answers_is_given_up_on() {
        let relay = Relay::default();
        let failed = relay
            .call(
                &[WindowId(1)],
                |_, _| {},
                "/work".into(),
                goto(),
                Duration::from_millis(20),
            )
            .await
            .unwrap_err();
        assert!(failed.contains("did not answer"), "{failed}");
        assert!(relay.waiting.lock().unwrap().is_empty());
    }
}
