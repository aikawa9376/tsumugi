//! The work host owns issue processing independently of its connected windows.
use tsumugi_model::{NewLocalIssue, Session};
use tsumugi_protocol::{Answer, TreeChange};

use crate::delegation::app::{root, with_store};
use crate::watch::Watched;

pub(crate) async fn queue(watched: &Watched, mut issue: NewLocalIssue) -> Result<Answer, String> {
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

pub(crate) async fn change(watched: &Watched, id: String, retry: bool) -> Result<Answer, String> {
    with_store(&root(), move |store| {
        if retry {
            store.retry_local_issue(&id)
        } else {
            store.cancel_local_issue(&id)
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    publish(watched).await?;
    watched.relist();
    Ok(Answer::Done)
}

async fn publish(watched: &Watched) -> Result<(), String> {
    let issues = with_store(&root(), |store| store.local_issues())
        .await
        .map_err(|e| e.to_string())?;
    watched.record([TreeChange::LocalIssues { issues }]);
    Ok(())
}

/// A successful listing reconciles old sessions before anything new is claimed.
pub(crate) async fn advance(watched: &Watched, sessions: &[Session]) -> Result<(), String> {
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
    publish(watched).await
}

async fn start(issue: &tsumugi_model::LocalIssue) -> Result<(), String> {
    let settings = crate::settings::current();
    let context = crate::programs::starting(&settings, None, None)?;
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
