//! Starting a local issue as several attempts, each a session in a new
//! worktree of the issue's directory made from the same commit, for the
//! window, the command line and the sessions that open sessions.

use tsumugi_model::{Attempt, LocalIssue, SessionId, TryAs};

use crate::delegation::app::with_store;
use crate::settings::Settings;
use crate::starting::Host;

/// The session that opens the attempts, when one does.
pub(crate) struct Opener {
    pub(crate) id: SessionId,
    /// What each attempt's prompt is followed by, and that it is to be
    /// supervised, when the opener supervises them.
    pub(crate) supervised: Option<String>,
    /// Whether the opener asks that the attempts be shown to the user.
    pub(crate) show: bool,
}

/// Start the issue `selector` names as one attempt for each of `tries`,
/// opened by `opener`. Every profile is checked, and the directory found to
/// be in Git, before anything is taken up; past that an attempt that cannot
/// start fails alone. The windows `watched` reaches are shown the issues
/// each time they change; none are without it.
pub(crate) async fn start(
    host: Host<'_>,
    settings: &Settings,
    selector: String,
    tries: Vec<TryAs>,
    opener: Option<Opener>,
    watched: Option<&crate::watch::Watched>,
) -> Result<(u64, LocalIssue), String> {
    let published = async || {
        if let Some(watched) = watched
            && let Err(error) = crate::local_issues::publish(watched).await
        {
            tracing::warn!("cannot publish the issues: {error}");
        }
    };
    for tried in &tries {
        context_of(settings, tried)?;
    }
    let root = host.root.to_path_buf();
    let (_, issue) = with_store(&root, move |store| store.local_issue_named(&selector))
        .await
        .map_err(|e| e.to_string())?;
    let base = tsumugi_vcs::worktree::head(&issue.dir).await.map_err(|_| {
        format!(
            "{} is not in a Git repository: each attempt needs a worktree of its own",
            issue.dir.display()
        )
    })?;
    let (number, begun) = with_store(&root, {
        let id = issue.id.clone();
        move |store| store.begin_attempts(&id, &tries, &base)
    })
    .await
    .map_err(|e| e.to_string())?;
    published().await;
    // Past the take-up nothing may return early, or an attempt would be left
    // starting with no session ever to follow; a name that then clashes
    // fails only its own attempt.
    let listed = host.daemon.sessions().await.unwrap_or_default();
    let label = crate::content::label_of(&begun.title)
        .unwrap_or_else(|| crate::content::label_from(&begun.body));
    let names = names(&label, begun.attempts.len(), |name| {
        crate::starting::fresh_name(name, &listed, None)
    });
    for (attempt, name) in begun.attempts.iter().zip(names) {
        let started = start_one(host, settings, &begun, attempt, name, &opener).await;
        let (id, number) = (begun.id.clone(), attempt.number);
        let supervised = opener.as_ref().is_some_and(|o| o.supervised.is_some());
        let recorded = with_store(&root, move |store| match started {
            Ok(worker) if supervised => store.supervise(worker),
            Ok(_) => Ok(()),
            Err(error) => store.fail_attempt(&id, number, &error),
        })
        .await;
        if let Err(lost) = recorded {
            tracing::warn!(issue = %begun.id, number, "cannot record how the attempt started: {lost}");
        }
        published().await;
    }
    let id = begun.id.clone();
    let (_, now) = with_store(&root, move |store| store.local_issue_named(&id))
        .await
        .map_err(|e| e.to_string())?;
    Ok((number, now))
}

/// The context an attempt starts in, from the profile it names.
pub(crate) fn context_of(
    settings: &Settings,
    tried: &TryAs,
) -> Result<tsumugi_agents::AgentContext, String> {
    let profile = tried
        .profile
        .as_deref()
        .map(|name| {
            name.parse::<tsumugi_model::profile::ProfileId>()
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    crate::programs::starting(settings, None, profile.as_ref())
}

/// A name for each of `count` attempts from `label`, set apart by number
/// and made fresh by `fresh` against the sessions there are, so no two
/// attempts, nor their worktrees, share one.
fn names(label: &str, count: usize, fresh: impl Fn(&str) -> String) -> Vec<String> {
    (1..=count)
        .map(|number| fresh(&format!("{label} {number}")))
        .collect()
}

/// Start `attempt` at `issue` as `name`, answering its session's record.
async fn start_one(
    host: Host<'_>,
    settings: &Settings,
    issue: &LocalIssue,
    attempt: &Attempt,
    name: String,
    opener: &Option<Opener>,
) -> Result<SessionId, String> {
    let context = context_of(settings, &attempt.tried_as)?;
    let mut prompt = format!("Local issue: {}\n\n{}", issue.title, issue.body);
    if let Some(supervised) = opener.as_ref().and_then(|o| o.supervised.as_deref()) {
        prompt.push_str(supervised);
    }
    let request = crate::execution::StartRequest {
        local_issue: Some(issue.id.clone()),
        attempt: Some(attempt.number),
        agent: context.agent(),
        profile: Some(context.profile),
        model: attempt.tried_as.model.clone(),
        name,
        name_given: true,
        prompt,
        env: crate::daemon::agent_env(),
        started_ms: tsumugi_model::time::now_ms(),
        show: opener.as_ref().is_some_and(|o| o.show),
        ..Default::default()
    };
    let place = crate::starting::Place {
        dir: issue.dir.clone(),
        worktree: true,
        base: Some(attempt.base.clone()),
    };
    crate::starting::start(
        host,
        settings,
        opener.as_ref().map(|o| o.id),
        place,
        request,
    )
    .await
    .map(|started| started.recorded.id)
    .map_err(|failure| failure.message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_attempt_is_named_apart_from_the_others() {
        let names = super::names("fix login", 3, |name| name.to_owned());
        assert_eq!(names, ["fix login 1", "fix login 2", "fix login 3"]);
    }
}
