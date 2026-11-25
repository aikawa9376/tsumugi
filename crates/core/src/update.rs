//! How a message changes the window's state, and what it asks done.

use tsumugi_protocol::Request;

use crate::chrome::{panels, status_line};
use crate::effect::Effect;
use crate::msg::Msg;
use crate::overlay::{self, Menu};
use crate::settle::settle;
use crate::state::ViewState;
use crate::{chrome, console, content, details, host, picker, prompt, tree};

/// `msg` applied to `state`, and the state settled after it.
pub fn step(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    let mut effects = update(state, msg);
    effects.extend(settle(state));
    effects
}

/// `msg` applied to the slices it concerns.
pub fn update(state: &mut ViewState, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::SidebarPanel(panel) => crate::sidebar::switch(state, panel),
        Msg::SidebarSelect(key) => {
            if state.sidebar.rows().iter().any(|row| row.key == key) {
                state.sidebar.selected = Some(key);
            }
            Vec::new()
        }
        Msg::SidebarFold(key) => {
            crate::sidebar::toggle(&mut state.sidebar, key);
            Vec::new()
        }
        Msg::SidebarFilter(text) => {
            state.sidebar.filter = text;
            Vec::new()
        }
        Msg::SidebarFiltering(on) => {
            state.sidebar.filtering = on;
            Vec::new()
        }
        Msg::BoardClock(now_ms) => {
            state.board.tick(now_ms);
            Vec::new()
        }
        Msg::BoardQuery(text) => {
            state.board.query = text;
            Vec::new()
        }
        Msg::BoardProject(name) => {
            crate::board::toggle(&mut state.board.projects, name);
            Vec::new()
        }
        Msg::BoardLane(lane) => {
            crate::board::toggle(&mut state.board.states, lane);
            Vec::new()
        }
        Msg::BoardSelect(environment, id) => {
            state.board.select(environment, id);
            Vec::new()
        }
        Msg::BoardSearching(on) => {
            state.board.searching = on;
            Vec::new()
        }
        Msg::ResourcesClose => {
            state.overlays.resources = None;
            Vec::new()
        }
        Msg::LocalIssuesClose => {
            state.overlays.local_issues = None;
            Vec::new()
        }
        Msg::Opened { home, environment } => {
            state.picker.home = home;
            state.hosts.open(environment.clone());
            vec![host::hello(environment)]
        }
        Msg::EnvironmentOpened { environment, name } => state.hosts.add(environment, name),
        Msg::EnvironmentRemoved(environment) => host::close(state, &environment),
        Msg::OnWorkMachine(environment) => {
            state.hosts.on_work_machine(&environment);
            Vec::new()
        }
        Msg::WindowFocused(on) => {
            if !on {
                state.console.escaped_from = None;
            }
            chrome::window_focused(&mut state.chrome, on)
        }
        Msg::Zoom(step) => chrome::step(&mut state.chrome, step),
        Msg::KeysRead(file) => crate::config::keys_read(state, file),
        Msg::PanelChosen(panel) => crate::config::panel_chosen(&mut state.config, panel),

        Msg::PanelsKept(layout) => panels::kept(&mut state.chrome.panels, layout),
        Msg::BodyResized(body) => panels::resized(&mut state.chrome.panels, body),
        Msg::PanelToggle(side) => panels::toggle(&mut state.chrome.panels, side),
        Msg::PanelShow(side) => panels::show(&mut state.chrome.panels, side),
        Msg::PanelReset(side) => panels::reset(&mut state.chrome.panels, side),
        Msg::PanelDragStart { side, x } => panels::take(&mut state.chrome.panels, side, x),
        Msg::PanelDragMove { x } => panels::drag(&mut state.chrome.panels, x),
        Msg::PanelDragEnd => panels::drop(&mut state.chrome.panels),
        Msg::Status(line) => status_line::set(&mut state.chrome.status, line),
        Msg::Notice(text) => status_line::notice(&mut state.chrome.status, text),
        Msg::NoticeLapsed { notice, text } => {
            status_line::lapsed(&mut state.chrome.status, notice, &text)
        }
        Msg::Failing { last, now } => {
            status_line::failing(&mut state.chrome.status, last.as_deref(), now.as_deref())
        }
        Msg::KeysHelpOpen(here) => chrome::open_keys_help(&mut state.chrome, here),
        Msg::KeysHelpClose => chrome::close_keys_help(&mut state.chrome),

        Msg::Host { environment, event } => host::told(state, environment, event),

        Msg::Relist => state.hosts.ask_each(Request::Relist),
        Msg::Typed {
            environment,
            inputs,
        } => vec![Effect::Host {
            environment,
            request: Request::Typed { inputs },
        }],
        Msg::TerminalOpened {
            environment,
            terminal,
        } => vec![Effect::Host {
            environment,
            request: Request::TerminalOpened { terminal },
        }],
        Msg::TerminalFailed { opening } => {
            content::not_started(&mut state.content, opening);
            Vec::new()
        }
        Msg::TerminalClosed { environment, id } => vec![Effect::Host {
            environment,
            request: Request::TerminalClosed { id },
        }],

        Msg::ContentOpened { kept, scratch } => content::opened(&mut state.content, kept, scratch),
        Msg::Focus(area) => {
            crate::floating::blur(state);
            // The message being written for a commit is given up with the prompt.
            if area != crate::content::Area::Prompt {
                state.prompt.committing = None;
                state.prompt.pulling = None;
                state.prompt.basing = None;
                state.prompt.branching = None;
                state.prompt.writing = None;
            }
            content::focus(&mut state.content, area)
        }
        Msg::Show { thing, place, how } => {
            let local = state.hosts.is_local(thing.environment());
            content::show(&mut state.content, thing, place, how, local)
        }
        Msg::ShowSession(environment, id) => content::show_session(state, environment, id),
        Msg::Draft { worktree, write } => content::draft(
            &mut state.content,
            state.hosts.environment.clone(),
            worktree,
            write,
        ),
        Msg::OpenRow {
            item,
            how,
            take_keys,
        } => content::open_row(state, item, how, take_keys),
        Msg::DraftWritten {
            environment,
            worktree,
            prompt,
        } => content::draft_written(state, environment, worktree, prompt),
        Msg::EditFile(environment, path) => content::edit_file(state, environment, path),
        Msg::OpenFile(environment, path) => {
            content::open_file(state, environment, path, content::Opening::Existing)
        }
        Msg::OpenFileIn(environment, path, how) => {
            content::open_file(state, environment, path, how)
        }
        Msg::OpenDiff {
            environment,
            worktree,
            spot,
            neovim,
        } => {
            let place = crate::tree::Place::at(&environment, &worktree);
            if neovim {
                vec![Effect::StartDiffEditor { place, spot }]
            } else {
                state.content.go_to_place(place);
                state.content.windows.open_tab(content::Tab::Diff {
                    environment,
                    worktree,
                    spot: Box::new(spot),
                });
                state.content.focus = content::Area::Console;
                Vec::new()
            }
        }
        Msg::DiffView {
            environment,
            worktree,
            choice,
        } => {
            state
                .content
                .diff_views
                .choose(&environment, &worktree, choice);
            Vec::new()
        }
        Msg::DiffUnfold {
            environment,
            worktree,
            path,
        } => {
            state
                .content
                .diff_views
                .unfold(&environment, &worktree, &path);
            Vec::new()
        }
        Msg::RowClicked(item) => content::click_row(state, item),
        Msg::Window(action) if crate::floating::is_own(&action) => {
            crate::floating::act(state, action)
        }
        Msg::Window(action) => content::act(state, action),
        Msg::OpenLink {
            environment,
            url,
            modifier,
        } => crate::browser::open_link(state, environment, url, modifier),
        Msg::WindowFocus(id) => content::focus_window(state, id),
        Msg::TabShow(at) => content::show_tab(state, at),
        Msg::SessionConversation { tab, shown } => content::show_conversation(state, tab, shown),
        Msg::TabReveal(tab) => content::reveal(state, &tab),
        Msg::Replace { from, to } => content::replace(&mut state.content, &from, to),
        Msg::ShellClosed { environment, id } => {
            let mut effects = content::shell_closed(
                &mut state.content,
                &environment,
                state.hosts.work(&environment),
                id,
            );
            effects.insert(
                0,
                Effect::Host {
                    environment,
                    request: Request::TerminalClosed { id },
                },
            );
            effects
        }
        Msg::WindowDragStart { border, from, px } => {
            content::drag_start(&mut state.content, border, from, px)
        }
        Msg::WindowDragMove(pos) => content::drag_move(&mut state.content, pos),
        Msg::WindowDragEnd => content::drag_end(&mut state.content),
        Msg::WindowMoveStart { window, tab, x, y } => {
            content::move_start(&mut state.content, window, tab, (x, y))
        }
        Msg::WindowMoveOver { x, y, over } => content::move_over(&mut state.content, (x, y), over),
        Msg::WindowMoveEnd => content::move_end(&mut state.content),
        Msg::OpenLogin {
            environment,
            profile,
        } => vec![Effect::OpenLogin {
            environment,
            profile,
        }],
        Msg::Cloned {
            environment,
            dir,
            then,
        } => picker::cloned(state, environment, dir, then),
        Msg::DraftProfile { tab, profile } => {
            let found = state
                .config
                .settings
                .find_profile(&profile)
                .map(std::borrow::Cow::into_owned);
            let Some(chosen) = found else {
                return status_line::notice(
                    &mut state.chrome.status,
                    format!(
                        "Profile `{profile}` is no longer defined; choose another in the draft or under Settings → Agents"
                    ),
                );
            };
            let from = content::agent_of(state, tab.as_ref());
            match content::choose_profile(&mut state.content, tab, &chosen, from) {
                Some(told) => status_line::notice(&mut state.chrome.status, told),
                None => Vec::new(),
            }
        }
        Msg::DraftOptions { tab, options } => {
            content::choose_options(&mut state.content, tab, options)
        }
        Msg::PanesLive(live) => content::tell_live(&mut state.content, live),

        Msg::TreeSelect(row) => tree::select(&mut state.tree, row),
        Msg::TreeKey(key) => tree::move_cursor(state, key),
        Msg::TreeFold { row, open } => tree::fold(&mut state.tree, &row, open),
        Msg::TreeFoldToggle(row) => tree::toggle(&mut state.tree, &row),
        Msg::TreeFilter(text) => tree::filter(&mut state.tree, text),
        Msg::TreeFiltering(on) => tree::filtering(&mut state.tree, on),
        Msg::TreeEveryWorktree => tree::every_worktree(&mut state.tree, &state.hosts),

        Msg::DetailsShown(told) => details::tell(&mut state.details, *told),
        Msg::DetailsTab(tab) => details::show_tab(&mut state.details, tab),
        Msg::DetailsTabStep(delta) => details::step_tab(&mut state.details, delta),
        Msg::DetailsShowComponent(c) => details::show_component(&mut state.details, c),
        Msg::DetailsKey(details::Key::Discard) => overlay::ask_discard(state),
        Msg::DetailsKey(details::Key::Amend) => overlay::ask_rewrite(state, true),
        Msg::DetailsKey(details::Key::ForcePush) => overlay::ask_rewrite(state, false),
        Msg::DetailsKey(details::Key::CreatePull) => match state.details.shown.worktree.clone() {
            Some(worktree) => {
                let environment = state.details.shown.environment.clone();
                prompt::pull_request::begin(state, environment, worktree)
            }
            None => Vec::new(),
        },
        Msg::DetailsKey(details::Key::DraftPull) => {
            crate::ai::generate(state, tsumugi_model::AiAction::PullRequest)
        }
        Msg::DetailsKey(details::Key::GenerateCommit) => {
            crate::ai::generate(state, tsumugi_model::AiAction::CommitMessage)
        }
        Msg::DetailsKey(details::Key::FixCommit) => crate::ai::fix_commit_failure(state),
        Msg::GeneratedText {
            environment,
            worktree,
            action,
            result,
        } => crate::ai::generated(state, environment, worktree, action, result),
        Msg::CommitRefused {
            environment,
            worktree,
            message,
            output,
        } => crate::ai::commit_refused(state, environment, worktree, message, output),
        Msg::CommitMade {
            environment,
            worktree,
        } => crate::ai::commit_made(state, environment, worktree),
        Msg::DetailsKey(details::Key::Abort) => overlay::ask_abort(state),
        Msg::DetailsKey(details::Key::Base) => match state.details.shown.worktree.clone() {
            Some(worktree) => {
                let environment = state.details.shown.environment.clone();
                prompt::base::begin(state, environment, worktree)
            }
            None => Vec::new(),
        },
        Msg::DetailsKey(details::Key::Resolve) => crate::ai::resolve_conflicts(state),
        Msg::DetailsKey(details::Key::Neovim) => details::open_in_neovim(state),
        Msg::DetailsKey(
            key @ (details::Key::LogCopyHash
            | details::Key::LogOpenRemote
            | details::Key::LogExplain
            | details::Key::LogBranch
            | details::Key::LogWorktree
            | details::Key::LogCherryPick
            | details::Key::LogRevert),
        ) => details::press_on_commit(state, key),
        Msg::DetailsKey(details::Key::Commit) => match state.details.shown.worktree.clone() {
            Some(worktree) => {
                let environment = state.details.shown.environment.clone();
                prompt::begin_commit(state, environment, worktree, false)
            }
            None => Vec::new(),
        },
        Msg::DetailsKey(key) => {
            let environment = content::shown_environment(state);
            details::press(&mut state.details, state.hosts.work(&environment), key)
        }
        Msg::DetailsClick(at) => {
            let environment = content::shown_environment(state);
            details::click(&mut state.details, state.hosts.work(&environment), at)
        }
        Msg::DetailsOpenRow(at) => {
            let environment = content::shown_environment(state);
            details::open_row(&mut state.details, state.hosts.work(&environment), at)
        }
        Msg::DetailsSelect(at) => {
            let environment = content::shown_environment(state);
            details::select(&mut state.details, state.hosts.work(&environment), at)
        }
        Msg::DetailsBack => details::back(&mut state.details),
        Msg::DetailsReadAgain => vec![Effect::Host {
            environment: content::shown_environment(state),
            request: Request::ReadAgain,
        }],
        Msg::CommandsWanted => vec![Effect::Host {
            environment: content::shown_environment(state),
            request: Request::CommandsWanted,
        }],

        Msg::PickerOpen => picker::open(state, picker::Purpose::Go),
        Msg::PickerAddOpen => picker::open(state, picker::Purpose::Add),
        Msg::ResumeOpen => picker::open_resume(state),
        Msg::InterruptedOpen(environment) => picker::open_interrupted(state, environment),
        Msg::NoticesOpen => crate::notices::open(state),
        Msg::ReadsRestored(reads) => crate::notices::restore(state, reads),
        Msg::MutesRestored(mutes) => crate::notices::mute::restore(state, mutes),
        Msg::OrganizeRestored(organize) => tree::restore_organize(&mut state.tree, organize),
        Msg::SessionMarkUnread { environment: _, id } => crate::unread::mark(state, &id),
        Msg::NotificationsMute { environment, path } => {
            crate::notices::mute::toggle(state, &environment, &path)
        }
        Msg::PickerClose => picker::close(&mut state.picker),
        Msg::PickerQuery(text) => picker::query(&mut state.picker, text),
        Msg::PickerKey(key) => picker::press(state, &key),
        Msg::PickerChoose(at) => picker::choose(state, at),
        Msg::PickerScope(at) => picker::scope(&mut state.picker, at),
        Msg::PickerDir(dir) => picker::start_in(state, dir),
        Msg::PickerEvery => picker::every(&mut state.picker),
        Msg::CommandsOpen => picker::commands::open(state),
        Msg::OpenBehind(tab) => {
            state.content.windows.open_behind(tab);
            Vec::new()
        }
        Msg::CommandsTyped(typed) => picker::commands::type_into(state, typed),

        msg @ (Msg::Key { .. }
        | Msg::Text { .. }
        | Msg::ImagePasted { .. }
        | Msg::FilesDropped(_)
        | Msg::ConsoleTyped
        | Msg::InputMayHold(_)
        | Msg::SequenceDropped
        | Msg::ConsoleSendNext
        | Msg::ConsolePassEveryKey
        | Msg::ReadingStarted
        | Msg::ReaderQuit
        | Msg::PromptRuns(_)
        | Msg::EditorAnswers
        | Msg::AskInput
        | Msg::InputAsked(_)
        | Msg::PullLapsed(_)
        | Msg::InputTaken) => console::update(state, msg),

        msg @ (Msg::Recall { .. }
        | Msg::PromptMode(..)
        | Msg::PromptShell(_)
        | Msg::PromptImages(_)
        | Msg::Drafted(_)
        | Msg::ImageSelect(_)
        | Msg::ImagesChoose
        | Msg::PromptSend
        | Msg::PromptEmpty
        | Msg::PromptTaken(_)
        | Msg::PromptWent(_)
        | Msg::PullCreated { .. }
        | Msg::GitHubCompose(_)
        | Msg::GitHubWrite(_)
        | Msg::GitHubAsk(_)
        | Msg::GitHubFix(_)
        | Msg::GitHubFixRead { .. }
        | Msg::GitHubWritten { .. }
        | Msg::PromptRetained
        | Msg::ComposeLapsed(_)) => prompt::update(state, msg),

        Msg::Perform { action, key, probe } => crate::perform::perform(state, action, key, &probe),

        Msg::HeldKey(key) => {
            state.console.escaped_from = None;
            overlay::press(state, key)
        }
        Msg::MenuOpen {
            x,
            y,
            target,
            reach,
        } => overlay::open_menu(
            state,
            Menu {
                x,
                y,
                target,
                reach,
                cursor: None,
            },
        ),
        Msg::MenuClose => overlay::close_menu(state),
        Msg::MenuRun { target, action } => overlay::run_menu(state, target, action),
        Msg::LightboxOpen(thing) => overlay::lightbox::open(state, thing),
        Msg::LightboxAction(action) => overlay::lightbox::act(state, action),
        Msg::LightboxPan { dx, dy } => overlay::lightbox::pan(state, dx, dy),
        Msg::SwitcherRelease => overlay::switcher::commit(state),
        Msg::FloatingFocus => {
            crate::floating::focus(state);
            Vec::new()
        }
        Msg::FloatingClose => {
            crate::floating::close(state);
            Vec::new()
        }
        Msg::FloatingBlur => {
            crate::floating::blur(state);
            Vec::new()
        }
        Msg::FloatingSelect { environment, id } => {
            crate::floating::select(state, environment, id);
            Vec::new()
        }
        Msg::FloatingStarted { environment, id } => {
            crate::floating::started(state, environment, id);
            Vec::new()
        }
        Msg::FloatingFailed { environment } => {
            crate::floating::failed(state, &environment);
            Vec::new()
        }
        Msg::AddingOpen => overlay::adding(state, true),
        Msg::AddingClose => overlay::adding(state, false),
        Msg::Confirm(ask) => overlay::ask(state, *ask),
        Msg::ConfirmStop { selection } => overlay::ask_stop(state, selection),
        Msg::ConfirmAnswer(yes) => overlay::answer(state, yes),
        Msg::ConfirmToggle => crate::worktree::toggle(state),
        Msg::ComposerOpen(row) => crate::worktree::open(state, &row),
        Msg::ComposerText { field, text } => crate::worktree::type_into(state, field, text),
        Msg::ComposerFocus(field) => crate::worktree::focus(state, field),
        Msg::ComposerKey(key) => crate::worktree::composer_key(state, &key),
        Msg::ComposerCursor(at) => crate::worktree::cursor_at(state, at),
        Msg::Slept { result } => crate::worktree::slept(state, result),
        Msg::CleanupKey(key) => crate::worktree::cleanup_key(state, &key),
        Msg::CleanupToggle(at) => crate::worktree::cleanup_toggle(state, at),
        Msg::CleanupRead {
            environment,
            repository,
            result,
        } => crate::worktree::cleanup_read(state, &environment, &repository, result),
        Msg::WorktreeStarts {
            environment,
            repository,
            result,
        } => crate::worktree::read_starts(state, &environment, &repository, result),
        Msg::WorktreeMade { id, result } => crate::worktree::made(state, id, result),
        Msg::WorktreeSetUp { id, result } => crate::worktree::set_up(state, id, result),
        Msg::WorktreeDismiss(id) => crate::worktree::dismiss(state, id),
        Msg::WorktreeInspected {
            environment,
            path,
            result,
        } => crate::worktree::inspected(state, environment, path, result),
        Msg::WorktreeDeleted { path, result } => crate::worktree::removed(state, &path, result),
        Msg::WorktreeMergeInspected {
            environment,
            path,
            result,
        } => crate::worktree::merge_inspected(state, environment, path, result),
        Msg::WorktreeMerged {
            environment,
            path,
            result,
        } => crate::worktree::merged(state, environment, &path, result),
        Msg::WorktreeResolved { path, result } => crate::worktree::resolved(state, &path, result),
        Msg::RenameStart {
            environment,
            id,
            place,
        } => overlay::start_rename(state, &environment, &id, place),
        Msg::RenameText {
            environment,
            id,
            place,
            text,
        } => overlay::type_name(state, &environment, &id, place, text),
        Msg::RenameEnd {
            environment,
            id,
            place,
            commit,
        } => overlay::end_rename(state, &environment, &id, place, commit),
        Msg::NamingStart { row, kind } => overlay::start_naming(state, &row, kind),
        Msg::NamingText(text) => overlay::type_naming(state, text),
        Msg::NamingEnd { commit } => overlay::end_naming(state, commit),
        Msg::EntryMade {
            environment,
            worktree,
            path,
            dir,
        } => tree::made(&mut state.tree, &environment, &worktree, &path, dir),
        Msg::DaemonStanding { environment, note } => {
            overlay::daemon_standing(state, environment, note)
        }
        Msg::DaemonRestarted { environment, note } => {
            overlay::daemon_restarted(state, environment, note)
        }
        Msg::BannerClose => overlay::close_banner(state),
        Msg::BringBackAll => overlay::bring_back_all(state, false),
        Msg::LetGoAll => overlay::bring_back_all(state, true),
        Msg::InterruptedLater => overlay::interrupted_later(state),
        Msg::BroughtBack { outcomes, .. } => overlay::brought_back(state, &outcomes),
    }
}
