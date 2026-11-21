//! What an environment's details say of the agents its profiles start.

use std::collections::{BTreeMap, BTreeSet};

use tsumugi_model::Agent;
use tsumugi_model::profile::{LoginStanding, ProfileId};
use tsumugi_model::usage::Limit;
use tsumugi_protocol::{Account, AgentsWork};

use super::{Gauge, gauges};
use crate::settings::Settings;

/// How far a profile's usage is known, so that a number is never mistaken
/// for the lack of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageStatus {
    /// The profile is set not to poll, so nothing is asked of its account.
    NotPolled,
    /// Its login is logged out.
    NotSignedIn,
    /// Its account was asked and gave no answer.
    Unavailable,
    /// Not answered yet.
    Waiting,
    /// Its limits, as last told.
    Limits,
}

impl UsageStatus {
    /// What a row says in place of numbers; empty where there are numbers.
    pub fn word(self) -> &'static str {
        match self {
            Self::NotPolled => "Not polling",
            Self::NotSignedIn => "Not signed in",
            Self::Unavailable => "Usage unavailable",
            Self::Waiting => "Reading usage…",
            Self::Limits => "",
        }
    }
}

/// One profile of an environment, as its details list it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRow {
    pub id: ProfileId,
    pub name: String,
    pub agent: Agent,
    /// The profile a new session starts from when it names none.
    pub default: bool,
    /// Whether its agent's account has usage limits to tell; one without
    /// says only how its login stands.
    pub reports_usage: bool,
    /// Whether its numbers are known, and if not, why.
    pub status: UsageStatus,
    /// What is left of its login's limits, as last told.
    pub gauges: Vec<Gauge>,
    /// The page where its agent's account shows its usage.
    pub usage_url: Option<String>,
}

impl ProfileRow {
    /// The most used of its limits, in percent; none without numbers.
    pub fn tightest(&self) -> Option<u32> {
        self.gauges.iter().map(|g| g.used).max()
    }

    /// Whether some limit has passed the warning threshold.
    pub fn warning(&self) -> bool {
        self.gauges.iter().any(Gauge::warning)
    }
}

/// The rows of the usage view: the profile with the tightest window first,
/// and those without numbers after, each in the order the profiles list
/// them.
pub fn usage_roster(mut rows: Vec<ProfileRow>) -> Vec<ProfileRow> {
    // A stable sort keeps the profiles' order among equals.
    rows.sort_by_key(|r| std::cmp::Reverse(r.tightest()));
    rows
}

/// Each of `settings`' profiles, in the order they list them, with what
/// `agents` tell of their agents and the limits `usage` holds for their
/// logins, at `now_ms` since the Unix epoch. `unavailable` holds the logins
/// whose last ask got no answer, and `login` how a profile's login stands,
/// when it is known.
pub fn profile_rows(
    settings: &Settings,
    agents: &AgentsWork,
    usage: &BTreeMap<Account, Vec<Limit>>,
    unavailable: &BTreeSet<Account>,
    login: impl Fn(&ProfileId) -> Option<LoginStanding>,
    now_ms: u64,
) -> Vec<ProfileRow> {
    settings
        .profiles
        .iter()
        .map(|p| {
            let agent = p.agent();
            let facts = agents.of(agent);
            let reports_usage = facts.is_some_and(|f| f.usage_limits);
            let account = Account {
                agent,
                home: p.home.clone(),
            };
            let status = if !p.poll_usage {
                UsageStatus::NotPolled
            } else if login(&p.id) == Some(LoginStanding::LoggedOut) {
                UsageStatus::NotSignedIn
            } else if unavailable.contains(&account) {
                UsageStatus::Unavailable
            } else if usage.contains_key(&account) {
                UsageStatus::Limits
            } else {
                UsageStatus::Waiting
            };
            ProfileRow {
                id: p.id.clone(),
                status,
                name: p.label().to_string(),
                agent,
                default: p.id == settings.profile,
                reports_usage,
                gauges: usage
                    .get(&account)
                    .filter(|_| reports_usage && status == UsageStatus::Limits)
                    .map(|limits| gauges(limits, now_ms))
                    .unwrap_or_default(),
                usage_url: facts.and_then(|f| f.usage_url.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tsumugi_model::profile::{AgentProfile, LoginStanding};
    use tsumugi_model::usage::{Severity, Window};
    use tsumugi_protocol::AgentFacts;

    use super::*;

    fn profile(id: &str, name: &str, agent: Agent, home: Option<&str>) -> AgentProfile {
        AgentProfile {
            id: id.parse().unwrap(),
            name: name.into(),
            home: home.map(PathBuf::from),
            ..AgentProfile::builtin(agent)
        }
    }

    fn settings() -> Settings {
        Settings {
            profile: "claude-work".parse().unwrap(),
            profiles: vec![
                profile("claude", "Claude Personal", Agent::Claude, None),
                profile("claude-work", "Claude Work", Agent::Claude, Some("/p/w")),
                profile("pi", "", Agent::Pi, None),
            ],
            ..Settings::default()
        }
    }

    fn agents() -> AgentsWork {
        let facts = |agent, usage_limits, usage_url: Option<&str>| AgentFacts {
            agent,
            program: agent.key().into(),
            usage_url: usage_url.map(String::from),
            usage_limits,
        };
        AgentsWork {
            agents: vec![
                facts(
                    Agent::Claude,
                    true,
                    Some("https://claude.ai/settings/usage"),
                ),
                facts(Agent::Pi, false, None),
            ],
        }
    }

    fn week(percent: f64) -> Limit {
        Limit {
            window: Window::Week,
            percent,
            resets_ms: None,
            severity: Severity::Normal,
        }
    }

    #[test]
    fn every_profile_is_listed_in_order_with_the_default_marked() {
        let rows = profile_rows(
            &settings(),
            &agents(),
            &BTreeMap::new(),
            &BTreeSet::new(),
            |_| None,
            0,
        );
        let listed: Vec<_> = rows
            .iter()
            .map(|r| (r.id.to_string(), r.name.as_str(), r.agent, r.default))
            .collect();
        assert_eq!(
            listed,
            [
                ("claude".into(), "Claude Personal", Agent::Claude, false),
                ("claude-work".into(), "Claude Work", Agent::Claude, true),
                ("pi".into(), "pi", Agent::Pi, false),
            ]
        );
    }

    #[test]
    fn each_profile_shows_the_limits_of_its_own_login() {
        let work = Account {
            agent: Agent::Claude,
            home: Some("/p/w".into()),
        };
        let usage = BTreeMap::from([(work, vec![week(30.0)])]);
        let rows = profile_rows(
            &settings(),
            &agents(),
            &usage,
            &BTreeSet::new(),
            |_| None,
            0,
        );
        assert!(rows[0].gauges.is_empty());
        assert_eq!(rows[1].gauges, gauges(&[week(30.0)], 0));
        assert_eq!(rows[1].gauges[0].left, "70% left");
    }

    #[test]
    fn a_profile_whose_agent_has_no_limits_reports_no_usage_and_each_links_its_agent_s_page() {
        let pi = Account {
            agent: Agent::Pi,
            home: None,
        };
        let usage = BTreeMap::from([(pi, vec![week(10.0)])]);
        let rows = profile_rows(
            &settings(),
            &agents(),
            &usage,
            &BTreeSet::new(),
            |_| None,
            0,
        );
        let pi = &rows[2];
        assert!(!pi.reports_usage);
        assert!(pi.gauges.is_empty());
        assert_eq!(pi.usage_url, None);
        assert!(rows[0].reports_usage);
        assert_eq!(
            rows[0].usage_url.as_deref(),
            Some("https://claude.ai/settings/usage")
        );
    }

    fn rows_with(
        settings: &Settings,
        usage: &BTreeMap<Account, Vec<Limit>>,
        unavailable: &BTreeSet<Account>,
        login: impl Fn(&ProfileId) -> Option<LoginStanding>,
    ) -> Vec<ProfileRow> {
        profile_rows(settings, &agents(), usage, unavailable, login, 0)
    }

    fn claude_accounts() -> (Account, Account) {
        (
            Account {
                agent: Agent::Claude,
                home: None,
            },
            Account {
                agent: Agent::Claude,
                home: Some("/p/w".into()),
            },
        )
    }

    #[test]
    fn a_profile_that_does_not_poll_says_so_whatever_was_last_read() {
        let (personal, _) = claude_accounts();
        let mut s = settings();
        s.profiles[0].poll_usage = false;
        let usage = BTreeMap::from([(personal, vec![week(95.0)])]);
        let rows = rows_with(&s, &usage, &BTreeSet::new(), |_| None);
        assert_eq!(rows[0].status, UsageStatus::NotPolled);
        assert!(rows[0].gauges.is_empty());
        assert_eq!(rows[1].status, UsageStatus::Waiting);
    }

    #[test]
    fn a_logged_out_profile_a_failed_ask_and_numbers_are_told_apart() {
        let (personal, work) = claude_accounts();
        let usage = BTreeMap::from([(personal.clone(), vec![week(10.0)])]);
        let out = rows_with(&settings(), &usage, &BTreeSet::new(), |id| {
            (id.as_str() == "claude").then_some(LoginStanding::LoggedOut)
        });
        assert_eq!(out[0].status, UsageStatus::NotSignedIn);
        assert!(out[0].gauges.is_empty(), "no stale numbers for a logout");

        let failed = BTreeSet::from([personal.clone(), work.clone()]);
        let rows = rows_with(&settings(), &usage, &failed, |_| None);
        assert_eq!(rows[0].status, UsageStatus::Unavailable);
        assert!(rows[0].gauges.is_empty());

        let rows = rows_with(&settings(), &usage, &BTreeSet::new(), |_| None);
        assert_eq!(rows[0].status, UsageStatus::Limits);
        assert_eq!(rows[1].status, UsageStatus::Waiting);
    }

    #[test]
    fn the_roster_lists_the_tightest_window_first_and_those_without_numbers_after() {
        let (personal, work) = claude_accounts();
        let usage = BTreeMap::from([
            (personal, vec![week(20.0)]),
            (work, vec![week(40.0), week(88.0)]),
        ]);
        let rows = rows_with(&settings(), &usage, &BTreeSet::new(), |_| None);
        let roster = usage_roster(rows);
        let ids: Vec<_> = roster.iter().map(|r| r.id.to_string()).collect();
        // `pi` reports no usage, so it has no numbers and comes last.
        assert_eq!(ids, ["claude-work", "claude", "pi"]);
        assert_eq!(roster[0].tightest(), Some(88));
        assert!(roster[0].warning());
        assert!(!roster[1].warning());
    }

    #[test]
    fn rows_without_numbers_follow_those_with_and_keep_their_order() {
        let (personal, _) = claude_accounts();
        let usage = BTreeMap::from([(personal, vec![week(1.0)])]);
        let mut s = settings();
        s.profiles[0].poll_usage = false;
        let rows = rows_with(&s, &usage, &BTreeSet::new(), |_| None);
        let ids: Vec<_> = usage_roster(rows)
            .iter()
            .map(|r| r.id.to_string())
            .collect();
        assert_eq!(ids, ["claude", "claude-work", "pi"]);
    }

    #[test]
    fn a_gauge_shows_what_is_used_or_what_is_left_as_chosen() {
        let g = &gauges(&[week(30.0)], 0)[0];
        assert_eq!(g.shown(false), "30% used");
        assert_eq!(g.shown(true), "70% left");
        assert!(!g.warning());
        assert!(gauges(&[week(80.0)], 0)[0].warning());
    }

    #[test]
    fn nothing_is_known_to_report_usage_before_the_host_tells_of_its_agents() {
        let rows = profile_rows(
            &settings(),
            &AgentsWork::default(),
            &BTreeMap::new(),
            &BTreeSet::new(),
            |_| None,
            0,
        );
        assert!(
            rows.iter()
                .all(|r| !r.reports_usage && r.usage_url.is_none())
        );
    }
}
