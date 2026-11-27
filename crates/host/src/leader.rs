//! Handing issues to a leader: the first prompt a leader session is given, and
//! the start of that session.

use tsumugi_model::{LocalIssue, LocalIssueState};
use tsumugi_protocol::Answer;

use crate::delegation::app::{root, with_store};
use crate::settings::{Settings, Tier, WorkerTier};
use crate::watch::Watched;

/// What a leader is first told, for `issues`, the issues it is handed:
/// its discretion, the language it speaks to the user in, which tiers it can
/// start workers by, how many it keeps at work, and where its guide is.
/// Refused, with the reason, when it could not do what it is handed.
pub(crate) fn prompt(
    settings: &Settings,
    language: &str,
    issues: &[(u64, LocalIssue)],
    max_workers: Option<u32>,
) -> Result<String, String> {
    if issues.is_empty() {
        return Err("choose the issues to hand to the leader".into());
    }
    if let Some((number, issue)) = issues
        .iter()
        .find(|(_, issue)| issue.state != LocalIssueState::Open)
    {
        return Err(format!(
            "issue #{number} is {}; only open issues are handed to a leader",
            issue.state.word()
        ));
    }
    if !settings.delegation.enabled {
        return Err(
            "sessions opening sessions is switched off in the settings, and a leader works through it".into(),
        );
    }
    let tiers = Tier::ALL.map(|tier| (tier, settings.worker_tiers.resolve(tier)));
    if tiers.iter().all(|(_, found)| found.is_err()) {
        return Err(
            "no worker tier is defined: define `light`, `standard` or `heavy` under [worker_tiers] in settings.toml before handing issues to a leader".into(),
        );
    }
    let workers = max_workers.unwrap_or(settings.leader.max_workers).max(1);
    let mut told = String::from(
        "You are the leader of the local issues listed below: the user hands them to you and you carry them through yourself. \
         Load your guide with `tsumugi skills get issue-supervisor` (the `issue-supervisor` skill) and follow it.\n\n",
    );
    told.push_str("Issues (work each in its own worktree; start its worker with `issue` set to its number):\n");
    for (number, issue) in issues {
        told.push_str(&format!(
            "- #{number} {} ({})\n",
            issue.title,
            issue.dir.display()
        ));
    }
    told.push_str(&format!(
        "\nKeep at most {workers} worker{} at work at once.\n\nWorker tiers (start a worker with `tier`):\n",
        if workers == 1 { "" } else { "s" }
    ));
    for (tier, found) in tiers {
        match found {
            Ok(tier_launch) => told.push_str(&format!("- {}: {}\n", tier.name(), describe(tier_launch))),
            Err(_) => told.push_str(&format!(
                "- {}: not defined; do not start a worker by it, and tell the user if the work needs it\n",
                tier.name()
            )),
        }
    }
    told.push('\n');
    told.push_str(&settings.leader.discretion.instructions());
    told.push_str("\n\n");
    told.push_str(&tsumugi_core::settings::language_instructions(language));
    told.push('\n');
    Ok(told)
}

/// What a tier starts, as the leader reads it.
fn describe(tier: &WorkerTier) -> String {
    let profile = tier
        .profile
        .as_ref()
        .map_or("the default profile".to_string(), |p| {
            format!("profile `{p}`")
        });
    match tier.model.as_str() {
        "" => format!("{profile}, its own default model"),
        model => format!("{profile}, model `{model}`"),
    }
}

/// Start a leader session for the issues `ids` name, in the directory of the
/// first, and say so to every window.
pub(crate) async fn start(
    watched: &Watched,
    ids: Vec<String>,
    max_workers: Option<u32>,
) -> Result<Answer, String> {
    let settings = crate::settings::current();
    let issues = with_store(&root(), move |store| {
        ids.iter()
            .map(|id| store.local_issue_named(id))
            .collect::<crate::delegation::Result<Vec<_>>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    let language = crate::settings::language(&settings.language, |name| std::env::var(name).ok());
    let told = prompt(&settings, &language, &issues, max_workers)?;
    let context = crate::programs::starting(&settings, None, settings.leader.profile.as_ref())?;
    let daemon = crate::daemon::daemon();
    let listed = daemon.sessions().await.map_err(|e| e.to_string())?;
    let dir = issues[0].1.dir.clone();
    let request = crate::execution::StartRequest {
        agent: context.agent(),
        profile: Some(context.profile),
        model: Some(settings.leader.model.clone()).filter(|model| !model.is_empty()),
        name: crate::starting::name_for(Some("Leader"), &told, &listed),
        name_given: true,
        prompt: told,
        env: crate::daemon::agent_env(),
        started_ms: tsumugi_model::time::now_ms(),
        show: true,
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
        dir,
        worktree: false,
        base: None,
    };
    crate::starting::start(host, &settings, None, place, request)
        .await
        .map_err(|failure| failure.message)?;
    watched.relist();
    Ok(Answer::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(state: LocalIssueState) -> LocalIssue {
        LocalIssue {
            id: "a".into(),
            dir: "/r/api".into(),
            title: "Fix the flaky test".into(),
            body: "body".into(),
            state,
            record: None,
            session: None,
            error: None,
            schedule: None,
            attempts: Vec::new(),
        }
    }

    fn open() -> Vec<(u64, LocalIssue)> {
        vec![(7, issue(LocalIssueState::Open))]
    }

    fn defined() -> Settings {
        Settings::parse(
            "[worker_tiers.light]\nmodel = 'haiku'\n[worker_tiers.heavy]\nprofile = 'codex'\nmodel = 'gpt-5'\n",
        )
        .unwrap()
    }

    #[test]
    fn a_leader_is_handed_the_issues_the_tiers_the_discretion_and_the_language() {
        let told = prompt(&defined(), "ja", &open(), Some(3)).unwrap();
        assert!(told.contains("#7 Fix the flaky test (/r/api)"), "{told}");
        assert!(
            told.contains("`tsumugi skills get issue-supervisor`"),
            "{told}"
        );
        assert!(told.contains("at most 3 workers"), "{told}");
        assert!(
            told.contains("light: the default profile, model `haiku`"),
            "{told}"
        );
        assert!(
            told.contains("heavy: profile `codex`, model `gpt-5`"),
            "{told}"
        );
        assert!(told.contains("standard: not defined"), "{told}");
        assert!(told.contains("Ask the user before"), "{told}");
        assert!(told.contains("push to a remote"), "{told}");
        assert!(told.contains("in Japanese"), "{told}");
    }

    #[test]
    fn without_a_count_of_its_own_a_leader_keeps_as_many_workers_as_the_settings_allow() {
        let settings =
            Settings::parse("[leader]\nmax_workers = 1\n[worker_tiers.light]\nmodel = 'haiku'\n")
                .unwrap();
        let told = prompt(&settings, "en", &open(), None).unwrap();
        assert!(told.contains("at most 1 worker at work"), "{told}");
    }

    #[test]
    fn issues_that_cannot_be_handed_over_refuse_the_start_with_the_reason() {
        let settings = defined();
        assert!(
            prompt(&settings, "en", &[], None)
                .unwrap_err()
                .contains("choose")
        );
        let started = vec![(9, issue(LocalIssueState::Running))];
        let refused = prompt(&settings, "en", &started, None).unwrap_err();
        assert!(refused.contains("#9 is running"), "{refused}");
    }

    #[test]
    fn a_leader_with_no_tier_defined_is_refused_rather_than_left_to_start_nothing() {
        let refused = prompt(&Settings::default(), "en", &open(), None).unwrap_err();
        assert!(refused.contains("[worker_tiers]"), "{refused}");
    }

    #[test]
    fn a_leader_needs_sessions_to_be_allowed_to_open_sessions() {
        let settings = Settings::parse(
            "[delegation]\nenabled = false\n[worker_tiers.light]\nmodel = 'haiku'\n",
        )
        .unwrap();
        let refused = prompt(&settings, "en", &open(), None).unwrap_err();
        assert!(refused.contains("switched off"), "{refused}");
    }
}
