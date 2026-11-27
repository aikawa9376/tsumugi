//! What the work host answers when a window asks and waits: starting
//! sessions and terminals held until the window has told them the theme,
//! the terminal host's standing and its restart, keeping a pasted image,
//! saving the settings.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::watch;
use tsumugi_model::TerminalProgram;
use tsumugi_model::time::now_ms;
use tsumugi_protocol::{
    Answer, Ask, ConfigFile, FileText, GitOp, HostPath, SessionStart, TerminalStart,
};

use crate::delegation::app as delegation;
use crate::execution::{COLS, ROWS, StartRequest, TerminalStartRequest, spawn_may_have_happened};
use crate::scratch;
use crate::serve::Asks;
use crate::terminal_host::{self, Judged};
use crate::watch::Watched;
use crate::work_host::tsumugi_program;
use crate::{daemon, held, pasted, programs, sessions, settings};

mod clone;
mod logins;

use clone::{clone, cloned};
use logins::{delete_login, login_standing, open_login};

/// What answers each ask, through `watched` and the terminal host `judged`
/// stands as.
pub(crate) fn asks(watched: Watched, judged: Arc<watch::Sender<Judged>>) -> Asks {
    Arc::new(move |ask| {
        let (watched, judged) = (watched.clone(), judged.clone());
        Box::pin(async move { answer(&watched, &judged, ask).await })
    })
}

async fn answer(watched: &Watched, judged: &watch::Sender<Judged>, ask: Ask) -> Answer {
    if matches!(
        &ask,
        Ask::StartSession(_)
            | Ask::StartLeader { .. }
            | Ask::StartTerminal(_)
            | Ask::OpenLogin { .. }
            | Ask::Clone { .. }
            | Ask::BringBack { .. }
            | Ask::Run { .. }
    ) {
        let prepared = terminal_host::judged(judged.subscribe()).await;
        let starts = match &ask {
            // A queued issue is started by the host once it can be.
            Ask::Run { args, .. } => {
                args.first().is_some_and(|command| command != "issue")
                    && args.get(1).is_some_and(|op| op == "start")
            }
            _ => true,
        };
        // Existing terminals can be used even when preparing terminfo failed.
        if starts && let Err(text) = prepared {
            return Answer::Failed { text };
        }
    }
    let answered = match ask {
        Ask::BrowserResult { call, result } => {
            watched.browsed(call, result);
            Ok(Answer::Done)
        }
        Ask::QueueLocalIssue(issue) => crate::local_issues::queue(watched, issue).await,
        Ask::CreateLocalIssue(issue) => crate::local_issues::create(watched, issue).await,
        Ask::StartLeader {
            issues,
            max_workers,
        } => crate::leader::start(watched, issues, max_workers).await,
        Ask::StartLocalIssue { id } => crate::local_issues::start_saved(watched, id).await,
        Ask::TryLocalIssue { id, tries } => crate::local_issues::try_as(watched, id, tries)
            .await
            .map(|_| Answer::Done),
        Ask::CancelLocalIssue { id } => crate::local_issues::change(watched, id, false).await,
        Ask::RetryLocalIssue { id } => crate::local_issues::change(watched, id, true).await,
        Ask::CloseLocalIssue { id } => crate::local_issues::close(watched, id).await,
        Ask::ReopenLocalIssue { id } => crate::local_issues::reopen(watched, id).await,
        Ask::DeleteLocalIssue { id } => crate::local_issues::delete(watched, id).await,
        Ask::SaveCommand(new) => crate::saved_commands::save(watched, new).await,
        Ask::DeleteSavedCommand { id } => crate::saved_commands::delete(watched, id).await,
        Ask::SetCheckpoint {
            worktree,
            comment,
            status,
        } => crate::checkpoints::set(watched, worktree, comment, status).await,
        Ask::SaveSchedule(new) => crate::schedules::save(watched, new).await,
        Ask::EditSchedule { id, edit } => crate::schedules::edit(watched, id, edit).await,
        Ask::SetScheduleEnabled { id, enabled } => {
            crate::schedules::enable(watched, id, enabled).await
        }
        Ask::RunSchedule { id } => crate::schedules::run_now(watched, id).await,
        Ask::DeleteSchedule { id } => crate::schedules::delete(watched, id).await,
        Ask::KeepResource {
            dir,
            target,
            label,
            note,
        } => crate::resources::keep(watched, &dir, &target, label, note).await,
        Ask::KeepScreenshot {
            dir,
            data,
            label,
            note,
        } => crate::resources::keep_screenshot(watched, &dir, &data, label, note).await,
        Ask::DescribeResource { id, label, note } => {
            crate::resources::describe(watched, id, label, note).await
        }
        Ask::DropResource { id } => crate::resources::drop(watched, id).await,
        Ask::PathEntries { root, typed } => crate::completion::read(root, typed)
            .await
            .map(|entries| Answer::PathEntries { entries }),
        Ask::PastPreview { past } => Ok(Answer::PastPreview {
            preview: crate::watch::past_preview(*past).await,
        }),
        Ask::Apply { .. } | Ask::MutationStatus { .. } | Ask::MutationEpoch => {
            Err("mutation admission belongs to the work connection".into())
        }
        Ask::SubmitPrompt {
            session,
            agent,
            text,
            images,
            wait,
        } => submit_prompt(&session, agent, &text, &images, wait).await,
        Ask::StartSession(start) => start_session(watched, *start).await,
        Ask::StartTerminal(start) => start_terminal(watched, *start).await,
        Ask::TerminalHost => terminal_host::judged(judged.subscribe())
            .await
            .map(|standing| Answer::TerminalHost { standing }),
        Ask::RestartTerminalHost => {
            let restarted = terminal_host::restart(judged).await;
            watched.relist();
            restarted.map(|standing| Answer::TerminalHost { standing })
        }
        Ask::KeepImage { mime, data } => keep_image(&mime, &data),
        Ask::KeepImageFile { path } => blocking(move || keep_image_file(&path)).await,
        Ask::MakeEntry { path, dir } => {
            let made = blocking(move || crate::making::make(&path, dir)).await;
            // The listing shows what was made without waiting on the disk's watch.
            watched.read_again();
            made.map(|()| Answer::Done)
        }
        Ask::PullWorktree { repository, number } => {
            tsumugi_vcs::github::pull_worktree(&repository, number)
                .await
                .map(|path| Answer::Worktree { path })
        }
        Ask::WorktreeStarts { repository } => tsumugi_vcs::worktree::starts(&repository)
            .await
            .map(|starts| Answer::WorktreeStarts { starts }),
        Ask::CreateWorktree { repository, new } => {
            let made = crate::worktrees::make(&repository, &new).await;
            watched.read_again();
            made.map(|path| Answer::Worktree { path })
        }
        Ask::SetupWorktree {
            repository,
            worktree,
            choice,
        } => {
            let settings = settings::current();
            let done = crate::worktree_setup::set_up(
                &daemon::daemon(),
                &settings,
                &repository,
                &worktree,
                choice,
                true,
            )
            .await;
            // The listing shows what was given without waiting on the disk's watch.
            watched.read_again();
            done.map(|setup| Answer::WorktreeSetup { setup })
        }
        Ask::Sleep { ids } => {
            let slept = crate::hibernation::sleep_now(&ids).await;
            watched.relist();
            slept.map(|(count, left)| Answer::Slept { count, left })
        }
        Ask::CleanupEntries { repository } => {
            // GitHub's word on each branch is asked only where it can be
            // had; without it the entries are listed all the same.
            let pulls = match tsumugi_vcs::github::remote(&repository).await {
                Some(remote) if tsumugi_vcs::github::installed().await => {
                    tsumugi_vcs::github::pull_refs_of(&repository, &remote.slug)
                        .await
                        .unwrap_or_default()
                }
                _ => Vec::new(),
            };
            crate::cleanup::entries(&repository, &crate::gate::folder(), &pulls)
                .await
                .map(|entries| Answer::CleanupEntries { entries })
        }
        Ask::WorktreeStanding { path } => tsumugi_vcs::worktree::standing(&path)
            .await
            .map(|standing| Answer::WorktreeStanding { standing }),
        Ask::DeleteWorktree {
            path,
            force,
            branch,
        } => {
            let removed = tsumugi_vcs::worktree::delete(&path, force, branch).await;
            watched.relist();
            watched.read_again();
            removed.map(|removed| Answer::WorktreeRemoved { removed })
        }
        Ask::MergeStanding { path } => crate::integrate::standing(&path)
            .await
            .map(|standing| Answer::MergeStanding { standing }),
        Ask::MergeWorktree {
            path,
            changes,
            message,
            close_issue,
        } => {
            let merged =
                crate::integrate::merge(&path, changes, message.as_deref(), close_issue).await;
            if matches!(
                &merged,
                Ok(tsumugi_model::Merged::Done {
                    closed: Some(_),
                    ..
                })
            ) {
                // The windows show the issue closed without waiting for the next pass.
                let _ = crate::local_issues::publish(watched).await;
            }
            watched.relist();
            watched.read_again();
            merged.map(|merged| Answer::Merged { merged })
        }
        Ask::ResolveMerge { path } => {
            let resolved = crate::integrate::resolve(Some(watched), &path).await;
            watched.read_again();
            resolved.map(|text| Answer::Told { text })
        }
        Ask::SwitchBranch { worktree, to } => {
            let busy = {
                let host = watched.host();
                working_in(host.work().tree().sessions(), &worktree).map(|s| s.name.clone())
            };
            let switched = match busy {
                Some(name) => Err(format!(
                    "{name} is working in this worktree; switch once it is done"
                )),
                None => tsumugi_vcs::branch::switch(&worktree, &to).await,
            };
            watched.read_again();
            switched.map(|()| Answer::Done)
        }
        Ask::Git { worktree, op } => {
            let made = git_op(&worktree, op).await;
            // The listing shows what was made without waiting on the disk's watch.
            watched.read_again();
            made.map_err(|e| e.to_string())
        }
        Ask::GitHubThread {
            repository,
            listing,
            number,
        } => tsumugi_vcs::github::thread(&repository, listing, number)
            .await
            .map(|thread| Answer::Thread {
                thread: Box::new(thread),
            }),
        Ask::GitHubFixContext {
            repository,
            number,
            checks,
        } => tsumugi_vcs::github::fix_context(&repository, number, checks)
            .await
            .map(|context| Answer::FixContext { context }),
        Ask::GitHubCheckLog { repository, job } => Ok(Answer::CheckLog {
            log: tsumugi_vcs::github::job_log(&repository, job).await,
        }),
        Ask::GitHubWrite {
            repository,
            listing,
            number,
            write,
        } => tsumugi_vcs::github::write(&repository, listing, number, &write)
            .await
            .map(|text| Answer::Told { text }),
        Ask::GenerateText {
            worktree,
            action,
            profile,
            args,
            template,
        } => crate::generate::generate(worktree, action, profile, args, template).await,
        Ask::SaveSettings { text } => save_settings(text),
        Ask::SaveSettingsChecked { text, expected } => save_settings_checked(text, expected),
        Ask::SaveKeys { text } => save_keys(&text),
        Ask::Drafts => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::drafts::load(&dir))
                .await
                .map(|drafts| Answer::Drafts(Box::new(drafts)))
        }
        Ask::KeepDrafts(changes) => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::drafts::change(&dir, *changes))
                .await
                .map(|()| Answer::Done)
        }
        Ask::Review { worktree } => {
            let (dir, kept) = (crate::paths::instance_state(), worktree.clone());
            match blocking(move || crate::review_comments::load(&dir, &kept)).await {
                Ok(comments) => Ok(Answer::Review {
                    comments: crate::review_comments::judge(&worktree, comments).await,
                }),
                Err(text) => Err(text),
            }
        }
        Ask::Viewed { worktree } => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::viewed_files::load(&dir, &worktree))
                .await
                .map(|marks| Answer::Viewed { marks })
        }
        Ask::Written { worktree } => {
            let dir = crate::paths::instance_state();
            blocking(move || {
                crate::written_lines::load(&dir, &worktree, |path| {
                    std::fs::read(worktree.join(path)).ok()
                })
            })
            .await
            .map(|files| Answer::Written { files })
        }
        Ask::MarkViewed {
            worktree,
            mark,
            viewed,
        } => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::viewed_files::set(&dir, &worktree, mark, viewed))
                .await
                .map(|marks| Answer::Viewed { marks })
        }
        Ask::ChangeReview { worktree, change } => {
            let (dir, kept) = (crate::paths::instance_state(), worktree.clone());
            match blocking(move || crate::review_comments::change(&dir, &kept, *change)).await {
                Ok(comments) => Ok(Answer::Review {
                    comments: crate::review_comments::judge(&worktree, comments).await,
                }),
                Err(text) => Err(text),
            }
        }
        Ask::ConsumeDraft { key, text, images } => {
            let dir = crate::paths::instance_state();
            blocking(move || crate::drafts::consume(&dir, &key, &text, &images))
                .await
                .map(|cleared| Answer::DraftConsumed { cleared })
        }
        Ask::Picture { path, whole } => Ok(picture(path, whole).await),
        Ask::DiffPicture {
            worktree,
            rev,
            path,
        } => Ok(diff_picture(&worktree, rev.as_deref(), path).await),
        Ask::Compare { worktree, from, to } => {
            let id = |rev: String| {
                let worktree = worktree.clone();
                async move {
                    tsumugi_vcs::git::commit_id(&worktree, rev.trim())
                        .await
                        .map_err(|e| e.to_string())
                }
            };
            match (id(from).await, id(to).await) {
                (Ok(from), Ok(to)) => Ok(Answer::Compared { from, to }),
                (Err(text), _) | (_, Err(text)) => Err(text),
            }
        }
        Ask::Run { args, cwd, input } => Ok(match args.split_first() {
            Some((command, rest)) if command == "issue" => {
                crate::issue_cli::run(watched, rest, &cwd, input)
                    .await
                    .answer(crate::remote::active())
            }
            Some((command, rest)) if command == "worktree" => {
                if matches!(rest.first().map(String::as_str), Some("set" | "show")) {
                    crate::checkpoints::run(watched, rest, &cwd)
                        .await
                        .answer(crate::remote::active())
                } else {
                    crate::worktree_cli::run(watched, rest, &cwd)
                        .await
                        .answer(crate::remote::active())
                }
            }
            Some((command, rest)) if command == "schedule" => {
                crate::schedule_cli::run(watched, rest, &cwd, input)
                    .await
                    .answer(crate::remote::active())
            }
            _ => run(&args, &cwd, input).await,
        }),
        Ask::LoginStanding { profile, after } => {
            if let Some(terminal) = after {
                ended(terminal).await;
            }
            login_standing(&profile).await
        }
        Ask::OpenLogin {
            profile,
            cols,
            rows,
        } => open_login(watched, &profile, cols, rows).await,
        Ask::DeleteLogin { profile } => delete_login(&profile).await,
        Ask::Clone { typed, cols, rows } => clone(watched, &typed, cols, rows).await,
        Ask::Cloned { path, after } => cloned(&path, after).await,
        Ask::BringBack { ids } => {
            let outcomes = crate::restore::bring_back(ids).await;
            watched.relist();
            outcomes.map(|outcomes| Answer::BroughtBack { outcomes })
        }
        Ask::LetGo { ids } => {
            let gone = crate::restore::let_go(ids).await;
            watched.relist();
            gone.map(|()| Answer::Done)
        }
    };
    answered.unwrap_or_else(|text| Answer::Failed { text })
}

/// What `work` gives, run on a blocking thread; a thread that failed to give
/// it fails with its text.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    crate::environment::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
        .and_then(|done| done)
}

/// An agent's session working in the worktree at `worktree`, whose files a
/// switch of its branch would change under it.
fn working_in<'a>(
    sessions: &'a [tsumugi_model::Session],
    worktree: &std::path::Path,
) -> Option<&'a tsumugi_model::Session> {
    sessions
        .iter()
        .find(|s| s.state == tsumugi_model::State::Working && s.cwd.starts_with(worktree))
}

async fn submit_prompt(
    session: &str,
    agent: tsumugi_model::Agent,
    text: &str,
    images: &[PathBuf],
    wait: Option<u64>,
) -> Result<Answer, String> {
    let id = sessions::id_of(session).ok_or("invalid session ID")?;
    let listed = daemon::daemon()
        .sessions()
        .await
        .map_err(|e| e.to_string())?;
    let actual = sessions::agents(&listed)
        .find(|s| s.id == session)
        .ok_or("the prompt's session no longer exists")?;
    if actual.agent != agent {
        return Err("the prompt's agent does not match the session".into());
    }
    let composed = crate::compose::compose(0, agent, text, images);
    if composed.image_missing {
        return Err("a prompt attachment is missing on the work machine".into());
    }
    // Taken before it is typed, so a second prompt for the same wait is
    // refused however soon it follows; one that names none answers it too.
    match wait {
        Some(wait) if !crate::waits::take(session, wait) => {
            return Err("the session no longer waits for this answer".into());
        }
        Some(_) => {}
        None => crate::waits::answered(session),
    }
    match type_prompt(id, &composed.typing).await {
        Ok(()) => Ok(Answer::Done),
        Err(text) => Ok(Answer::Unconfirmed { text }),
    }
}

fn failed_spawn(error: tsumugi_term::Error, context: &str) -> Result<Answer, String> {
    let text = format!("{context}: {error}");
    if crate::remote::active() && spawn_may_have_happened(&error) {
        Ok(Answer::Unconfirmed { text })
    } else {
        Err(text)
    }
}

pub(crate) async fn type_prompt(
    id: u128,
    steps: &[tsumugi_protocol::Typing],
) -> Result<(), String> {
    use tsumugi_protocol::Typing;
    use tsumugi_term::Input;
    let daemon = daemon::daemon();
    let input: Vec<_> = steps
        .iter()
        .map(|step| match step {
            Typing::Text { text } => Input::Text(text.clone()),
            Typing::Paste { text } => Input::Paste(text.clone()),
            Typing::Key { name } => Input::Key(tsumugi_model::Key::new(name)),
            Typing::Pause { ms } => Input::Pause(std::time::Duration::from_millis((*ms).min(5000))),
        })
        .collect();
    daemon.input(id, &input).await.map_err(|e| e.to_string())
}

/// The command `tsumugi <args>` run as in `cwd` with `input` on its standard
/// input: what it wrote, and its exit status.
async fn run(args: &[String], cwd: &std::path::Path, input: Option<String>) -> Answer {
    let output = match args.split_first() {
        Some((command, rest)) if command == "session" => crate::start::run(rest, cwd, input).await,
        Some((command, rest)) if command == "ports" => crate::ports::run(rest, cwd).await,
        Some((command, rest)) if command == "terminal" => crate::terminals::run(rest, cwd).await,
        Some((command, rest)) if command == "gate" || command == "cleanup" => {
            crate::gate_cli::run(command, rest, cwd).await
        }
        _ => crate::command::Output {
            wrote: Err(format!("the work host runs no command {args:?}")),
            code: 2,
            effects_possible: false,
        },
    };
    output.answer(crate::remote::active())
}

/// The path of `place`, which has to be on this host's own machine: no
/// other host can be reached yet.
fn here(place: HostPath) -> Result<PathBuf, String> {
    match place.on_local() {
        Some(path) => Ok(path.to_path_buf()),
        None => Err(format!(
            "{} is on {}, which cannot be reached",
            place.path.display(),
            place.host.0
        )),
    }
}

/// Start the session `start` asks for, held, as the delegation store records
/// it: a draft's in the scratch folder, made if need be, when it is there.
async fn start_session(watched: &Watched, start: SessionStart) -> Result<Answer, String> {
    let settings = settings::current();
    let verb = if start.resume.is_some() && start.fork {
        "fork"
    } else if start.resume.is_some() {
        "resume"
    } else {
        "start"
    };
    let name = start.name;
    let at = here(start.cwd).map_err(|e| format!("cannot {verb} {name}: {e}"))?;
    let cwd = if start.draft {
        let root = settings::scratch_folder(&settings, &crate::paths::home());
        scratch::start_dir(&at, &root)
            .map_err(|e| format!("cannot make the scratch folder for {name}: {e}"))?
    } else {
        at
    };
    let request = StartRequest {
        agent: start.agent,
        profile: start.profile,
        name: name.clone(),
        name_given: start.name_given,
        cwd,
        model: start.model,
        permissions: start.permissions,
        prompt: start.prompt,
        images: start.images,
        resume: start.resume,
        fork: start.fork,
        home: start.home,
        env: daemon::agent_env(),
        started_ms: now_ms(),
        ..Default::default()
    };
    let resumed = verb != "start";
    let (resolved, recorded) = delegation::prepare(
        &delegation::root(),
        &settings,
        request,
        None,
        &tsumugi_program(),
    )
    .await
    .map_err(|e| {
        if resumed {
            format!("cannot {verb} {name}: {e}")
        } else {
            e
        }
    })?;
    let mut spawn = resolved.spawn;
    (spawn.cols, spawn.rows) = size(start.cols, start.rows);
    let terminal_host = daemon::daemon();
    let id = match terminal_host.spawn_held(spawn).await {
        Ok(id) => id,
        Err(e) => {
            if crate::remote::active() && spawn_may_have_happened(&e) {
                return failed_spawn(e, &format!("cannot {verb} {name}"));
            }
            recorded.failed().await;
            return Err(format!("cannot {verb} {name}: {e}"));
        }
    };
    if let Err(e) = recorded.attach(id).await {
        let _ = terminal_host.end(id).await;
        if crate::remote::active() {
            return Ok(Answer::Unconfirmed {
                text: format!("cannot confirm starting {name}: {e}"),
            });
        }
        return Err(format!("cannot {verb} {name}: {e}"));
    }
    held::hold(id).await;
    watched.relist();
    Ok(Answer::Session {
        id: sessions::hex(id),
    })
}

/// Where the terminal `start` asks for runs: the folder the settings name for a
/// floating one, and otherwise the one asked for.
fn terminal_dir(
    start: &TerminalStart,
    settings: &settings::Settings,
    home: &std::path::Path,
) -> Result<PathBuf, String> {
    if start.floating {
        return Ok(settings.floating_dir(home));
    }
    here(start.cwd.clone())
}

/// Start the shell, Neovim or command `start` asks for, held, and list it at
/// once.
async fn start_terminal(watched: &Watched, start: TerminalStart) -> Result<Answer, String> {
    let settings = settings::current();
    let cwd = terminal_dir(&start, &settings, &crate::paths::home())
        .map_err(|e| format!("cannot open {}: {e}", start.name))?;
    let mut resolved = crate::execution::resolve_terminal(
        &settings,
        TerminalStartRequest {
            name: start.name,
            cwd,
            program: start.program,
            floating: start.floating,
            // Not `agent_env`: a shell or Neovim keeps the user's own editor.
            env: daemon::terminal_env(),
            file: start.file,
            diff: start.diff,
            line: start.line,
            command: start
                .script
                .map(|script| {
                    tsumugi_model::shell_line(
                        &settings.shell.program(&programs::login_shell()),
                        &script,
                    )
                })
                .unwrap_or_default(),
            opener: None,
            show: false,
            restored_from: None,
        },
        &programs::login_shell(),
    );
    crate::execution::prepare_terminal(&resolved.terminal)?;
    (resolved.spawn.cols, resolved.spawn.rows) = size(start.cols, start.rows);
    let id = match daemon::daemon().spawn_held(resolved.spawn).await {
        Ok(id) => id,
        Err(error) => return failed_spawn(error, "cannot start the terminal"),
    };
    held::hold(id).await;
    resolved.terminal.id = id;
    // A command is not run again unasked, so only a shell or a Neovim is
    // kept to be opened again after the terminal host goes.
    if resolved.terminal.program != TerminalProgram::Command {
        let terminal = resolved.terminal.clone();
        let generation = daemon::daemon().generation();
        let kept = delegation::with_store(&delegation::root(), move |store| {
            store.open_terminal(&terminal, generation, now_ms())
        })
        .await;
        if let Err(e) = kept {
            tracing::warn!(
                terminal = %resolved.terminal.name,
                error = %e,
                "cannot keep a terminal to open again"
            );
        }
    }
    watched.terminal_opened(resolved.terminal.clone());
    Ok(Answer::Terminal {
        terminal: resolved.terminal,
    })
}

/// Once the terminal `id` has ended: its program exited, or it is gone; or
/// once a login left open has been waited for long enough. The terminal
/// host tells no one when a program exits, so it is looked at until then.
async fn ended(id: u128) {
    const LONGEST: std::time::Duration = std::time::Duration::from_secs(30 * 60);
    let until = tokio::time::Instant::now() + LONGEST;
    while tokio::time::Instant::now() < until {
        let gone = match daemon::daemon().sessions().await {
            Ok(sessions) => !sessions.iter().any(|s| s.id == id && !s.exited),
            Err(_) => true,
        };
        if gone {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

fn size(cols: u16, rows: u16) -> (u16, u16) {
    (
        if cols == 0 { COLS } else { cols },
        if rows == 0 { ROWS } else { rows },
    )
}

/// Keep a pasted image where the prompt's images are kept, dropping those
/// kept too long.
fn keep_image(mime: &str, data: &str) -> Result<Answer, String> {
    let dir = pasted::dir();
    let now = std::time::SystemTime::now();
    pasted::prune(&dir, now);
    pasted::save(&dir, mime, data, now)
        .map(|path| Answer::Kept { path })
        .map_err(|e| e.to_string())
}

/// Keep a copy of a dropped image file where the prompt's images are kept,
/// dropping those kept too long.
fn keep_image_file(from: &std::path::Path) -> Result<Answer, String> {
    let dir = pasted::dir();
    let now = std::time::SystemTime::now();
    pasted::prune(&dir, now);
    pasted::copy(&dir, from, now)
        .map(|path| Answer::Kept { path })
        .map_err(|e| e.to_string())
}

/// Make the settings `text` writes the settings, kept for the next run.
fn save_settings(text: String) -> Result<Answer, String> {
    let next = settings::read(&ConfigFile {
        path: settings::path(),
        text: FileText::Read { text },
    })?;
    settings::set(next)?;
    Ok(Answer::Done)
}

/// The image file at `path`, whole or as its small copy.
async fn picture(path: std::path::PathBuf, whole: bool) -> Answer {
    let url = crate::environment::spawn_blocking(move || {
        if whole {
            crate::pictures::file_shown(&path)
        } else {
            crate::pictures::file_thumb(&path)
        }
    })
    .await
    .ok()
    .flatten();
    Answer::Picture {
        url: url.map(|u| u.to_string()),
    }
}

/// One version of a picture a diff compares, or none when it is not there.
async fn diff_picture(worktree: &std::path::Path, rev: Option<&str>, path: String) -> Answer {
    let bytes = tsumugi_vcs::picture_version(worktree, rev, &path).await;
    let url = match bytes {
        Some(bytes) => crate::environment::spawn_blocking(move || {
            crate::pictures::shown_bytes(std::path::Path::new(&path), &bytes)
        })
        .await
        .ok()
        .flatten(),
        None => None,
    };
    Answer::Picture {
        url: url.map(|u| u.to_string()),
    }
}

/// What a keys file made without a binding in it starts with.
const KEYS_STARTER: &str = "# Key bindings over the defaults, area by area.\n";

/// Make `text` the key bindings, read again at once.
fn save_keys(text: &str) -> Result<Answer, String> {
    let path = crate::paths::keys_path();
    let text = if text.trim().is_empty() && !path.exists() {
        KEYS_STARTER
    } else {
        text
    };
    crate::config::write(&path, text)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let _ = settings::CHANGED.send(());
    Ok(Answer::Done)
}

fn save_settings_checked(text: String, expected: FileText) -> Result<Answer, String> {
    let path = settings::path();
    std::fs::create_dir_all(path.parent().ok_or("Settings path has no parent")?)
        .map_err(|e| e.to_string())?;
    use tsumugi_sys::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    if crate::config::file(&path).text != expected {
        return Err("Settings changed in another window; reload before saving".into());
    }
    save_settings(text)
}

/// Make `op` in the Git worktree at `worktree`.
async fn git_op(worktree: &std::path::Path, op: GitOp) -> Result<Answer, tsumugi_vcs::Error> {
    use tsumugi_vcs::write;
    let told = |text| Answer::Told { text };
    match op {
        GitOp::Stage { changes } => write::stage(worktree, &changes)
            .await
            .map(|()| Answer::Done),
        GitOp::Unstage { changes } => write::unstage(worktree, &changes)
            .await
            .map(|()| Answer::Done),
        GitOp::Discard { changes } => write::discard(worktree, &changes)
            .await
            .map(|()| Answer::Done),
        GitOp::StageAll => write::stage_all(worktree).await.map(|()| Answer::Done),
        GitOp::Apply {
            patch,
            index,
            reverse,
        } => write::apply_patch(worktree, &patch, index, reverse)
            .await
            .map(|()| Answer::Done),
        GitOp::Commit { message, amend } => {
            write::commit(worktree, &message, amend).await.map(told)
        }
        GitOp::Push { lease } => write::push(worktree, lease).await.map(told),
        GitOp::Pull => write::pull(worktree).await.map(told),
        GitOp::Abort => write::abort(worktree).await.map(told),
        GitOp::Resume => write::resume(worktree).await.map(told),
        GitOp::ChooseBase { base } => write::choose_base(worktree, base.as_deref())
            .await
            .map(|()| Answer::Done),
        GitOp::CherryPick { commit } => write::cherry_pick(worktree, &commit).await.map(told),
        GitOp::Revert { commit } => write::revert(worktree, &commit).await.map(told),
        GitOp::CreateBranch { name, commit } => write::create_branch(worktree, &name, &commit)
            .await
            .map(|()| Answer::Done),
        GitOp::CreatePull {
            base,
            title,
            body,
            draft,
        } => {
            let new = tsumugi_vcs::github::NewPull {
                base,
                title,
                body,
                draft,
            };
            tsumugi_vcs::github::create(worktree, &new)
                .await
                .map(|made| Answer::PullCreated {
                    number: made.number,
                    url: made.url,
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_switch_waits_on_an_agent_working_in_the_worktree_or_below_it() {
        use tsumugi_model::testing::session;
        let sessions = [
            session("idle").cwd("/w").build(),
            session("deep").cwd("/w/sub").working().build(),
            session("elsewhere").cwd("/v").working().build(),
        ];
        assert_eq!(
            working_in(&sessions, "/w".as_ref()).map(|s| s.id.as_str()),
            Some("deep")
        );
        assert_eq!(working_in(&sessions[..1], "/w".as_ref()), None);
        assert_eq!(working_in(&sessions, "/u".as_ref()), None);
    }

    #[test]
    fn a_session_or_terminal_starts_on_this_host_alone() {
        assert_eq!(here(HostPath::local("/w")), Ok(PathBuf::from("/w")));
        let away = HostPath {
            host: tsumugi_protocol::HostName("build-box".into()),
            path: "/w".into(),
        };
        assert_eq!(
            here(away),
            Err("/w is on build-box, which cannot be reached".into())
        );
    }

    #[test]
    fn a_floating_terminal_starts_where_the_settings_say_whatever_directory_is_asked() {
        let home = std::path::Path::new("/h");
        let start = |floating| TerminalStart {
            cwd: HostPath::local("/w"),
            floating,
            ..TerminalStart::default()
        };
        let settings =
            settings::Settings::parse("[floating_terminal]\ndirectory = \"~/src\"\n").unwrap();
        assert_eq!(
            terminal_dir(&start(true), &settings, home),
            Ok(PathBuf::from("/h/src"))
        );
        assert_eq!(
            terminal_dir(&start(false), &settings, home),
            Ok(PathBuf::from("/w"))
        );
        assert_eq!(
            terminal_dir(&start(true), &settings::Settings::default(), home),
            Ok(PathBuf::from("/h"))
        );
    }

    #[tokio::test]
    async fn a_command_is_run_by_its_name_and_one_unknown_or_misspoken_is_refused_as_misused() {
        let cwd = std::path::Path::new("/");
        for args in [
            vec!["daemon".to_string()],
            vec![],
            vec!["terminal".into(), "nonsense".into()],
            vec!["session".into(), "stop".into()],
        ] {
            let Answer::Ran {
                stdout,
                stderr,
                code,
            } = run(&args, cwd, None).await
            else {
                panic!("{args:?} ran nothing");
            };
            assert_eq!((stdout.as_str(), code), ("", 2), "{args:?}");
            assert!(!stderr.is_empty(), "{args:?}");
        }
    }
}
