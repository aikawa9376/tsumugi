//! Telling the user, outside the window, that a limit of an account is
//! nearly used up: once each time one passes the warning threshold.

use std::collections::{BTreeMap, BTreeSet};

use tsumugi_model::EnvironmentId;
use tsumugi_model::usage::Limit;
use tsumugi_protocol::Account;

use crate::config::Config;
use crate::details::gauges;
use crate::effect::Effect;
use crate::state::ViewState;

/// A limit of an account, as the user would name it.
pub(crate) type Warned = (Account, String);

/// The limits of `usage` at or over the threshold, for the accounts `polled`
/// says are asked, each with how much of it is used. The window has no
/// clock, so a limit is as the host last told, its reset not yet passed.
fn over(
    usage: &BTreeMap<Account, Vec<Limit>>,
    polled: impl Fn(&Account) -> bool,
) -> BTreeMap<Warned, u32> {
    usage
        .iter()
        .filter(|(account, _)| polled(account))
        .flat_map(|(account, limits)| {
            gauges(limits, 0)
                .into_iter()
                .filter(|g| g.warning())
                .map(|g| ((account.clone(), g.label), g.used))
        })
        .collect()
}

/// The limits in `now` that `before` did not hold over the threshold: a
/// limit that stays over is told once, and again after it dropped below.
fn crossed(before: &BTreeSet<Warned>, now: &BTreeMap<Warned, u32>) -> Vec<(Warned, u32)> {
    now.iter()
        .filter(|(key, _)| !before.contains(*key))
        .map(|(key, used)| (key.clone(), *used))
        .collect()
}

/// `environment`'s host said something: tell the user of each limit of its
/// accounts that passed the threshold since it was last heard from, as the
/// settings say. What stood over it when this window first heard from the
/// host is not news.
pub(crate) fn heard(state: &mut ViewState, environment: &EnvironmentId) -> Vec<Effect> {
    let Some(work) = state.hosts.work(environment) else {
        return Vec::new();
    };
    let profiles = Config::of_work(&work.config).settings.profiles;
    let polled = |account: &Account| {
        let mut of = profiles
            .iter()
            .filter(|p| p.agent() == account.agent && p.home == account.home)
            .peekable();
        of.peek().is_none() || of.any(|p| p.poll_usage)
    };
    let now = over(&work.details.usage, polled);
    let name = |account: &Account| {
        profiles
            .iter()
            .find(|p| p.agent() == account.agent && p.home == account.home)
            .map_or_else(
                || account.agent.label().to_string(),
                |p| p.label().to_string(),
            )
    };
    let told: Vec<(String, String)> = match state.notices.usage_warned.get(environment) {
        // The first word of a host passes over what already stood.
        None => Vec::new(),
        Some(before) => crossed(before, &now)
            .into_iter()
            .map(|((account, label), used)| (name(&account), format!("{label} limit {used}% used")))
            .collect(),
    };
    state
        .notices
        .usage_warned
        .insert(environment.clone(), now.into_keys().collect());
    if told.is_empty() || !state.config.settings.notify_usage {
        return Vec::new();
    }
    vec![Effect::NotifyNotices(told)]
}

#[cfg(test)]
mod tests {
    use tsumugi_model::Agent;
    use tsumugi_model::usage::{Severity, Window};
    use tsumugi_protocol::DetailsChange;
    use tsumugi_protocol::testing::{patch, snapshot};

    use super::*;
    use crate::config::testing::{configured, settings_file};
    use crate::scenario::Scenario;
    use crate::testing::session;

    fn claude() -> Account {
        Account {
            agent: Agent::Claude,
            home: None,
        }
    }

    fn limits(percent: f64) -> DetailsChange {
        DetailsChange::Usage {
            account: claude(),
            limits: vec![Limit {
                window: Window::Week,
                percent,
                resets_ms: None,
                severity: Severity::Normal,
            }],
        }
    }

    fn listing() -> tsumugi_protocol::testing::SnapshotBuilder {
        snapshot().listed(tsumugi_protocol::testing::listed().session(session("a").cwd("/r")))
    }

    fn told(run: &Scenario) -> Vec<Vec<(String, String)>> {
        run.effects()
            .iter()
            .filter_map(|e| match e {
                Effect::NotifyNotices(of) => Some(of.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_limit_passing_the_threshold_is_told_once_and_again_after_it_dropped_below() {
        let run = Scenario::new()
            .event(listing().seq(0))
            .event(patch(1).change(limits(50.0)));
        assert!(told(&run).is_empty());
        let run = run.event(patch(2).change(limits(81.0)));
        assert_eq!(
            told(&run),
            [vec![(
                "Claude Code".to_string(),
                "Week limit 81% used".to_string()
            )]]
        );
        let run = run.event(patch(3).change(limits(90.0)));
        assert_eq!(told(&run).len(), 1, "still over, so not told again");
        let run = run
            .event(patch(4).change(limits(10.0)))
            .event(patch(5).change(limits(85.0)));
        assert_eq!(told(&run).len(), 2);
    }

    #[test]
    fn what_stood_over_the_threshold_when_the_host_was_first_heard_is_not_told_of() {
        let run = Scenario::new().event(listing().seq(0).with(|w| {
            w.details.usage = BTreeMap::from([(
                claude(),
                vec![Limit {
                    window: Window::Week,
                    percent: 95.0,
                    resets_ms: None,
                    severity: Severity::Critical,
                }],
            )])
        }));
        assert!(told(&run).is_empty());
        let run = run.event(patch(1).change(limits(96.0)));
        assert!(told(&run).is_empty(), "it never dropped below");
    }

    #[test]
    fn nothing_is_told_when_turned_off_or_for_a_profile_that_does_not_poll() {
        let off = Scenario::new()
            .event(configured("notify_usage = false\n"))
            .event(listing().seq(0))
            .event(patch(1).change(limits(95.0)));
        assert!(told(&off).is_empty());

        let work = "[[profiles]]\nid = \"claude\"\nagent = \"claude\"\npoll_usage = false\n";
        let silent = Scenario::new()
            .event(listing().seq(0).with(move |w| {
                w.config.settings = Some(settings_file(work));
            }))
            .event(patch(1).change(limits(95.0)));
        assert!(told(&silent).is_empty());
    }

    #[test]
    fn a_profile_is_named_by_its_label() {
        let work = "[[profiles]]\nid = \"claude\"\nname = \"Claude Work\"\nagent = \"claude\"\n";
        let run = Scenario::new()
            .event(listing().seq(0).with(move |w| {
                w.config.settings = Some(settings_file(work));
            }))
            .event(patch(1).change(limits(95.0)));
        assert_eq!(told(&run)[0][0].0, "Claude Work");
    }
}
