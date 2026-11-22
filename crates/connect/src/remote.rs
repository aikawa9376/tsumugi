//! A selected work machine, reached through its gateway, and the checks made
//! before the window uses its hosts.

use crate::Target;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tsumugi_protocol::client::Connection;
use tsumugi_protocol::{RemoteInfo, SPOKEN, agree};

const DEADLINE: Duration = Duration::from_secs(15);

/// The hosts' discovery, accepted only when the window speaks their
/// protocols and, on a reconnection, they are the hosts it saw before.
pub(crate) fn admit(discovery: &[u8], expected: Option<&RemoteInfo>) -> Result<RemoteInfo, String> {
    let info: RemoteInfo =
        serde_json::from_slice(discovery).map_err(|e| format!("invalid remote discovery: {e}"))?;
    info.validate()?;
    if !info
        .work
        .protocols
        .iter()
        .any(|v| SPOKEN.iter().any(|ours| ours.0 == *v))
        || !info
            .terminal
            .protocols
            .iter()
            .any(|v| tsumugi_term::SPEAKS.contains(v))
    {
        return Err("remote host protocols are incompatible".into());
    }
    if let Some(expected) = expected
        && (expected.instance != info.instance || expected.terminal != info.terminal)
    {
        return Err(
            "remote terminal identity or generation changed; open a new connection explicitly"
                .into(),
        );
    }
    Ok(info)
}

/// Greets both hosts through the forwarded sockets and confirms they are
/// the ones the discovery named.
pub(crate) async fn greet(
    info: &RemoteInfo,
    work: &Path,
    terminal: &tsumugi_term::Daemon,
) -> Result<(), String> {
    let connection = Connection::open_unscoped(work)
        .await
        .map_err(|e| format!("cannot greet remote work host: {e}"))?;
    if connection.hello.endpoint.as_ref() != Some(&info.work)
        || connection.hello.terminal.as_ref() != Some(&info.terminal)
        || connection.hello.mutation_epoch.is_none()
        || agree(SPOKEN, &connection.hello.speaks).is_none()
    {
        return Err("remote work host identity or protocol changed during connection".into());
    }
    tokio::time::timeout(DEADLINE, terminal.sessions())
        .await
        .map_err(|_| "remote terminal handshake timed out".to_string())?
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Where this user keeps the key it pairs with: one for every client
/// instance, since any of them may open an environment that was paired.
pub fn client_key() -> PathBuf {
    tsumugi_model::paths::state_dir().join("tsumugi/net/client.key")
}

/// How far a pairing has come, as the person pairing is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairingStep {
    /// The work machine does not accept pairing requests now; the client
    /// keeps asking until it does.
    NotAccepting,
    /// The work machine's operator sees the request and is to type `code`.
    Shown { code: String },
    /// The operator typed another code; the request is made again with a
    /// new one.
    Mismatched,
}

/// Asks the work machine at `address` to pair under `name`, telling `step`
/// how far it has come, and returns the target that reaches it once the
/// operator typed the code shown.
pub async fn pair_with(
    address: &str,
    name: Option<&str>,
    step: impl FnMut(PairingStep),
) -> Result<Target, String> {
    if !crate::valid_address(address) {
        return Err(
            "give the work machine's address with its port, such as work-box.local:47130".into(),
        );
    }
    let identity =
        tsumugi_net::Identity::load_or_create(client_key()).map_err(|e| e.to_string())?;
    let name = name.map_or_else(tsumugi_net::host_name, str::to_string);
    pair_as(&identity, address, &name, step).await
}

async fn pair_as(
    identity: &tsumugi_net::Identity,
    address: &str,
    name: &str,
    step: impl FnMut(PairingStep),
) -> Result<Target, String> {
    // Asking again each second must not repeat what was said already.
    let telling = std::sync::Mutex::new((None::<PairingStep>, step));
    let tell = |now: PairingStep| {
        let mut telling = telling.lock().unwrap_or_else(|e| e.into_inner());
        let (told, step) = &mut *telling;
        if told.as_ref() != Some(&now) {
            step(now.clone());
            *told = Some(now);
        }
    };
    let asking = async {
        let mut code = tsumugi_net::PairingCode::generate();
        loop {
            let stream = tokio::time::timeout(DEADLINE, tokio::net::TcpStream::connect(address))
                .await
                .map_err(|_| format!("cannot reach {address}: timed out"))?
                .map_err(|e| format!("cannot reach {address}: {e}"))?;
            let shown = PairingStep::Shown {
                code: code.to_string(),
            };
            let asked = tsumugi_net::pair(identity, stream, name, &code, || tell(shown)).await;
            match asked {
                // No exchange ran, so the same code may be offered again.
                Err(tsumugi_net::PairError::Closed | tsumugi_net::PairError::Busy) => {
                    tell(PairingStep::NotAccepting);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                // A code is tried once; the next try asks with a new one.
                Err(tsumugi_net::PairError::Mismatch) => {
                    tell(PairingStep::Mismatched);
                    code = tsumugi_net::PairingCode::generate();
                }
                Err(tsumugi_net::PairError::Refused) => {
                    return Err(
                        "the work machine refused the request, or stopped allowing pairing".into(),
                    );
                }
                // The machine proved it knew the code, so its key is known;
                // whether it let this client in, only connecting tells.
                Err(tsumugi_net::PairError::Unconfirmed(server)) => {
                    let asked = tsumugi_net::Remote::new(identity.clone(), server, address)
                        .info()
                        .await;
                    return took(&asked).map(|()| server);
                }
                done => return done.map_err(|e| e.to_string()),
            }
        }
    };
    let server = tokio::time::timeout(PAIRING_WAIT, asking)
        .await
        .map_err(|_| "the work machine did not pair in time".to_string())??;
    Ok(Target::Native {
        address: address.into(),
        server: server.to_string(),
    })
}

/// Whether a pairing whose last answer was lost took, by what the work
/// machine said when asked with the client's key.
fn took(asked: &Result<Vec<u8>, tsumugi_net::Error>) -> Result<(), String> {
    match asked {
        Ok(_) => Ok(()),
        Err(tsumugi_net::Error::NotPaired) => {
            Err("the work machine did not finish pairing; pair again with a new code".into())
        }
        Err(error) => Err(format!(
            "the work machine may have paired, but cannot be asked to find out: {error}"
        )),
    }
}

/// Pairs as `pair_with` does and adds an environment for the work machine.
pub fn pair_cli(address: &str, name: Option<&str>, show: impl Fn(&str)) -> Result<String, String> {
    pair_cli_as(address, name, &format!("Work machine at {address}"), show)
}

/// [`pair_cli`], naming the environment it adds `environment_name`.
pub fn pair_cli_as(
    address: &str,
    name: Option<&str>,
    environment_name: &str,
    show: impl Fn(&str),
) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let target = runtime.block_on(pair_with(address, name, |step| {
        show(&match step {
            PairingStep::NotAccepting => "The work machine is not accepting pairing requests; waiting until it does (run `tsumugi remote pair` there, or allow pairing under Accept connections in its Settings)".to_string(),
            PairingStep::Shown { code } => format!("Type this code on the work machine: {code}"),
            PairingStep::Mismatched => "That was not the code; asking again with a new one".to_string(),
        })
    }))?;
    let Target::Native { server, .. } = &target else {
        unreachable!("pairing reaches a paired work machine");
    };
    let server = server.clone();
    let path = crate::environments::Catalog::path();
    let mut catalog = crate::environments::Catalog::load(&path)?;
    if let Some(known) = catalog.environments.iter().find(|p| p.target == target) {
        return Ok(format!(
            "Paired with the work machine {} again; its environment \"{}\" uses it",
            &server[..16],
            known.name
        ));
    }
    let expected = catalog.revision;
    catalog
        .environments
        .push(crate::environments::Environment::new(
            environment_name,
            target,
        ));
    catalog.save(&path, expected)?;
    Ok(format!(
        "Paired with the work machine {} and added the environment \"{environment_name}\"",
        &server[..16]
    ))
}

/// A work machine heard on the local network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    pub name: String,
    /// An address to pair with.
    pub address: String,
    /// The start of its key's fingerprint, to tell machines of one name apart.
    pub key: String,
}

/// The work machines announcing themselves on the local network.
pub async fn discover() -> Result<Vec<Heard>, String> {
    let heard = tsumugi_net::browse(Duration::from_secs(3))
        .await
        .map_err(|e| e.to_string())?;
    Ok(heard
        .into_iter()
        .filter_map(|found| {
            Some(Heard {
                address: found.addresses.first()?.to_string(),
                key: found.server.to_string()[..16].to_string(),
                name: found.name,
            })
        })
        .collect())
}

/// The work machines heard on the local network, one line each: its name,
/// an address to pair with, and the start of its key's fingerprint.
pub fn discover_cli() -> Result<Vec<String>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let heard = runtime.block_on(discover())?;
    Ok(heard
        .into_iter()
        .map(|heard| format!("{}  {}  {}", heard.name, heard.address, heard.key))
        .collect())
}

/// How long a client listens for its work machine on the local network
/// once the saved address failed.
const FIND_WAIT: Duration = Duration::from_secs(3);

/// How long the client waits for the operator to type its code.
const PAIRING_WAIT: Duration = Duration::from_secs(5 * 60);

/// A private folder for forwarded sockets, where their paths stay within
/// the platform's small limit.
fn private_sockets() -> Result<tempfile::TempDir, String> {
    let sockets = tempfile::Builder::new()
        .prefix("tsumugi-c-")
        .tempdir_in(tsumugi_sys::local::temp_dir())
        .map_err(|e| e.to_string())?;
    tsumugi_sys::fs::set_mode(sockets.path(), 0o700).map_err(|e| e.to_string())?;
    Ok(sockets)
}

/// The selected work machine, paired through its gateway.
#[derive(Debug, Clone)]
pub struct Remote {
    /// The address the work machine is kept at; shared by every clone, so a
    /// move tells each of them.
    address: Arc<std::sync::Mutex<String>>,
    identity: tsumugi_net::Identity,
    server: tsumugi_net::Fingerprint,
    sockets: Arc<tempfile::TempDir>,
    /// Where the local network last heard the machine, when that was not
    /// the saved address; shared by every clone, so a window reconnecting
    /// after the machine moved does not wait at the old address again.
    found: Arc<std::sync::Mutex<Option<String>>>,
}

impl Remote {
    pub fn new(target: &Target) -> Result<Self, String> {
        Self::with_key(target, &client_key())
    }

    pub fn with_key(target: &Target, key: &Path) -> Result<Self, String> {
        target.validate()?;
        let Target::Native { address, server } = target else {
            return Err("no paired work machine selected".into());
        };
        let server = server.parse().map_err(|_| "invalid work machine key")?;
        let identity = tsumugi_net::Identity::load_or_create(key).map_err(|e| e.to_string())?;
        let sockets = private_sockets()?;
        Ok(Self {
            address: Arc::new(std::sync::Mutex::new(address.clone())),
            identity,
            server,
            sockets: Arc::new(sockets),
            found: Arc::default(),
        })
    }

    /// The addresses to try, in order, before listening on the local network.
    fn candidates(&self) -> Vec<String> {
        let found = self.found.lock().unwrap().clone();
        found.into_iter().chain([self.saved()]).collect()
    }

    fn found_at(&self, address: &str) {
        *self.found.lock().unwrap() = (address != self.saved()).then(|| address.to_string());
    }

    /// The address the work machine is kept at now.
    fn saved(&self) -> String {
        self.address.lock().unwrap().clone()
    }

    /// The work machine is kept at `address` now, which every clone tries
    /// from the next connection on; where it was found before is forgotten.
    pub fn moved_to(&self, address: &str) {
        *self.address.lock().unwrap() = address.to_string();
        *self.found.lock().unwrap() = None;
    }

    /// How the work machine is named to the user.
    pub fn name(&self) -> String {
        self.saved()
    }

    pub fn work_socket(&self) -> PathBuf {
        self.sockets.path().join("work.sock")
    }

    pub fn terminal(&self) -> tsumugi_term::Daemon {
        tsumugi_term::Daemon::forwarded(self.sockets.path().join("terminal.sock"))
    }

    pub async fn connect(&self, expected: Option<&RemoteInfo>) -> Result<Tunnel, String> {
        let (net, discovery) = self.reach().await?;
        let info = admit(&discovery, expected)?;
        let forwarding = net
            .forward(&self.work_socket(), self.terminal().socket())
            .map_err(|e| e.to_string())?;
        greet(&info, &self.work_socket(), &self.terminal()).await?;
        Ok(Tunnel {
            remote: self.clone(),
            info,
            reached: net.address().to_string(),
            ports: Arc::new(Ports::new(net)),
            _forwarding: forwarding,
        })
    }

    /// The paired key at the saved address, or else wherever the local
    /// network hears it announced: an address is only where to try.
    async fn reach(&self) -> Result<(tsumugi_net::Remote, Vec<u8>), String> {
        let at =
            |address: String| tsumugi_net::Remote::new(self.identity.clone(), self.server, address);
        let mut failure = None;
        for address in self.candidates() {
            let remote = at(address);
            match remote.info().await {
                Ok(discovery) => {
                    self.found_at(remote.address());
                    return Ok((remote, discovery));
                }
                Err(tsumugi_net::Error::NotPaired) => {
                    return Err(self.explain(tsumugi_net::Error::NotPaired));
                }
                // What the saved address said is what is reported.
                Err(error) => failure = Some(error),
            }
        }
        let failure = failure.expect("the saved address is always tried");
        if let Ok(Some(found)) = tsumugi_net::find(self.server, FIND_WAIT).await {
            for address in found.addresses {
                let remote = at(address.to_string());
                match remote.info().await {
                    Ok(discovery) => {
                        self.found_at(remote.address());
                        return Ok((remote, discovery));
                    }
                    Err(tsumugi_net::Error::NotPaired) => {
                        return Err(self.explain(tsumugi_net::Error::NotPaired));
                    }
                    Err(_) => continue,
                }
            }
        }
        Err(self.explain(failure))
    }

    fn explain(&self, error: tsumugi_net::Error) -> String {
        let address = self.saved();
        match error {
            tsumugi_net::Error::NotPaired => format!(
                "the work machine at {address} refused this client's key: it no longer knows this computer. Pair again from the environment's heading in the tree, or with tsumugi connect pair"
            ),
            tsumugi_net::Error::OtherServer => {
                format!("the machine at {address} is not the paired work machine: its key differs")
            }
            error => format!("cannot reach the work machine at {address}: {error}"),
        }
    }
}

/// The way to a connected work machine's hosts, open while it is kept.
pub struct Tunnel {
    remote: Remote,
    info: RemoteInfo,
    /// The address the work machine answered at, which may not be the saved one.
    reached: String,
    ports: Arc<Ports>,
    _forwarding: tsumugi_net::Forwarding,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // The ports forwarded through it do not outlive it, whoever still
        // holds them.
        self.ports.close();
    }
}

/// The ports of a work machine reached from this computer, one local port
/// for each, for as long as the connection that made them lasts.
pub struct Ports {
    net: tsumugi_net::Remote,
    /// The runtime that keeps the connection, which the forwards run on
    /// whoever asks for one.
    runtime: tokio::runtime::Handle,
    open: std::sync::Mutex<Option<std::collections::BTreeMap<u16, tsumugi_net::PortForward>>>,
}

impl Ports {
    fn new(net: tsumugi_net::Remote) -> Self {
        Self {
            net,
            runtime: tokio::runtime::Handle::current(),
            open: std::sync::Mutex::new(Some(Default::default())),
        }
    }

    /// The port of this computer's loopback that reaches `port` of the work
    /// machine, made when it is not yet.
    pub fn forward(&self, port: u16) -> Result<u16, String> {
        let mut open = self.open.lock().unwrap();
        let open = open
            .as_mut()
            .ok_or("the work machine is no longer connected")?;
        if let Some(forward) = open.get(&port) {
            return Ok(forward.local());
        }
        let _entered = self.runtime.enter();
        let forward = self.net.forward_port(port).map_err(|e| e.to_string())?;
        let local = forward.local();
        open.insert(port, forward);
        Ok(local)
    }

    /// The port of the work machine that `local`, a port of this computer,
    /// reaches, when a forward made it.
    pub fn work_port(&self, local: u16) -> Option<u16> {
        self.forwarded()
            .into_iter()
            .find(|(_, forwarded)| *forwarded == local)
            .map(|(remote, _)| remote)
    }

    /// The ports forwarded, as (the work machine's, this computer's).
    pub fn forwarded(&self) -> Vec<(u16, u16)> {
        self.open
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .map(|(remote, forward)| (*remote, forward.local()))
            .collect()
    }

    /// Stop forwarding `port` of the work machine.
    pub fn stop(&self, port: u16) {
        if let Some(open) = self.open.lock().unwrap().as_mut() {
            open.remove(&port);
        }
    }

    /// Stop forwarding every port, and forward none again.
    fn close(&self) {
        self.open.lock().unwrap().take();
    }
}

impl Tunnel {
    /// The work of the environment `scope` names on the work machine, its
    /// changes kept in `journal`.
    pub fn work(
        &self,
        journal: Arc<crate::journal::Journal>,
        scope: Option<String>,
    ) -> Result<crate::work::Work, String> {
        crate::work::Work::new(self.remote.work_socket(), self.info.clone(), journal, scope)
    }

    pub fn info(&self) -> &RemoteInfo {
        &self.info
    }

    /// The work machine's ports reached from here, which end with the
    /// tunnel.
    pub fn ports(&self) -> Arc<Ports> {
        self.ports.clone()
    }

    /// The address the work machine answered at.
    pub fn reached(&self) -> &str {
        &self.reached
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoHosts;

    impl tsumugi_net::Hosts for NoHosts {
        fn current(&self) -> Result<tsumugi_net::HostsNow, String> {
            Err("no hosts".into())
        }
    }

    /// The pairing of a gateway listening on loopback, and its address.
    async fn gateway() -> (
        tempfile::TempDir,
        Arc<tsumugi_net::Pairing>,
        String,
        tokio::task::JoinHandle<()>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let server = tsumugi_net::Identity::generate().unwrap();
        let peers = Arc::new(tsumugi_net::PeerFile::open(dir.path().join("peers.json")).unwrap());
        let pairing = Arc::new(tsumugi_net::Pairing::new(
            server.fingerprint(),
            peers.clone(),
        ));
        let gateway =
            tsumugi_net::Gateway::new(&server, peers, pairing.clone(), Arc::new(NoHosts)).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let listening = tokio::spawn(async move {
            let _ = gateway.listen(listener).await;
        });
        (dir, pairing, address, listening)
    }

    async fn asked(pairing: &tsumugi_net::Pairing) -> tsumugi_net::Request {
        for _ in 0..1000 {
            if let Some(request) = pairing.requests().pop() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("no request arrived");
    }

    #[tokio::test]
    async fn pairing_waits_for_the_work_machine_and_asks_again_after_a_wrong_code() {
        let (_dir, pairing, address, listening) = gateway().await;
        let client = tsumugi_net::Identity::generate().unwrap();
        let (steps, mut heard) = tokio::sync::mpsc::unbounded_channel();
        let pairing_task = {
            let address = address.clone();
            tokio::spawn(async move {
                pair_as(&client, &address, "laptop", |step| {
                    let _ = steps.send(step);
                })
                .await
            })
        };
        assert_eq!(heard.recv().await, Some(PairingStep::NotAccepting));
        pairing.open(Duration::from_secs(60));
        let Some(PairingStep::Shown { code: first }) = heard.recv().await else {
            panic!("no code was shown");
        };
        let request = asked(&pairing).await;
        let wrong = if first == "000-000" {
            "111-111"
        } else {
            "000-000"
        };
        assert!(
            pairing
                .approve(request.id, wrong.parse().unwrap())
                .await
                .is_err()
        );
        assert_eq!(heard.recv().await, Some(PairingStep::Mismatched));
        let Some(PairingStep::Shown { code: second }) = heard.recv().await else {
            panic!("no new code was shown");
        };
        let request = asked(&pairing).await;
        pairing
            .approve(request.id, second.parse().unwrap())
            .await
            .unwrap();
        let target = pairing_task.await.unwrap().unwrap();
        assert!(matches!(target, Target::Native { address: reached, .. } if reached == address));
        listening.abort();
    }

    #[tokio::test]
    async fn a_forwarded_port_reaches_the_work_machine_and_ends_when_the_connection_does() {
        let dir = tempfile::tempdir().unwrap();
        let server = tsumugi_net::Identity::generate().unwrap();
        let peers = Arc::new(tsumugi_net::PeerFile::open(dir.path().join("peers.json")).unwrap());
        let client = tsumugi_net::Identity::generate().unwrap();
        tsumugi_net::Enroll::enroll(peers.as_ref(), client.fingerprint(), "laptop").unwrap();
        let pairing = Arc::new(tsumugi_net::Pairing::new(
            server.fingerprint(),
            peers.clone(),
        ));
        let gateway =
            tsumugi_net::Gateway::new(&server, peers, pairing, Arc::new(NoHosts)).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let listening = tokio::spawn(async move {
            let _ = gateway.listen(listener).await;
        });
        let server = server.fingerprint();
        let echo = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echoed = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let (mut read, mut write) = stream.split();
                    let _ = tokio::io::copy(&mut read, &mut write).await;
                });
            }
        });
        let ports = Ports::new(tsumugi_net::Remote::new(client, server, address));
        let local = ports.forward(echoed).unwrap();
        assert_eq!(ports.forward(echoed), Ok(local), "one local port for each");
        assert_eq!(ports.forwarded(), [(echoed, local)]);
        assert_eq!(ports.work_port(local), Some(echoed));
        assert_eq!(
            ports.work_port(echoed),
            None,
            "only a port made here stands for one there"
        );
        {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut near = tokio::net::TcpStream::connect(("127.0.0.1", local))
                .await
                .unwrap();
            near.write_all(b"ping").await.unwrap();
            let mut back = [0u8; 4];
            tokio::time::timeout(Duration::from_secs(5), near.read_exact(&mut back))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(&back, b"ping");
        }
        ports.stop(echoed);
        assert!(ports.forwarded().is_empty());
        let again = ports.forward(echoed).unwrap();
        ports.close();
        assert!(ports.forwarded().is_empty());
        assert!(
            ports.forward(echoed).is_err(),
            "none is made after it closed"
        );
        let mut refused = false;
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(("127.0.0.1", again))
                .await
                .is_err()
            {
                refused = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(refused, "the forwarded port still accepts connections");
        listening.abort();
    }

    #[test]
    fn a_pairing_whose_last_answer_was_lost_took_only_when_the_key_is_let_in() {
        assert!(took(&Ok(b"{}".to_vec())).is_ok());
        let refused = took(&Err(tsumugi_net::Error::NotPaired)).unwrap_err();
        assert!(refused.contains("did not finish pairing"), "{refused}");
        let unknown = took(&Err(tsumugi_net::Error::Unreachable("w:1".into()))).unwrap_err();
        assert!(unknown.contains("may have paired"), "{unknown}");
    }

    #[test]
    fn a_reconnection_tries_where_the_machine_was_last_found_before_the_saved_address() {
        let dir = tempfile::tempdir().unwrap();
        let target = Target::Native {
            address: "192.0.2.1:47130".into(),
            server: "ab".repeat(32),
        };
        let remote = Remote::with_key(&target, &dir.path().join("client.key")).unwrap();
        assert_eq!(remote.candidates(), ["192.0.2.1:47130"]);
        remote.found_at("10.0.0.9:47130");
        assert_eq!(remote.candidates(), ["10.0.0.9:47130", "192.0.2.1:47130"]);
        remote.clone().found_at("192.0.2.1:47130");
        assert_eq!(remote.candidates(), ["192.0.2.1:47130"]);
    }

    #[test]
    fn a_machine_moved_is_tried_where_it_moved_by_every_clone_first() {
        let dir = tempfile::tempdir().unwrap();
        let target = Target::Native {
            address: "192.0.2.1:47130".into(),
            server: "ab".repeat(32),
        };
        let remote = Remote::with_key(&target, &dir.path().join("client.key")).unwrap();
        let kept = remote.clone();
        remote.found_at("10.0.0.9:47130");
        remote.moved_to("box.example.com:47130");
        assert_eq!(kept.candidates(), ["box.example.com:47130"]);
        assert_eq!(kept.name(), "box.example.com:47130");
        // A connection begun before the move, reaching the old address,
        // leaves the address moved to kept.
        kept.found_at("192.0.2.1:47130");
        assert_eq!(
            remote.candidates(),
            ["192.0.2.1:47130", "box.example.com:47130"]
        );
    }

    fn discovered() -> RemoteInfo {
        let instance = "a".repeat(32);
        let endpoint = |generation: &str, protocols: Vec<u32>| tsumugi_protocol::RemoteEndpoint {
            instance: instance.clone(),
            generation: generation.into(),
            pid: 1,
            socket: format!("/tmp/tsumugi-r-501/{instance}/{generation}.sock").into(),
            build: "test".into(),
            protocols,
        };
        RemoteInfo {
            instance: instance.clone(),
            work: endpoint(&"b".repeat(32), vec![SPOKEN[0].0]),
            terminal: endpoint(&"c".repeat(32), tsumugi_term::SPEAKS.to_vec()),
        }
    }

    fn moved(endpoint: &mut tsumugi_protocol::RemoteEndpoint, generation: String) {
        endpoint.socket =
            format!("/tmp/tsumugi-r-501/{}/{generation}.sock", endpoint.instance).into();
        endpoint.generation = generation;
    }

    fn admitted(info: &RemoteInfo, expected: Option<&RemoteInfo>) -> Result<RemoteInfo, String> {
        admit(&serde_json::to_vec(info).unwrap(), expected)
    }

    #[test]
    fn hosts_the_window_speaks_with_are_admitted_on_a_first_connection() {
        let info = discovered();
        assert_eq!(admitted(&info, None), Ok(info));
    }

    #[test]
    fn hosts_that_speak_none_of_the_window_s_protocols_are_refused() {
        let mut work = discovered();
        work.work.protocols = vec![u32::MAX];
        let refused = admitted(&work, None).unwrap_err();
        assert!(refused.contains("protocols are incompatible"), "{refused}");
        let mut terminal = discovered();
        terminal.terminal.protocols = vec![u32::MAX];
        let refused = admitted(&terminal, None).unwrap_err();
        assert!(refused.contains("protocols are incompatible"), "{refused}");
    }

    #[test]
    fn a_reconnection_is_refused_once_the_terminal_host_was_replaced() {
        let seen = discovered();
        let mut replaced = seen.clone();
        moved(&mut replaced.terminal, "d".repeat(32));
        let refused = admitted(&replaced, Some(&seen)).unwrap_err();
        assert!(refused.contains("generation changed"), "{refused}");
    }

    #[test]
    fn a_reconnection_to_a_replaced_work_host_beside_the_same_terminal_host_is_admitted() {
        let seen = discovered();
        let mut replaced = seen.clone();
        moved(&mut replaced.work, "e".repeat(32));
        assert_eq!(admitted(&replaced, Some(&seen)), Ok(replaced));
    }

    #[test]
    fn the_client_key_is_kept_in_tsumugi_s_own_state_folder() {
        assert!(client_key().starts_with(tsumugi_model::paths::state_dir().join("tsumugi")));
    }
}
