//! The commands a session's agent sends that change or read what Tsumugi
//! keeps: its timeline, its name, its end, and the sessions it opens or lists.
use std::time::Duration;

use serde_json::Value;
use tsumugi_model::timeline::{Entry, Timeline};
use tsumugi_term::tags;

use super::Context;
use super::serve::answer;
use crate::delegation::{Code, Error, Result, Session, protocol};
use crate::starting::Stage;

pub(super) async fn create_local_issue(
    context: &Context,
    caller: &Session,
    asked: protocol::CreateLocalIssue,
) -> Result<Value> {
    let dir = asked.dir.unwrap_or_else(|| caller.cwd.clone());
    let dir = if dir.is_absolute() {
        dir
    } else {
        caller.cwd.join(dir)
    };
    let dir = tsumugi_sys::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| {
            Error::new(
                Code::Invalid,
                "Choose an existing working directory on the work machine",
            )
        })?;
    let issue = tsumugi_model::NewLocalIssue {
        id: tsumugi_model::SessionId::new().to_string(),
        dir,
        title: asked.title,
        body: asked.body,
    };
    let saved = context
        .call(move |store| {
            let id = issue.id.clone();
            store.add_local_issue(issue)?;
            store
                .local_issues()?
                .into_iter()
                .find(|issue| issue.id == id)
                .ok_or_else(|| Error::new(Code::Storage, "The saved issue could not be read"))
        })
        .await?;
    answer(saved)
}

/// The local issues of the environment, narrowed by state and directory.
pub(super) async fn local_issues(
    context: &Context,
    caller: &Session,
    asked: protocol::LocalIssues,
) -> Result<Value> {
    let state = asked
        .state
        .as_deref()
        .map(|word| {
            tsumugi_model::LocalIssueState::parse(word).ok_or_else(|| {
                Error::new(
                    Code::Invalid,
                    format!("{word} is no state of a local issue"),
                )
            })
        })
        .transpose()?;
    let dir = asked.dir.map(|dir| {
        let dir = caller.cwd.join(dir);
        tsumugi_sys::fs::canonicalize(&dir).unwrap_or(dir)
    });
    let all = context.call(|store| store.numbered_local_issues()).await?;
    let issues: Vec<Value> = all
        .iter()
        .filter(|(_, issue)| state.is_none_or(|state| issue.state == state))
        .filter(|(_, issue)| dir.as_ref().is_none_or(|dir| issue.dir.starts_with(dir)))
        .map(|(number, issue)| crate::local_issues::view(*number, issue))
        .collect();
    Ok(serde_json::json!({"issues": issues}))
}

/// One local issue, with its request.
pub(super) async fn local_issue(
    context: &Context,
    asked: protocol::LocalIssueName,
) -> Result<Value> {
    let (number, issue) = context
        .call(move |store| store.local_issue_named(&asked.issue))
        .await?;
    Ok(serde_json::json!({"issue": crate::local_issues::view(number, &issue)}))
}

/// Close, reopen or cancel a local issue. Starting one authorizes an agent
/// to work, and deleting one loses its request, so those stay with the
/// user.
pub(super) async fn change_local_issue(
    context: &Context,
    action: tsumugi_model::LocalIssueAction,
    asked: protocol::LocalIssueName,
) -> Result<Value> {
    use tsumugi_model::LocalIssueAction as Action;
    if !matches!(action, Action::Close | Action::Reopen | Action::Cancel) {
        return Err(Error::new(
            Code::Refused,
            format!(
                "An agent does not {} a local issue; the user does",
                action.word()
            ),
        ));
    }
    let (number, issue) = context
        .call(move |store| store.act_on_local_issue(&asked.issue, action))
        .await?;
    Ok(serde_json::json!({"issue": crate::local_issues::view(number, &issue)}))
}

/// The project a command names by `dir`, or else the caller's: the root of
/// the workspace holding the directory.
async fn project_of(
    caller: &Session,
    dir: Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf> {
    let dir = dir.unwrap_or_else(|| caller.cwd.clone());
    let dir = if dir.is_absolute() {
        dir
    } else {
        caller.cwd.join(dir)
    };
    let dir = tsumugi_sys::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| {
            Error::new(
                Code::Invalid,
                "Choose an existing directory on the work machine",
            )
        })?;
    Ok(crate::workspaces::root_of(&dir).await)
}

/// What the user keeps in the project that holds `words`, each said to
/// exist or not when it is a path on this machine.
pub(super) async fn resources(
    context: &Context,
    caller: &Session,
    asked: protocol::Resources,
) -> Result<Value> {
    let project = project_of(caller, asked.dir).await?;
    let kept = {
        let project = project.clone();
        context
            .call(move |store| store.resources(Some(&project)))
            .await?
    };
    let query = asked.query.unwrap_or_default();
    let listed: Vec<Value> = kept
        .iter()
        .filter(|resource| resource.matches(&query))
        .map(|resource| {
            let path = resource.reach() == tsumugi_model::Reach::Path;
            serde_json::json!({
                "id": resource.id,
                "title": resource.title(),
                "target": resource.target,
                "kind": resource.reach(),
                "note": resource.note,
                "exists": path.then(|| std::path::Path::new(&resource.target).exists()),
            })
        })
        .collect();
    Ok(serde_json::json!({"project": project, "resources": listed}))
}

/// Keep a path or an address in the caller's project, a relative path
/// as one of the caller's directory.
pub(super) async fn add_resource(
    context: &Context,
    caller: &Session,
    asked: protocol::AddResource,
) -> Result<Value> {
    let project = project_of(caller, asked.dir).await?;
    let target = asked.target.trim().to_owned();
    let target = match tsumugi_model::Reach::of(&target) {
        tsumugi_model::Reach::Path if !std::path::Path::new(&target).is_absolute() => {
            caller.cwd.join(&target).to_string_lossy().into_owned()
        }
        _ => target,
    };
    let new = tsumugi_model::NewResource {
        id: tsumugi_model::SessionId::new().to_string(),
        project,
        target,
        label: asked.label,
        note: asked.note,
    };
    let kept = context.call(move |store| store.add_resource(new)).await?;
    answer(kept)
}

/// Stop keeping the resource of `asked`'s id.
pub(super) async fn remove_resource(
    context: &Context,
    asked: protocol::RemoveResource,
) -> Result<Value> {
    context
        .call(move |store| store.remove_resource(&asked.id))
        .await?;
    answer(serde_json::json!({"removed": true}))
}

/// Make the changes the caller's agent asks of its timeline and journal
/// them; the trees as they now stand, with each change that could not be
/// made. A call none of whose changes could be made is refused and left out
/// of the journal.
pub(super) async fn record_timeline(
    context: &Context,
    caller: &Session,
    asked: protocol::TimelineTasks,
) -> Result<Value> {
    if asked.ops.is_empty() {
        return Err(Error::new(Code::Invalid, "ops names no change to make"));
    }
    let code = caller.code.clone();
    context
        .call(move |store| {
            let mut timeline = Timeline::replay(&store.timeline_of(&code)?);
            let entry = Entry {
                at_ms: crate::delegation::now_ms(),
                ops: asked.ops,
            };
            let refused = timeline.apply(&entry);
            if refused.len() == entry.ops.len() {
                return Err(Error::new(
                    Code::Invalid,
                    format!(
                        "no change could be made:
{}",
                        refused.join(
                            "
"
                        )
                    ),
                ));
            }
            store.record_timeline(&code, &entry)?;
            let mut answer = timeline.text();
            if !refused.is_empty() {
                answer.push_str(
                    "
Not made:
",
                );
                answer.push_str(&refused.join(
                    "
",
                ));
            }
            Ok(Value::String(answer))
        })
        .await
}

/// Name the caller's session as its agent asks, in its tags, which the tree
/// shows, and in its record. A name a person gave is kept, and so is every
/// name while the user has switched renaming off. Its agent is not told
/// with its own command, which would have to be typed while it works.
pub(super) async fn rename(
    context: &Context,
    caller: &Session,
    asked: protocol::Rename,
) -> Result<Value> {
    if !context.settings()?.rename_sessions {
        return Err(Error::new(
            Code::Refused,
            format!(
                "the user has switched off sessions naming themselves; keep the name {}",
                caller.name
            ),
        ));
    }
    if caller.name_given {
        return Err(Error::new(
            Code::Refused,
            format!("the user named this session {}; keep the name", caller.name),
        ));
    }
    let label = crate::content::label_of(&asked.name)
        .ok_or_else(|| Error::new(Code::Invalid, "the name holds no letters or digits"))?;
    let terminal = caller
        .terminal
        .ok_or_else(|| Error::new(Code::Refused, "the session has no terminal yet"))?;
    let listed = context.daemon.sessions().await.map_err(Error::storage)?;
    let name = crate::starting::fresh_name(&label, &listed, Some(terminal));
    let held = listed
        .iter()
        .find(|s| s.id == terminal)
        .map(|s| tags::holding(&s.tags, crate::sessions::NAME))
        .unwrap_or_default();
    context
        .daemon
        .tag(terminal, tags::tagged(crate::sessions::NAME, &name), held)
        .await
        .map_err(Error::storage)?;
    let (id, kept) = (caller.id, name.clone());
    context
        .call(move |store| store.rename(id, &kept, false))
        .await?;
    answer(protocol::Renamed { name })
}

/// How long a session closing itself runs on once it is answered, for its
/// agent to take the answer in.
const CLOSING: Duration = Duration::from_secs(1);

/// End the caller's session as the user ends one, a moment after it is
/// answered: ending it at once would cut its agent off before the answer
/// reached it, and the agent would show the call as failed.
async fn close_own(context: &Context, caller: &Session) -> Result<Value> {
    let terminal = caller
        .terminal
        .ok_or_else(|| Error::new(Code::Refused, "the session has no terminal yet"))?;
    let (context, id) = (context.clone(), caller.id);
    crate::environment::spawn(async move {
        tokio::time::sleep(CLOSING).await;
        if let Err(error) = context.daemon.end(terminal).await {
            tracing::warn!(%id, %error, "cannot end a session that closed itself");
            return;
        }
        if let Err(error) = context.call(move |store| store.end(id)).await {
            tracing::warn!(
                %id,
                %error,
                "cannot record a session that closed itself as ended"
            );
        }
    });
    answer(protocol::Closing { closing: true })
}

/// End the caller's own session, or the one `asked` names, which the caller
/// opened.
pub(super) async fn close(
    context: &Context,
    caller: &Session,
    asked: protocol::CloseSession,
) -> Result<Value> {
    let Some(wanted) = asked.target.clone() else {
        return close_own(context, caller).await;
    };
    let record = context
        .call(move |store| {
            let id = super::serve::addressed(store, &wanted)?;
            store.known(id)
        })
        .await?;
    if record.id == caller.id {
        return close_own(context, caller).await;
    }
    if record.parent != Some(caller.id) {
        return Err(Error::new(
            Code::Refused,
            format!(
                "{} ({}) was not opened by this session; only the session that opened a session can end it",
                record.name, record.code
            ),
        ));
    }
    if record.ended {
        return Err(Error::new(
            Code::Refused,
            format!("{} ({}) has ended already", record.name, record.code),
        ));
    }
    let terminal = record
        .terminal
        .ok_or_else(|| Error::new(Code::Refused, "the session has no terminal yet"))?;
    if !asked.force {
        let listed = context.daemon.sessions().await.map_err(Error::storage)?;
        let state = crate::sessions::agents(&listed)
            .find(|shown| crate::sessions::id_of(&shown.id) == Some(terminal))
            .map(|shown| shown.state);
        // A directory that is no repository holds no changes to lose.
        let changes = tsumugi_vcs::git::changes(&record.cwd)
            .await
            .map_or(0, |changes| changes.len());
        let found = losses(state.as_ref(), changes);
        if !found.is_empty() {
            return Err(Error::new(
                Code::Refused,
                format!(
                    "{} ({}) {}; call close_session again with force to end it anyway",
                    record.name,
                    record.code,
                    found.join(" and ")
                ),
            ));
        }
    }
    let (id, opener) = (record.id, caller.id);
    let note = format!(
        "[closed] The session {} ({}) that opened this one closed it.",
        caller.name, caller.code
    );
    // Its end is the opener's own, so the opener is not told of it as a
    // worker's failure.
    context.call(move |store| store.release_worker(id)).await?;
    context.daemon.end(terminal).await.map_err(Error::storage)?;
    context
        .call(move |store| {
            // A note both sessions list, which is how the user sees who ended
            // the session, beside the tree showing it ended.
            let note = store.send(opener, id, &note)?;
            store.delivered(note)?;
            store.end(id)
        })
        .await?;
    answer(protocol::Closed {
        closed: record.id,
        code: record.code,
        name: record.name,
    })
}

/// What ending a session would lose, in words: work in hand, as it stands
/// in the tree, and changes its directory holds that no commit keeps.
fn losses(state: Option<&tsumugi_model::State>, changes: usize) -> Vec<String> {
    use tsumugi_model::State;
    let mut found = Vec::new();
    match state {
        Some(State::Working | State::Background) => found.push("is working".to_owned()),
        Some(State::Waiting(wait)) => found.push(format!("waits on the user ({wait})")),
        _ => {}
    }
    if changes > 0 {
        found.push(format!(
            "has {changes} uncommitted changes in its directory"
        ));
    }
    found
}

/// Tell the session that supervises the caller how its work ended.
pub(super) async fn report_done(
    context: &Context,
    caller: &Session,
    asked: protocol::ReportDone,
) -> Result<Value> {
    let id = caller.id;
    let reported = context
        .call(move |store| {
            store.report_done(id, asked.outcome.word(), &asked.summary, &asked.files)
        })
        .await?;
    answer(protocol::Reported { reported })
}

/// How long a session waits when it says nothing, and the longest it may.
const WAITING: Duration = Duration::from_secs(300);
const WAITING_AT_MOST: Duration = Duration::from_secs(3600);
/// How often what waits for a waiting session is looked for.
const LOOKING: Duration = Duration::from_millis(200);

/// Take the oldest message waiting for the caller, waiting for one to arrive
/// for as long as asked. What it takes is not handed to the agent again
/// between its turns.
pub(super) async fn wait(
    context: &Context,
    caller: &Session,
    asked: protocol::Wait,
) -> Result<Value> {
    let limit = asked
        .timeout_seconds
        .map_or(WAITING, Duration::from_secs)
        .min(WAITING_AT_MOST);
    let until = tokio::time::Instant::now() + limit;
    let id = caller.id;
    loop {
        let taken = context.call(move |store| store.take_next(id)).await?;
        if let Some((message, sender)) = taken {
            return answer(protocol::Waited {
                message: Some(protocol::Delivery {
                    id: message.id,
                    from: message.sender,
                    content: super::serve::envelope(&message, sender.as_ref(), caller.parent),
                }),
                timed_out: false,
            });
        }
        if tokio::time::Instant::now() >= until {
            return answer(protocol::Waited {
                message: None,
                timed_out: true,
            });
        }
        tokio::time::sleep(LOOKING.min(until - tokio::time::Instant::now())).await;
    }
}

/// `prompt`, told besides who supervises the session it starts.
fn supervised_prompt(prompt: &str, supervisor: &Session) -> String {
    format!(
        "{prompt}\n\n---\nYou are a worker supervised by the session {} ({}). \
         When your work is done, or you cannot go on, call the `report_done` tool once with the outcome \
         (succeeded or failed), a summary and the files you modified. \
         The user can read this session and step into it at any time.",
        supervisor.name, supervisor.code
    )
}

/// What the caller's agent is told as a prompt arrives or once its
/// conversation was cleared: of its session's name, and, while nothing is on
/// its timeline, to record the plan of its work there. Nothing when neither
/// applies.
pub(super) async fn prompted(context: &Context, caller: &Session, cleared: bool) -> Result<Value> {
    let naming = naming(context, caller, cleared).await?;
    let code = caller.code.clone();
    let empty = context
        .call(move |store| store.timeline_of(&code))
        .await?
        .is_empty();
    let timeline = empty.then(|| crate::delegation::launch::TIMELINE_AT_PROMPT.to_owned());
    let note = match (naming, timeline) {
        (Some(naming), Some(timeline)) => Some(format!("{naming}\n\n{timeline}")),
        (naming, timeline) => naming.or(timeline),
    };
    answer(protocol::Note { note })
}

/// What the caller's agent is told of its session's name: once its
/// conversation was cleared, that the name is of the work before, even one
/// a person gave, which a clear hands back to the agent; as each prompt
/// arrives, to weigh the name against the work. Nothing while renaming is
/// switched off, or at a prompt while a person's name stands.
async fn naming(context: &Context, caller: &Session, cleared: bool) -> Result<Option<String>> {
    if !context.settings()?.rename_sessions {
        return Ok(None);
    }
    if cleared {
        if caller.name_given {
            let id = caller.id;
            context.call(move |store| store.hand_back_name(id)).await?;
        }
        return Ok(Some(crate::delegation::launch::naming_after_clear(
            &caller.name,
        )));
    }
    Ok((!caller.name_given).then(|| crate::delegation::launch::naming_at_prompt(&caller.name)))
}

/// The ended session `wanted` names, by its code, its id or its
/// conversation, among `all`, whose conversation is to be resumed: the
/// newest record that fits. A conversation a running session holds is not
/// resumed a second time.
fn resumption(all: &[Session], wanted: &str) -> Result<Session> {
    let wanted = wanted.trim();
    let named = |session: &&Session| {
        session.id.to_string() == wanted
            || session.code.as_str() == wanted
            || session.conversation.as_deref() == Some(wanted)
    };
    let old = all.iter().rev().find(named).ok_or_else(|| {
        Error::new(
            Code::NotFound,
            format!("no session has the code, id or conversation {wanted}"),
        )
    })?;
    let conversation = old.conversation.as_deref().filter(|c| !c.is_empty());
    let held = all
        .iter()
        .find(|session| !session.ended && session.conversation.as_deref() == conversation)
        .filter(|_| conversation.is_some());
    if let Some(running) = held.or((!old.ended).then_some(old)) {
        return Err(Error::new(
            Code::Refused,
            format!(
                "{} ({}) runs this conversation; send to it instead of resuming it",
                running.name, running.code
            ),
        ));
    }
    if conversation.is_none() {
        return Err(Error::new(
            Code::Refused,
            format!("{} ({}) kept no conversation to resume", old.name, old.code),
        ));
    }
    Ok(old.clone())
}

/// The start that brings `old`'s conversation back as `name`: where, as
/// which agent and from which profile it ran, as it was started, and under
/// the same code.
fn resumed_request(old: &Session, name: String) -> crate::execution::StartRequest {
    crate::execution::StartRequest {
        agent: old.agent,
        profile: Some(old.profile.clone()),
        model: old.model.clone(),
        name,
        name_given: old.name_given,
        cwd: old.cwd.clone(),
        permissions: old.permissions.clone(),
        resume: old.conversation.clone(),
        home: old.home.clone(),
        ..Default::default()
    }
}

/// What a session working an issue is first told: the issue, then what its
/// opener adds.
fn issue_prompt(number: u64, issue: &tsumugi_model::LocalIssue, added: &str) -> String {
    let issue = format!("Local issue #{number}: {}\n\n{}", issue.title, issue.body);
    match added.trim() {
        "" => issue,
        added => format!("{issue}\n\n---\n{added}"),
    }
}

pub(super) async fn start(
    context: &Context,
    caller: &Session,
    start: protocol::Start,
) -> Result<Value> {
    let prompt = start.prompt.trim().to_owned();
    if prompt.is_empty() && start.resume.is_none() && start.issue.is_none() {
        return Err(Error::new(Code::Invalid, "the prompt is empty"));
    }
    let settings = context.settings()?;
    if !settings.delegation.enabled {
        return Err(Error::new(
            Code::Refused,
            "the user has switched off sessions opening sessions; carry on yourself, or ask the user to open the session",
        ));
    }
    let resumed = match &start.resume {
        Some(wanted) => {
            if start.dir.is_some()
                || start.worktree
                || start.base.is_some()
                || start.agent.is_some()
                || start.profile.is_some()
            {
                return Err(Error::new(
                    Code::Invalid,
                    "a resume runs where, as which agent and from which profile the session ran; leave dir, worktree, base, agent and profile out",
                ));
            }
            let wanted = wanted.clone();
            Some(
                context
                    .call(move |store| resumption(&store.sessions()?, &wanted))
                    .await?,
            )
        }
        None => None,
    };
    let worked = match &start.issue {
        Some(wanted) => {
            let wanted = wanted.clone();
            Some(
                context
                    .call(move |store| store.local_issue_named(&wanted))
                    .await?,
            )
        }
        None => None,
    };
    // The caller's own profile is not passed on: a session starts from the
    // one it names, else from the default; a resume from the one it ran from.
    let chosen = match &resumed {
        Some(old) => crate::programs::starting(&settings, Some(old.agent), Some(&old.profile)),
        None => crate::programs::starting(&settings, start.agent, start.profile.as_ref()),
    }
    .map_err(|error| Error::new(Code::Invalid, error))?;
    let (agent, profile) = (chosen.agent(), chosen.profile);
    if resumed.is_none() {
        context
            .models
            .lock()
            .expect("the models' lock")
            .check(&profile, start.model.as_deref())
            .map_err(|error| Error::new(Code::Invalid, error))?;
    }
    let dir = match (&resumed, &start.dir, &worked) {
        (Some(old), _, _) => old.cwd.clone(),
        (None, Some(dir), _) => dir.clone(),
        (None, None, Some((_, issue))) => issue.dir.clone(),
        (None, None, None) => caller.cwd.clone(),
    };
    let dir = tsumugi_sys::fs::canonicalize(&dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| Error::new(Code::Invalid, format!("{} is no directory", dir.display())))?;
    if start.base.is_some() && !start.worktree {
        return Err(Error::new(
            Code::Invalid,
            "a base revision needs a new worktree (worktree: true)",
        ));
    }
    // Taken up before anything runs, so the issue holds its directory against
    // the queue from now on; a start that then fails marks it failed, as a
    // queued one does.
    let taken = match &worked {
        Some((_, issue)) => {
            let id = issue.id.clone();
            Some(
                context
                    .call(move |store| store.begin_local_issue(&id))
                    .await?,
            )
        }
        None => None,
    };
    let launched = launch(
        context,
        caller,
        &settings,
        &start,
        resumed,
        taken.as_ref(),
        (agent, profile, dir),
    )
    .await;
    if let (Err(error), Some((_, issue))) = (&launched, &taken) {
        let (id, message) = (issue.id.clone(), error.message.clone());
        if let Err(error) = context
            .call(move |store| store.fail_local_issue(&id, &message))
            .await
        {
            tracing::warn!(%error, "cannot mark an issue whose session did not start as failed");
        }
    }
    let started = launched?;
    if start.supervised {
        // Recorded as soon as it has started: its agent needs longer than
        // that to make a first report.
        let worker = started.recorded.id;
        context.call(move |store| store.supervise(worker)).await?;
    }
    if start.resume.is_some() && !prompt.is_empty() {
        let (opener, resumed) = (caller.id, started.recorded.id);
        context
            .call(move |store| store.send(opener, resumed, &prompt))
            .await?;
    }
    answer(protocol::Started {
        id: started.recorded.id,
        code: started.recorded.code,
        name: started.name,
        dir: started.dir,
    })
}

/// Start the session `start` asks for, once what it asks was found fit.
async fn launch(
    context: &Context,
    caller: &Session,
    settings: &crate::settings::Settings,
    start: &protocol::Start,
    resumed: Option<Session>,
    taken: Option<&(u64, tsumugi_model::LocalIssue)>,
    (agent, profile, dir): (
        tsumugi_model::Agent,
        tsumugi_model::profile::ProfileId,
        std::path::PathBuf,
    ),
) -> Result<crate::starting::Started> {
    let listed = context.daemon.sessions().await.map_err(Error::storage)?;
    let mut request = match &resumed {
        Some(old) => {
            let name = crate::starting::name_for(Some(&old.name), &old.name, &listed);
            resumed_request(old, name)
        }
        None => {
            let prompt = start.prompt.trim();
            let prompt = match taken {
                Some((number, issue)) => issue_prompt(*number, issue, prompt),
                None => prompt.to_owned(),
            };
            crate::execution::StartRequest {
                agent,
                profile: Some(profile),
                model: start.model.clone(),
                name: crate::starting::name_for(start.name.as_deref(), &prompt, &listed),
                prompt: if start.supervised {
                    supervised_prompt(&prompt, caller)
                } else {
                    prompt
                },
                permissions: start.permissions.clone().unwrap_or_default(),
                ..Default::default()
            }
        }
    };
    request.env = crate::daemon::agent_env();
    request.started_ms = crate::delegation::now_ms();
    request.show = !start.background;
    request.local_issue = taken.map(|(_, issue)| issue.id.clone());
    let place = crate::starting::Place {
        dir,
        worktree: start.worktree,
        base: start.base.clone(),
    };
    let host = crate::starting::Host {
        daemon: &context.daemon,
        root: &context.root,
        program: &context.program,
    };
    crate::starting::start(host, settings, Some(caller.id), place, request)
        .await
        .map_err(|failure| {
            let code = match failure.stage {
                Stage::Refused => Code::Invalid,
                Stage::NotStarted | Stage::MayHaveRun | Stage::SetupFailed => Code::Storage,
            };
            Error::new(code, failure.message)
        })
}

/// The ports the sessions of a worktree listen on: the caller's own unless
/// another is named, or every session's.
pub(super) async fn ports(caller: &Session, asked: protocol::Ports) -> Result<Value> {
    let dir = (!asked.all).then(|| match asked.worktree {
        Some(dir) if dir.is_absolute() => dir,
        Some(dir) => caller.cwd.join(dir),
        None => caller.cwd.clone(),
    });
    let listed = crate::ports::listed(dir)
        .await
        .map_err(|text| Error::new(Code::Storage, text))?;
    answer(listed)
}

/// The sessions that have not ended, narrowed as `filter` asks.
pub(super) async fn listed(all: Vec<Session>, filter: &protocol::Sessions) -> Result<Value> {
    let wanted_repo = match &filter.repo {
        Some(dir) => Some(tsumugi_vcs::common_dir(dir).await.ok_or_else(|| {
            Error::new(
                Code::Invalid,
                format!("{} is in no repository", dir.display()),
            )
        })?),
        None => None,
    };
    let wanted_dir = filter
        .dir
        .as_ref()
        .map(|dir| tsumugi_sys::fs::canonicalize(dir).unwrap_or(dir.clone()));
    let mut out = Vec::new();
    for session in all.iter().filter(|session| !session.ended) {
        let repository = tsumugi_vcs::common_dir(&session.cwd).await;
        if wanted_repo.is_some() && repository != wanted_repo {
            continue;
        }
        if let Some(dir) = &wanted_dir
            && !session.cwd.starts_with(dir)
        {
            continue;
        }
        let opened_by = session
            .parent
            .and_then(|parent| all.iter().find(|s| s.id == parent))
            .map(protocol::Opener::of);
        out.push(protocol::Listed {
            id: session.id,
            code: session.code.clone(),
            name: session.name.clone(),
            agent: session.agent,
            dir: session.cwd.clone(),
            repository,
            readiness: session.readiness,
            opened_by,
        });
    }
    answer(out)
}

#[cfg(test)]
mod tests;
