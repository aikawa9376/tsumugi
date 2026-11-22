use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AgentPermissions, SessionId};

/// The program that owns a session's conversation.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    Claude,
    Codex,
    Pi,
    OpenCode,
    Cursor,
}

impl Agent {
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Pi,
        Self::OpenCode,
        Self::Cursor,
    ];
    pub const fn key(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::OpenCode => "opencode",
            Self::Cursor => "cursor",
        }
    }

    /// What the user calls the agent.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Pi => "Pi",
            Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor CLI",
        }
    }

    /// The agent after this one, going round to the first after the last.
    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// The agent before this one, going round to the last before the first.
    pub fn prev(self) -> Self {
        let at = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(at + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "pi" => Some(Self::Pi),
            "opencode" => Some(Self::OpenCode),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}

/// An agent session tsumugi runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Tsumugi's record of the session, for one Tsumugi started.
    pub delegation: Option<SessionId>,
    /// The code its record keeps; none for one started before codes were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<crate::SessionCode>,
    /// The session that opened this one; none for one the user started.
    pub parent: Option<SessionId>,
    /// Whether the session that opened it asked that it be shown to the user.
    #[serde(default)]
    pub show: bool,
    pub agent: Agent,
    /// The profile it started from, and the folder of that profile's own it
    /// runs in, none for its agent's own environment; the agent's own
    /// profile for one started before sessions were.
    pub profile: crate::profile::ProfileId,
    pub home: Option<PathBuf>,
    /// What its profile was called when it started, as the settings it
    /// started with say; empty while that is not known.
    #[serde(default)]
    pub profile_name: String,
    /// Which session this is: the daemon's id.
    pub id: String,
    /// The conversation's id; empty until the conversation tells it.
    pub session_id: String,
    /// The process the session runs in, as Claude Code lists it; unknown
    /// until it is listed.
    pub pid: Option<u32>,
    pub name: String,
    pub cwd: PathBuf,
    pub state: State,
    /// What its program said with the state it last reported, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// When the session started, in milliseconds since the Unix epoch.
    pub started_ms: Option<u64>,
    /// The most recent input sent through Tsumugi, kept on the daemon session.
    pub last_input_ms: Option<u64>,
    pub permissions: AgentPermissions,
    /// Whether the session runs with tsumugi's editor as its external editor,
    /// so that asking it for its input hands the input to the window; false
    /// when that is not known, as for one started by an earlier version.
    pub tsumugi_editor: bool,
    /// The daemon id of the interrupted session it was brought back for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from: Option<String>,
    /// Why nothing runs for it though it is listed: a session that is
    /// restarted or woken takes its place. None for a session that runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<Rest>,
}

/// Why a listed session runs no program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rest {
    /// Its program exited, cleanly or not, without being ended from Tsumugi.
    Exited,
    /// Tsumugi stopped its program as it had been done and left alone for a
    /// while; focusing it or sending to it wakes it.
    Hibernated,
}

impl Rest {
    /// The reason in one word.
    pub const fn word(self) -> &'static str {
        match self {
            Self::Exited => "exited",
            Self::Hibernated => "hibernated",
        }
    }

    /// The reason a word names.
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Exited, Self::Hibernated]
            .into_iter()
            .find(|rest| rest.word() == word)
    }
}

impl Session {
    /// Whether it can be put to sleep and woken into its conversation again:
    /// the host keeps a record of it, it told its conversation, it is not
    /// Cursor's, which cannot be resumed from Tsumugi, and it does not rest
    /// already.
    pub fn wakeable(&self) -> bool {
        self.rest.is_none()
            && self.delegation.is_some()
            && !self.session_id.is_empty()
            && self.agent != Agent::Cursor
    }

    /// Whether the session was started to read only.
    pub fn reads_only(&self) -> bool {
        self.permissions == AgentPermissions::ReadOnly
    }
}

/// A conversation Claude Code lists as held by a process running now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub session_id: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Waiting(Wait),
    /// Started by tsumugi and not yet reported on: Claude Code runs no hooks
    /// before the folder is trusted, so it may be asking that.
    Starting,
    Working,
    /// Its turn ended, and work it started, as subagents or shells, still
    /// runs behind it and brings it back once it ends.
    Background,
    #[default]
    Idle,
    Done,
    Failed,
}

impl State {
    /// Whether every key typed goes to the session as an answer.
    pub fn takes_every_key(&self) -> bool {
        matches!(self, State::Waiting(_) | State::Starting)
    }

    /// The state in one word.
    pub fn word(&self) -> &'static str {
        match self {
            State::Waiting(_) => "waiting",
            State::Starting => "starting",
            State::Working => "working",
            State::Background => "background",
            State::Idle => "idle",
            State::Done => "done",
            State::Failed => "failed",
        }
    }

    /// The state a program reported of itself in OSC 7501: its `state`,
    /// and for `blocked` what it waits on.
    pub fn reported(state: &str, kind: Option<&str>) -> State {
        match state {
            "working" => State::Working,
            "blocked" => State::Waiting(match kind {
                Some("permission") => Wait::Permission,
                Some("question") => Wait::Input,
                Some("auth") => Wait::Other("a login".into()),
                Some(other) => Wait::Other(other.to_string()),
                None => Wait::Other("an answer".into()),
            }),
            "done" => State::Done,
            "error" => State::Failed,
            _ => State::Idle,
        }
    }
}

/// What a waiting session is waiting for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wait {
    Permission,
    Input,
    Other(String),
}

impl std::fmt::Display for Wait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Wait::Permission => f.write_str("permission prompt"),
            Wait::Input => f.write_str("waiting for input"),
            Wait::Other(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("cannot read the session list: {0}")]
pub struct ParseError(#[from] serde_json::Error);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    session_id: String,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    kind: Option<String>,
}

/// The conversations running in a terminal of their own, tsumugi's sessions
/// among them, from the output of `claude agents --json`.
pub fn parse_agents(json: &str) -> Result<Vec<Running>, ParseError> {
    let raws: Vec<Raw> = serde_json::from_str(json)?;
    Ok(raws
        .into_iter()
        .filter(|r| r.kind.as_deref() == Some("interactive"))
        .filter_map(|raw| {
            Some(Running {
                session_id: raw.session_id,
                pid: raw.pid?,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_is_called_by_its_program_s_own_name() {
        assert_eq!(Agent::Claude.label(), "Claude Code");
        assert_eq!(Agent::Codex.label(), "Codex");
        assert_eq!(Agent::Pi.label(), "Pi");
        assert_eq!(Agent::parse("pi"), Some(Agent::Pi));
        assert_eq!(Agent::OpenCode.label(), "OpenCode");
        assert_eq!(Agent::parse("opencode"), Some(Agent::OpenCode));
        assert_eq!(
            serde_json::to_value(Agent::OpenCode).unwrap(),
            Agent::OpenCode.key()
        );
    }

    #[test]
    fn a_session_can_be_put_to_sleep_only_when_it_can_be_woken_into_its_conversation() {
        let mut s = crate::testing::session("a").session_id("c1").build();
        assert!(!s.wakeable(), "no record of it kept by the host");
        s.delegation = Some(SessionId::new());
        assert!(s.wakeable());
        s.session_id.clear();
        assert!(!s.wakeable(), "no conversation told");
        s.session_id = "c1".into();
        s.agent = Agent::Cursor;
        assert!(!s.wakeable(), "Cursor cannot be resumed");
        s.agent = Agent::Claude;
        s.rest = Some(Rest::Hibernated);
        assert!(!s.wakeable(), "already asleep");
    }

    fn one(json: &str) -> Running {
        parse_agents(json).unwrap().remove(0)
    }

    #[test]
    fn switching_the_agent_goes_through_every_one_and_round_again() {
        let mut seen = vec![Agent::Claude];
        for _ in 1..Agent::ALL.len() {
            seen.push(seen.last().unwrap().next());
        }
        assert_eq!(seen, Agent::ALL);
        assert_eq!(seen.last().unwrap().next(), Agent::Claude);
    }

    #[test]
    fn switching_the_agent_back_undoes_switching_it() {
        for agent in Agent::ALL {
            assert_eq!(agent.next().prev(), agent);
        }
        assert_eq!(Agent::Claude.prev(), Agent::ALL[Agent::ALL.len() - 1]);
    }

    #[test]
    fn a_running_conversation_is_read_from_claude_s_listing_with_its_process() {
        let s = one(r#"[{"pid":1,"cwd":"/r","kind":"interactive","startedAt":5,
            "sessionId":"abc","name":"r-6a","status":"idle"}]"#);
        assert_eq!(
            s,
            Running {
                session_id: "abc".into(),
                pid: 1
            }
        );
    }

    #[test]
    fn claude_s_background_sessions_and_those_without_a_process_are_not_listed() {
        let json = r#"[
            {"id":"j","cwd":"/r","kind":"background","sessionId":"a","state":"done"},
            {"pid":7,"id":"k","cwd":"/r","kind":"background","sessionId":"b","status":"idle"},
            {"cwd":"/r","kind":"interactive","sessionId":"c","status":"idle"}
        ]"#;
        assert!(parse_agents(json).unwrap().is_empty());
    }

    #[test]
    fn a_reported_state_is_read_with_what_it_waits_on() {
        assert_eq!(State::reported("working", None), State::Working);
        assert_eq!(
            State::reported("blocked", Some("permission")),
            State::Waiting(Wait::Permission)
        );
        assert_eq!(
            State::reported("blocked", Some("question")),
            State::Waiting(Wait::Input)
        );
        assert_eq!(
            State::reported("blocked", Some("auth")),
            State::Waiting(Wait::Other("a login".into()))
        );
        assert_eq!(State::reported("done", None), State::Done);
        assert_eq!(State::reported("error", None), State::Failed);
        assert_eq!(State::reported("idle", None), State::Idle);
        assert_eq!(State::reported("pondering", None), State::Idle);
    }

    #[test]
    fn keys_answer_a_session_waiting_on_a_choice_or_still_starting() {
        assert!(State::Waiting(Wait::Input).takes_every_key());
        assert!(State::Starting.takes_every_key());
        assert!(!State::Idle.takes_every_key());
        assert!(!State::Working.takes_every_key());
        assert!(!State::Background.takes_every_key());
        assert!(!State::Done.takes_every_key());
        assert!(!State::Failed.takes_every_key());
    }

    #[test]
    fn a_session_whose_turn_ended_with_work_running_behind_it_is_in_the_background() {
        assert_eq!(State::Background.word(), "background");
    }

    #[test]
    fn malformed_output_is_an_error() {
        assert!(parse_agents("not json").is_err());
    }
}
