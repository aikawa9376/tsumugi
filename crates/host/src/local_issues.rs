//! The work host owns issue processing independently of its connected windows.
use tsumugi_model::{LocalIssue, LocalIssueAction, NewLocalIssue, Session, TryAs};
use tsumugi_protocol::{Answer, TreeChange};

use crate::delegation::app::{root, with_store};
use crate::watch::Watched;

pub(crate) async fn create(watched: &Watched, mut issue: NewLocalIssue) -> Result<Answer, String> {
    issue.dir = tsumugi_sys::fs::canonicalize(&issue.dir)
        .ok()
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| "Choose an existing working directory on the work machine".to_owned())?;
    with_store(&root(), move |store| store.add_local_issue(issue))
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    watched.relist();
    Ok(Answer::Done)
}

pub(crate) async fn queue(watched: &Watched, issue: NewLocalIssue) -> Result<Answer, String> {
    let id = issue.id.clone();
    create(watched, issue).await?;
    start_saved(watched, id).await
}

pub(crate) async fn start_saved(watched: &Watched, id: String) -> Result<Answer, String> {
    edit(watched, move |store| store.start_local_issue(&id)).await
}

pub(crate) async fn change(watched: &Watched, id: String, retry: bool) -> Result<Answer, String> {
    edit(watched, move |store| {
        if retry {
            store.retry_local_issue(&id)
        } else {
            store.cancel_local_issue(&id)
        }
    })
    .await
}

pub(crate) async fn close(watched: &Watched, id: String) -> Result<Answer, String> {
    edit(watched, move |store| store.close_local_issue(&id)).await
}

pub(crate) async fn reopen(watched: &Watched, id: String) -> Result<Answer, String> {
    edit(watched, move |store| store.reopen_local_issue(&id)).await
}

pub(crate) async fn delete(watched: &Watched, id: String) -> Result<Answer, String> {
    edit(watched, move |store| store.delete_local_issue(&id)).await
}

/// Do `action` to the issue `selector` names, which the windows then show.
pub(crate) async fn act(
    watched: &Watched,
    selector: &str,
    action: LocalIssueAction,
) -> crate::delegation::Result<(u64, LocalIssue)> {
    let selector = selector.to_owned();
    let done = with_store(&root(), move |store| {
        store.act_on_local_issue(&selector, action)
    })
    .await?;
    publish(watched)
        .await
        .map_err(crate::delegation::Error::storage)?;
    watched.relist();
    Ok(done)
}

/// What an issue says of itself to a command or an agent.
pub(crate) fn view(number: u64, issue: &LocalIssue) -> serde_json::Value {
    serde_json::json!({
        "number": number,
        "id": issue.id,
        "dir": issue.dir,
        "title": issue.title,
        "body": issue.body,
        "state": issue.state,
        "session": issue.session,
        "error": issue.error,
        "schedule": issue.schedule,
        "attempts": issue.attempts,
    })
}

/// An attempt's profile and model as a command names them.
pub(crate) fn tried_as_word(tried: &TryAs) -> String {
    let profile = tried.profile.as_deref().unwrap_or("default profile");
    match &tried.model {
        Some(model) => format!("{profile}:{model}"),
        None => profile.to_owned(),
    }
}

async fn edit(
    watched: &Watched,
    change: impl FnOnce(&mut crate::delegation::store::Store) -> crate::delegation::Result<()>
    + Send
    + 'static,
) -> Result<Answer, String> {
    with_store(&root(), change)
        .await
        .map_err(|e| e.to_string())?;
    publish(watched).await?;
    watched.relist();
    Ok(Answer::Done)
}

pub(crate) async fn publish(watched: &Watched) -> Result<(), String> {
    let issues = with_store(&root(), |store| store.local_issues())
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::LocalIssues { issues }]);
    Ok(())
}

/// A successful listing reconciles old sessions before anything new is claimed.
pub(crate) async fn advance(
    watched: &Watched,
    sessions: &[Session],
) -> Result<Vec<std::path::PathBuf>, String> {
    let sessions = sessions.to_vec();
    let claimed = with_store(&root(), move |store| {
        store.follow_local_issues(&sessions)?;
        store.claim_local_issues()
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    for issue in claimed {
        if let Err(error) = start(&issue).await {
            with_store(&root(), move |store| {
                store.fail_local_issue(&issue.id, &error)
            })
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    publish(watched).await?;
    with_store(&root(), |store| {
        Ok(store
            .local_issues()?
            .into_iter()
            .map(|issue| issue.dir)
            .collect())
    })
    .await
    .map_err(|e| e.to_string())
}

async fn start(issue: &tsumugi_model::LocalIssue) -> Result<(), String> {
    let settings = crate::settings::current();
    let profile = with_store(&root(), {
        let id = issue.id.clone();
        move |store| store.local_issue_profile(&id)
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|name| {
        name.parse::<tsumugi_model::profile::ProfileId>()
            .map_err(|e| e.to_string())
    })
    .transpose()?;
    let context = crate::programs::starting(&settings, None, profile.as_ref())?;
    let daemon = crate::daemon::daemon();
    let listed = daemon.sessions().await.map_err(|e| e.to_string())?;
    let name = crate::starting::name_for(Some(&issue.title), &issue.body, &listed);
    let request = crate::execution::StartRequest {
        local_issue: Some(issue.id.clone()),
        agent: context.agent(),
        profile: Some(context.profile),
        name,
        name_given: true,
        prompt: format!("Local issue: {}\n\n{}", issue.title, issue.body),
        env: crate::daemon::agent_env(),
        started_ms: tsumugi_model::time::now_ms(),
        ..Default::default()
    };
    let program = crate::work_host::tsumugi_program();
    let root = root();
    let host = crate::starting::Host {
        daemon: &daemon,
        root: &root,
        program: &program,
    };
    let place = crate::starting::Place {
        dir: issue.dir.clone(),
        worktree: false,
        base: None,
    };
    crate::starting::start(host, &settings, None, place, request)
        .await
        .map(|_| ())
        .map_err(|failure| failure.message)
}

/// Start the issue `selector` names as one attempt for each of `tries`, as
/// the user asks from a window or the command line.
pub(crate) async fn try_as(
    watched: &Watched,
    selector: String,
    tries: Vec<TryAs>,
) -> Result<(u64, LocalIssue), String> {
    let settings = crate::settings::current();
    let (root, daemon, program) = (
        root(),
        crate::daemon::daemon(),
        crate::work_host::tsumugi_program(),
    );
    let host = crate::starting::Host {
        daemon: &daemon,
        root: &root,
        program: &program,
    };
    let started =
        crate::attempts::start(host, &settings, selector, tries, None, Some(watched)).await;
    watched.relist();
    started
}
