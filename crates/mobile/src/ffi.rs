//! What the app's Kotlin calls, through the bindings uniffi generates: the
//! phone with its key and its machines, pairing, watching a machine's waits
//! and replying to one.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tsumugi_model::Agent;
use tsumugi_net::{Fingerprint, Identity, Remote};

use crate::link::{self, LinkError};
use crate::machines::{self, Machine, Machines};
use crate::pairing::{self, Step};
use crate::reply::{Outcome, Replies};
use crate::waits::{Answer, Waiting};

/// The longest a lost connection waits before it is tried again.
const BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum PhoneError {
    #[error("{0}")]
    Failed(String),
}

fn failed(error: impl ToString) -> PhoneError {
    PhoneError::Failed(error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MachineEntry {
    /// The machine's key, in hex.
    pub key: String,
    pub name: String,
    pub address: String,
}

impl From<Machine> for MachineEntry {
    fn from(machine: Machine) -> Self {
        Self {
            key: machine.key.to_string(),
            name: machine.name,
            address: machine.address,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum WaitAnswer {
    Reply,
    AtWindow { asks: String },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WaitEntry {
    pub session: String,
    pub wait: u64,
    pub name: String,
    pub workspace: String,
    /// What the user calls the agent.
    pub agent: String,
    /// The agent as a reply names it.
    pub agent_key: String,
    pub answer: WaitAnswer,
    pub last_user: Option<String>,
    pub last_agent: Option<String>,
}

impl From<Waiting> for WaitEntry {
    fn from(waiting: Waiting) -> Self {
        Self {
            session: waiting.session,
            wait: waiting.wait,
            name: waiting.name,
            workspace: waiting.workspace,
            agent: waiting.agent.label().into(),
            agent_key: waiting.agent.key().into(),
            answer: match waiting.answer {
                Answer::Reply => WaitAnswer::Reply,
                Answer::AtWindow { asks } => WaitAnswer::AtWindow { asks },
            },
            last_user: waiting.last_user,
            last_agent: waiting.last_agent,
        }
    }
}

/// How the connection to a machine stands.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Standing {
    Connecting,
    /// The waits told are the machine's as they are now.
    Live,
    /// Lost, and tried again shortly; the waits told last are as old as
    /// the last of them.
    Lost {
        why: String,
    },
    /// The machine refused the phone, or they share no version: connecting
    /// again does not help until something changes.
    Refused {
        why: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PairStep {
    NotAccepting,
    Shown { code: String },
    Mismatched,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ReplyOutcome {
    Sent,
    NotSent {
        why: String,
    },
    Refused {
        why: String,
    },
    Unknown {
        why: String,
    },
    /// Not sent, since a reply to the same wait went already or may have.
    Held {
        why: String,
    },
}

impl From<Outcome> for ReplyOutcome {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Sent => ReplyOutcome::Sent,
            Outcome::NotSent { why } => ReplyOutcome::NotSent { why },
            Outcome::Refused { why } => ReplyOutcome::Refused { why },
            Outcome::Unknown { why } => ReplyOutcome::Unknown { why },
        }
    }
}

#[uniffi::export(with_foreign)]
pub trait WorkListener: Send + Sync {
    fn standing(&self, standing: Standing);
    fn waits(&self, waits: Vec<WaitEntry>);
}

#[uniffi::export(with_foreign)]
pub trait PairListener: Send + Sync {
    fn step(&self, step: PairStep);
    fn paired(&self, machine: MachineEntry);
    fn failed(&self, why: String);
}

/// This phone: its key, the machines it paired with, and the work it
/// runs.
#[derive(uniffi::Object)]
pub struct Phone {
    identity: Identity,
    machines: Machines,
    runtime: tokio::runtime::Runtime,
    replies: Mutex<Replies>,
}

/// A task the phone runs, ended when stopped or let go.
#[derive(uniffi::Object)]
pub struct Task {
    task: tokio::task::AbortHandle,
}

#[uniffi::export]
impl Task {
    pub fn stop(&self) {
        self.task.abort();
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn key(text: &str) -> Result<Fingerprint, PhoneError> {
    text.parse().map_err(failed)
}

#[uniffi::export]
impl Phone {
    /// The phone whose key and machines are kept in `dir`, the app's own
    /// files.
    #[uniffi::constructor]
    pub fn new(dir: String) -> Result<Arc<Self>, PhoneError> {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).map_err(failed)?;
        let identity = Identity::load_or_create(dir.join("client.key")).map_err(failed)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            identity,
            machines: Machines::new(&dir),
            runtime,
            replies: Mutex::default(),
        }))
    }

    /// This phone's key, as the work machine lists it.
    pub fn key(&self) -> String {
        self.identity.fingerprint().to_string()
    }

    pub fn machines(&self) -> Result<Vec<MachineEntry>, PhoneError> {
        Ok(self
            .machines
            .list()
            .map_err(failed)?
            .into_iter()
            .map(MachineEntry::from)
            .collect())
    }

    /// Pair under `name` with the machine at `address`, telling `listener`
    /// how far it has come; the machine is kept once it paired.
    pub fn pair(
        &self,
        address: String,
        name: String,
        listener: Arc<dyn PairListener>,
    ) -> Result<Arc<Task>, PhoneError> {
        let address = machines::address(&address).map_err(failed)?;
        let identity = self.identity.clone();
        let kept = Machines::new(self.machines.dir());
        let task = self.runtime.spawn(async move {
            let step = |step: Step| {
                listener.step(match step {
                    Step::NotAccepting => PairStep::NotAccepting,
                    Step::Shown { code } => PairStep::Shown { code },
                    Step::Mismatched => PairStep::Mismatched,
                })
            };
            match pairing::pair(&identity, &address, &name, step).await {
                Ok(server) => {
                    let machine = Machine {
                        key: server,
                        name: host(&address).to_string(),
                        address,
                    };
                    match kept.keep(machine.clone()) {
                        Ok(()) => listener.paired(machine.into()),
                        Err(error) => listener.failed(error.to_string()),
                    }
                }
                Err(why) => listener.failed(why),
            }
        });
        Ok(Arc::new(Task {
            task: task.abort_handle(),
        }))
    }

    /// Reach the machine of `key` at `address` from now on, without pairing
    /// again.
    pub fn readdress(&self, key: String, address: String) -> Result<MachineEntry, PhoneError> {
        let key = self::key(&key)?;
        Ok(self
            .machines
            .readdress(&key, &address)
            .map_err(failed)?
            .into())
    }

    pub fn rename(&self, key: String, name: String) -> Result<MachineEntry, PhoneError> {
        let key = self::key(&key)?;
        Ok(self.machines.rename(&key, &name).map_err(failed)?.into())
    }

    /// Keep the machine of `key` no more; it knows this phone until it
    /// revokes it.
    pub fn forget(&self, key: String) -> Result<(), PhoneError> {
        let key = self::key(&key)?;
        self.machines.forget(&key).map_err(failed)
    }

    /// Watch the waits of the machine of `key`, telling `listener` how the
    /// connection stands and each time they change, connecting again while
    /// it is lost.
    pub fn watch(
        &self,
        key: String,
        listener: Arc<dyn WorkListener>,
    ) -> Result<Arc<Task>, PhoneError> {
        let remote = self.remote(&key)?;
        let task = self.runtime.spawn(async move {
            let mut backoff = Duration::from_secs(1);
            loop {
                listener.standing(Standing::Connecting);
                let mut live = false;
                let error = link::watch(&remote, |waits| {
                    if !live {
                        live = true;
                        listener.standing(Standing::Live);
                    }
                    listener.waits(waits.into_iter().map(WaitEntry::from).collect());
                })
                .await;
                if !error.passing() {
                    listener.standing(Standing::Refused {
                        why: error.to_string(),
                    });
                    return;
                }
                listener.standing(Standing::Lost {
                    why: error.to_string(),
                });
                if live {
                    backoff = Duration::from_secs(1);
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(BACKOFF);
            }
        });
        Ok(Arc::new(Task {
            task: task.abort_handle(),
        }))
    }

    /// Reply `text` to the wait `wait` of `session`, whose agent is
    /// `agent`, on the machine of `key`. Waits for the outcome; called off
    /// the main thread.
    pub fn reply(
        &self,
        key: String,
        session: String,
        agent: String,
        wait: u64,
        text: String,
    ) -> ReplyOutcome {
        let not_sent = |why: String| ReplyOutcome::NotSent { why };
        let remote = match self.remote(&key) {
            Ok(remote) => remote,
            Err(error) => return not_sent(error.to_string()),
        };
        let Some(agent) = Agent::parse(&agent) else {
            return not_sent(format!("no agent is called {agent}"));
        };
        if text.trim().is_empty() {
            return not_sent("the reply is empty".into());
        }
        if let Err(held) = self.replies.lock().unwrap().begin(&session, wait) {
            return ReplyOutcome::Held {
                why: held.to_string(),
            };
        }
        let outcome = self
            .runtime
            .block_on(link::reply(&remote, &session, agent, wait, &text));
        self.replies.lock().unwrap().end(&session, wait, &outcome);
        outcome.into()
    }
}

impl Phone {
    fn remote(&self, key: &str) -> Result<Remote, PhoneError> {
        let machine = self.machines.get(&self::key(key)?).map_err(failed)?;
        Ok(Remote::new(
            self.identity.clone(),
            machine.key,
            machine.address,
        ))
    }
}

/// The host part of `address`, a machine's name until the user gives it
/// another.
fn host(address: &str) -> &str {
    let host = address.rsplit_once(':').map_or(address, |(host, _)| host);
    host.trim_start_matches('[').trim_end_matches(']')
}

impl From<LinkError> for PhoneError {
    fn from(error: LinkError) -> Self {
        failed(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_is_first_named_by_the_host_it_was_reached_at() {
        assert_eq!(host("work-box.local:47130"), "work-box.local");
        assert_eq!(host("[fd00::5]:47130"), "fd00::5");
    }
}
