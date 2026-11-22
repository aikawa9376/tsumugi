//! A window on the log of a check's Actions job: the end of it, as a fix
//! request quotes it, scrolled to the first error, read with the viewer's
//! keys.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use tsumugi_core::actions::Action;
use tsumugi_core::content::{Area, WindowId};
use tsumugi_core::msg::Msg;
use tsumugi_core::viewers::ViewerAction;
use tsumugi_model::EnvironmentId;
use tsumugi_model::github::{CheckLog, Log, Write};
use tsumugi_protocol::{Answer, Ask};

use crate::bridge::AppCtx;

static NEXT: AtomicU64 = AtomicU64::new(1);

/// A window on the log of the Actions job `job` on the pull request `number`
/// of the repository at `repository`, read when it opens and again whenever
/// the keys ask; its run is what the keys run again.
#[component]
pub(crate) fn CheckLogWindow(
    window: WindowId,
    environment: EnvironmentId,
    repository: PathBuf,
    number: u64,
    job: std::sync::Arc<tsumugi_core::content::CheckJob>,
    commands: ReadSignal<Option<(WindowId, crate::viewers::Command)>>,
) -> Element {
    let app = use_context::<AppCtx>();
    let command = use_memo(move || commands().filter(|(w, _)| *w == window).map(|(_, c)| c));
    let id = use_hook(|| format!("check-log-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    let mut read = use_signal(|| None::<CheckLog>);
    let mut reads = use_signal(|| 0_u64);
    let (job_id, run, name) = (job.id, job.run, job.name.clone());
    let last =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(command.peek().map_or(0, |c| c.seq))));

    use_effect({
        let (environment, repository) = (environment.clone(), repository.clone());
        move || {
            let _ = reads();
            let asked = app.ask_in(
                &environment,
                Ask::GitHubCheckLog {
                    repository: repository.clone(),
                    job: job_id,
                },
            );
            spawn(async move {
                let log = match asked.await {
                    Answer::CheckLog { log } => log,
                    Answer::Failed { text } => unread(text),
                    other => unread(format!("the work host answered {other:?}")),
                };
                read.set(Some(log));
            });
        }
    });

    // What was read is scrolled to its first error, or its end, where a log's
    // reader looks first.
    use_effect({
        let id = id.clone();
        move || {
            let Some(log) = read() else { return };
            let motion = match log.first_error {
                Some(line) => {
                    format!(
                        "const l=document.getElementById('{id}-{line}');if(l)l.scrollIntoView({{block:'center'}});"
                    )
                }
                None => "e.scrollTo(0,e.scrollHeight);".to_string(),
            };
            document::eval(&format!(
                "{{const e=document.getElementById('{id}');if(e){{{motion}}}}}"
            ));
        }
    });

    use_effect({
        let id = id.clone();
        move || {
            let Some(c) = command() else { return };
            if last.replace(c.seq) == c.seq {
                return;
            }
            let motion = match c.action {
                ViewerAction::Close => {
                    app.dispatch(Msg::Window(Action::WindowQuit));
                    return;
                }
                ViewerAction::Reload => {
                    reads += 1;
                    return;
                }
                ViewerAction::Rerun => {
                    rerun(app, run);
                    return;
                }
                ViewerAction::CopyPath => {
                    if let Some(CheckLog {
                        log: Log::Excerpt { text, .. },
                        ..
                    }) = read.peek().as_ref()
                    {
                        let _ = crate::clipboard::copy_to_clipboard(text);
                    }
                    return;
                }
                ViewerAction::ScrollDown => "e.scrollBy(0,24)",
                ViewerAction::ScrollUp => "e.scrollBy(0,-24)",
                ViewerAction::ScrollHalfDown => "e.scrollBy(0,e.clientHeight/2)",
                ViewerAction::ScrollHalfUp => "e.scrollBy(0,-e.clientHeight/2)",
                ViewerAction::ScrollTop => "e.scrollTo(0,0)",
                ViewerAction::ScrollBottom => "e.scrollTo(0,e.scrollHeight)",
                _ => return,
            };
            document::eval(&format!(
                "{{const e=document.getElementById('{id}');if(e){{{motion}}}}}"
            ));
        }
    });

    let body = match read() {
        None => rsx! { p { class: "check-log-note", "Reading…" } },
        Some(CheckLog {
            log: Log::Unread { why },
            ..
        }) => rsx! { p { class: "check-log-note", "{why}" } },
        Some(CheckLog {
            log: Log::Excerpt { text, cut },
            first_error,
        }) => rsx! {
            if cut {
                p { class: "check-log-note", "Only the end of the log is shown; earlier lines were left out." }
            }
            pre { class: "check-log-text",
                for (n , line) in text.split('\n').map(str::to_owned).enumerate() {
                    div {
                        key: "{n}",
                        id: "{id}-{n}",
                        class: if first_error == Some(n) { "check-log-line first-error" } else { "check-log-line" },
                        "{line}"
                    }
                }
            }
        },
    };
    rsx! {
        div { class: "diff-review check-log",
            onmousedown: move |_| app.dispatch(Msg::Focus(Area::Console)),
            div { class: "diff-review-toolbar",
                span { class: "check-log-name", "{name}" }
                span { class: "check-log-for", "PR #{number}" }
                button {
                    onclick: move |_| {
                        app.dispatch(Msg::WindowFocus(window));
                        rerun(app, run);
                    },
                    "Re-run failed jobs"
                }
                button { onclick: move |_| reads += 1, "Reload" }
            }
            div { id: "{id}", class: "diff-review-scroll check-log-scroll", {body} }
        }
    }
}

fn unread(why: String) -> CheckLog {
    CheckLog {
        log: Log::Unread { why },
        first_error: None,
    }
}

/// Ask whether to run the failed jobs of `run` again, or say why that
/// cannot be asked.
fn rerun(app: AppCtx, run: Option<u64>) {
    match run {
        Some(run) => app.dispatch(Msg::GitHubAsk(Write::Rerun { run })),
        None => app.notice(
            "This check's address does not name its Actions run, so its jobs cannot be run again here; open it on GitHub"
                .into(),
        ),
    }
}
