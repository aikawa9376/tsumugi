//! The phone's connection to a work machine's work host, through the
//! gateway and over TLS that pins the machine's key: the hello and the
//! versions, the work watched as snapshot and patches, and a reply applied
//! once under its own stamp.

use std::time::Duration;

use tokio::io::{AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tsumugi_model::Agent;
use tsumugi_net::{Channel, Remote};
use tsumugi_protocol::client::Frames;
use tsumugi_protocol::{
    Answer, Ask, Event, Hello, MutationStamp, RemoteInfo, Request, SPOKEN, Sight, Version,
    WorkSnapshot, agree, decode, encode,
};

use crate::reply::{Outcome, settled};
use crate::waits::{Waiting, conversations, waiting};

/// How long a work host has to say hello, or to answer what it is asked.
const STEP: Duration = Duration::from_secs(10);
/// How long a reply's outcome is asked after, before it is told unknown.
const OUTCOME: Duration = Duration::from_secs(30);

/// Why the phone is not connected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    /// The machine answered and refused this phone: revoked, or never
    /// paired. Connecting again does not help.
    #[error("the work machine does not know this phone; pair it again")]
    NotPaired,
    #[error("the machine at this address is not the one paired")]
    OtherMachine,
    #[error("this app is too old for Tsumugi on the work machine; update the app")]
    AppTooOld,
    #[error("Tsumugi on the work machine is too old for this app; update it there")]
    HostTooOld,
    /// Nothing answered, or the connection was lost: it may come back.
    #[error("{0}")]
    Lost(String),
}

impl LinkError {
    /// Whether connecting again may help.
    pub fn passing(&self) -> bool {
        matches!(self, LinkError::Lost(_))
    }
}

impl From<tsumugi_net::Error> for LinkError {
    fn from(error: tsumugi_net::Error) -> Self {
        match error {
            tsumugi_net::Error::NotPaired => LinkError::NotPaired,
            tsumugi_net::Error::OtherServer => LinkError::OtherMachine,
            tsumugi_net::Error::Unreachable(address) => {
                LinkError::Lost(format!("nothing answered at {address}"))
            }
            error => LinkError::Lost(error.to_string()),
        }
    }
}

impl From<std::io::Error> for LinkError {
    fn from(error: std::io::Error) -> Self {
        tsumugi_net::Error::from(error).into()
    }
}

/// Which side is to update, for versions that share none.
pub fn versions(ours: &[Version], theirs: &[Version]) -> Result<Version, LinkError> {
    agree(ours, theirs).ok_or_else(|| {
        if theirs.iter().max() > ours.iter().max() {
            LinkError::AppTooOld
        } else {
            LinkError::HostTooOld
        }
    })
}

type Stream = TlsStream<TcpStream>;

/// A connection to the work host, welcomed in a version both speak.
pub struct Work {
    events: Frames<BufReader<ReadHalf<Stream>>>,
    requests: WriteHalf<Stream>,
}

impl Work {
    pub async fn open(remote: &Remote) -> Result<Self, LinkError> {
        let stream = remote.open(Channel::Work).await?;
        let (read, requests) = tokio::io::split(stream);
        let mut work = Self {
            events: Frames::new(BufReader::new(read)),
            requests,
        };
        let hello: Hello = match tokio::time::timeout(STEP, work.events.next()).await {
            Ok(Ok(Some(line))) => decode(line).map_err(|e| LinkError::Lost(e.to_string()))?,
            Ok(Ok(None)) => return Err(LinkError::Lost("the work host hung up".into())),
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => return Err(LinkError::Lost("the work host said no hello".into())),
        };
        versions(SPOKEN, &hello.speaks)?;
        work.send(&Request::Hello {
            versions: SPOKEN.to_vec(),
        })
        .await?;
        loop {
            match work.next_timed().await? {
                Event::Welcome { .. } => return Ok(work),
                Event::Refused { versions: theirs } => {
                    return Err(versions(SPOKEN, &theirs)
                        .err()
                        .unwrap_or(LinkError::HostTooOld));
                }
                _ => {}
            }
        }
    }

    pub async fn send(&mut self, request: &Request) -> Result<(), LinkError> {
        let line = encode(request);
        tokio::time::timeout(STEP, async {
            self.requests.write_all(line.as_bytes()).await?;
            self.requests.flush().await
        })
        .await
        .map_err(|_| LinkError::Lost("writing to the work host timed out".into()))?
        .map_err(LinkError::from)
    }

    /// The next event the host tells; an error once it hangs up.
    pub async fn next(&mut self) -> Result<Event, LinkError> {
        loop {
            match self.events.next().await? {
                Some(line) => {
                    if let Ok(event) = decode(&line) {
                        return Ok(event);
                    }
                }
                None => return Err(LinkError::Lost("the work host hung up".into())),
            }
        }
    }

    async fn next_timed(&mut self) -> Result<Event, LinkError> {
        tokio::time::timeout(STEP, self.next())
            .await
            .map_err(|_| LinkError::Lost("the work host did not answer".into()))?
    }

    /// What the host answers `ask`, asked as the only ask on this
    /// connection.
    async fn ask(&mut self, ask: Ask) -> Result<Answer, LinkError> {
        self.send(&Request::Ask {
            asked: 1,
            ask: Box::new(ask),
        })
        .await?;
        loop {
            if let Event::Answer { asked: 1, answer } = self.next_timed().await? {
                return Ok(answer);
            }
        }
    }
}

/// What the work host shows, as the phone keeps it.
#[derive(Debug, Default)]
pub struct Shown {
    work: WorkSnapshot,
    asked: Option<std::collections::BTreeSet<String>>,
}

/// What to do after an event.
#[derive(Debug, PartialEq)]
pub enum Next {
    Nothing,
    /// The waits changed.
    Tell(Vec<Waiting>),
    /// Say to the host, then tell the waits.
    Ask(Box<Request>, Vec<Waiting>),
}

impl Shown {
    /// Take `event` in; what follows from it.
    pub fn take(&mut self, event: Event) -> Next {
        match event {
            Event::Snapshot(work) => self.work = *work,
            Event::Patch(patch) if patch.seq == self.work.seq.next() => {
                for change in patch.changes {
                    self.work.apply(change);
                }
                self.work.seq = patch.seq;
            }
            // A patch before the snapshot's last change is in it already.
            Event::Patch(patch) if patch.seq <= self.work.seq => return Next::Nothing,
            // One missed: the whole work again.
            Event::Patch(_) => return Next::Ask(Box::new(Request::Snapshot), Vec::new()),
            _ => return Next::Nothing,
        }
        let waits = waiting(&self.work);
        let wanted = conversations(&self.work);
        if self.asked.as_ref() == Some(&wanted) {
            return Next::Tell(waits);
        }
        self.asked = Some(wanted.clone());
        Next::Ask(Box::new(sight(wanted)), waits)
    }

    pub fn work(&self) -> &WorkSnapshot {
        &self.work
    }
}

/// What the phone shows: the conversations of the sessions that wait, and
/// nothing else.
pub fn sight(conversations: std::collections::BTreeSet<String>) -> Request {
    Request::Watch(Box::new(Sight {
        conversations,
        ..Sight::default()
    }))
}

/// Watch the work at `remote`, telling `tell` the waits each time they
/// change, until the connection fails: why it did.
pub async fn watch(remote: &Remote, mut tell: impl FnMut(Vec<Waiting>)) -> LinkError {
    let mut work = match Work::open(remote).await {
        Ok(work) => work,
        Err(error) => return error,
    };
    if let Err(error) = work.send(&sight(Default::default())).await {
        return error;
    }
    let mut shown = Shown::default();
    loop {
        let event = match work.next().await {
            Ok(event) => event,
            Err(error) => return error,
        };
        match shown.take(event) {
            Next::Nothing => {}
            Next::Tell(waits) => tell(waits),
            Next::Ask(request, waits) => {
                if let Err(error) = work.send(&request).await {
                    return error;
                }
                if !matches!(*request, Request::Snapshot) {
                    tell(waits);
                }
            }
        }
    }
}

/// The hosts' identities, as the gateway tells them.
pub async fn info(remote: &Remote) -> Result<RemoteInfo, LinkError> {
    let bytes = remote.info().await?;
    let info: RemoteInfo =
        serde_json::from_slice(&bytes).map_err(|e| LinkError::Lost(e.to_string()))?;
    info.validate().map_err(LinkError::Lost)?;
    Ok(info)
}

/// What is asked to reply `text` to the wait `wait` of `session`.
pub fn submit(session: &str, agent: Agent, wait: u64, text: &str) -> Request {
    Request::Ask {
        asked: 1,
        ask: Box::new(Ask::SubmitPrompt {
            session: session.into(),
            agent,
            text: text.into(),
            images: Vec::new(),
            wait: Some(wait),
        }),
    }
}

/// Reply `text` to the wait `wait` of `session`, once: anything that fails
/// before the reply is written leaves it not sent, and after, unknown.
pub async fn reply(remote: &Remote, session: &str, agent: Agent, wait: u64, text: &str) -> Outcome {
    let not_sent = |error: LinkError| Outcome::NotSent {
        why: error.to_string(),
    };
    let info = match info(remote).await {
        Ok(info) => info,
        Err(error) => return not_sent(error),
    };
    let mut work = match Work::open(remote).await {
        Ok(work) => work,
        Err(error) => return not_sent(error),
    };
    let epoch = match work.ask(Ask::MutationEpoch).await {
        Ok(Answer::MutationEpoch { epoch }) => epoch,
        Ok(_) => {
            return Outcome::NotSent {
                why: "the work machine takes no replies from a phone".into(),
            };
        }
        Err(error) => return not_sent(error),
    };
    let stamp = MutationStamp {
        instance: info.instance.clone(),
        work: info.work.generation.clone(),
        terminal: info.terminal.generation.clone(),
        epoch,
        id: uuid::Uuid::new_v4().simple().to_string(),
    };
    let apply = Ask::Apply {
        stamp: stamp.clone(),
        request: Box::new(submit(session, agent, wait, text)),
    };
    let unknown = |error: LinkError| Outcome::Unknown {
        why: error.to_string(),
    };
    let asked = tokio::time::timeout(OUTCOME, async {
        let mut answer = work.ask(apply).await?;
        loop {
            match answer {
                Answer::Mutation {
                    status: tsumugi_protocol::MutationState::Accepted,
                } => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    answer = work
                        .ask(Ask::MutationStatus {
                            stamp: stamp.clone(),
                        })
                        .await?;
                }
                Answer::Mutation { status } => return Ok(settled(status)),
                // An answer other than an outcome: the host may have typed it.
                _ => {
                    return Ok(Outcome::Unknown {
                        why: "the work machine said nothing of the reply".into(),
                    });
                }
            }
        }
    })
    .await;
    match asked {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => unknown(error),
        Err(_) => unknown(LinkError::Lost("the reply's outcome took too long".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsumugi_model::State;
    use tsumugi_model::testing::session;
    use tsumugi_protocol::testing::{listed, patch, snapshot};
    use tsumugi_protocol::{Seq, TreeChange};

    #[test]
    fn versions_that_share_none_say_which_side_is_to_update() {
        let v = Version;
        assert_eq!(versions(&[v(9)], &[v(8), v(9)]), Ok(v(9)));
        assert_eq!(versions(&[v(9)], &[v(10)]), Err(LinkError::AppTooOld));
        assert_eq!(versions(&[v(9)], &[v(8)]), Err(LinkError::HostTooOld));
        assert_eq!(versions(&[v(9)], &[]), Err(LinkError::HostTooOld));
    }

    #[test]
    fn a_refusal_of_the_phone_is_told_apart_from_a_machine_that_cannot_be_reached() {
        assert_eq!(
            LinkError::from(tsumugi_net::Error::NotPaired),
            LinkError::NotPaired
        );
        assert_eq!(
            LinkError::from(tsumugi_net::Error::OtherServer),
            LinkError::OtherMachine
        );
        let lost = LinkError::from(tsumugi_net::Error::Unreachable("w:47130".into()));
        assert!(lost.passing());
        assert!(!LinkError::NotPaired.passing());
        assert!(!LinkError::AppTooOld.passing());
    }

    fn waits(ids: &[(&str, u64)]) -> TreeChange {
        TreeChange::Waits {
            waits: ids.iter().map(|(id, n)| (id.to_string(), *n)).collect(),
        }
    }

    fn work_with(ids: &[(&str, u64)]) -> WorkSnapshot {
        snapshot()
            .seq(5)
            .listed(listed().sessions(ids.iter().map(|(id, _)| session(*id).state(State::Done))))
            .with(|w| {
                w.tree.waits = ids.iter().map(|(id, n)| (id.to_string(), *n)).collect();
            })
            .build()
    }

    #[test]
    fn a_snapshot_asks_the_host_to_read_the_conversations_of_the_sessions_that_wait() {
        let mut shown = Shown::default();

        let next = shown.take(Event::Snapshot(Box::new(work_with(&[("a", 1)]))));

        let Next::Ask(request, told) = next else {
            panic!("{next:?}")
        };
        assert_eq!(*request, sight(["a".to_string()].into_iter().collect()));
        assert_eq!(told.len(), 1);
    }

    #[test]
    fn a_patch_that_changes_nothing_of_what_is_read_only_tells_the_waits() {
        let mut shown = Shown::default();
        shown.take(Event::Snapshot(Box::new(work_with(&[("a", 1)]))));

        let next = shown.take(Event::Patch(patch(6).change(waits(&[("a", 2)])).build()));

        assert!(matches!(next, Next::Tell(_)), "{next:?}");
        assert_eq!(shown.work().seq, Seq(6));
        assert_eq!(shown.work().tree.waits["a"], 2);
    }

    #[test]
    fn a_patch_already_in_the_snapshot_is_passed_over() {
        let mut shown = Shown::default();
        shown.take(Event::Snapshot(Box::new(work_with(&[("a", 1)]))));

        let next = shown.take(Event::Patch(patch(5).change(waits(&[])).build()));

        assert_eq!(next, Next::Nothing);
        assert_eq!(shown.work().tree.waits["a"], 1);
    }

    #[test]
    fn a_patch_missed_asks_for_the_whole_work_again() {
        let mut shown = Shown::default();
        shown.take(Event::Snapshot(Box::new(work_with(&[("a", 1)]))));

        let next = shown.take(Event::Patch(patch(8).change(waits(&[])).build()));

        assert_eq!(next, Next::Ask(Box::new(Request::Snapshot), Vec::new()));
        assert_eq!(shown.work().seq, Seq(5));
    }

    #[test]
    fn a_reply_names_the_wait_it_answers_and_attaches_nothing() {
        let Request::Ask { ask, .. } = submit("a", Agent::Codex, 7, "go on") else {
            panic!()
        };
        assert_eq!(
            *ask,
            Ask::SubmitPrompt {
                session: "a".into(),
                agent: Agent::Codex,
                text: "go on".into(),
                images: Vec::new(),
                wait: Some(7),
            }
        );
    }
}
