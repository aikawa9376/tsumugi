//! Reaching a work machine's gateway through `ssh -L`, so that the gateway
//! need not listen beyond the work machine's loopback or announce itself.
//! The tunnel only carries bytes: the work machine is still paired and known
//! by its key, as over any other network.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The port a gateway listens on unless told otherwise.
pub const GATEWAY_PORT: u16 = 47130;

/// Where on this computer a tunnel to a host ends unless told otherwise:
/// the same port for the same host, so the environment paired through it
/// keeps its address from one connection to the next.
const FIRST_PORT: u16 = 47200;
const PORTS: u16 = 800;

/// How long a tunnel has to start answering once ssh has set it up.
const COMES_UP: Duration = Duration::from_secs(5);

/// What `tsumugi connect ssh` was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// What `ssh` takes for a destination: a name from `~/.ssh/config`, a
    /// host name or `user@host`.
    pub host: String,
    /// The port on this computer the tunnel ends in; one chosen by the host
    /// when left out.
    pub local_port: Option<u16>,
    /// The port the gateway listens on, on the work machine's loopback.
    pub remote_port: u16,
    /// Whether to have the work machine run its hosts and gateway as
    /// services, listening on its loopback only, before pairing.
    pub start: bool,
    /// How this client calls itself to the operator of the work machine.
    pub name: Option<String>,
}

/// Whether `ssh` would take `host` as a destination and nothing else: not an
/// option, and no word that could be taken for a command.
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-@:[]%".contains(&c))
}

/// The port on this computer `host`'s tunnel ends in.
pub fn local_port_for(host: &str) -> u16 {
    // FNV-1a: stable across runs and machines, unlike a randomly seeded hash.
    let hash = host.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    FIRST_PORT + (hash % u64::from(PORTS)) as u16
}

/// The options every `ssh` of one connection shares, so that the first sets
/// up a connection the others reuse and a passphrase is asked once.
fn shared(control: &Path) -> Vec<String> {
    vec![
        "-o".into(),
        "ControlMaster=auto".into(),
        "-o".into(),
        format!("ControlPath={}/%C", control.display()),
        "-o".into(),
        "ControlPersist=10m".into(),
    ]
}

/// What sets up the tunnel to `host` and leaves it running in the
/// background once it is authenticated.
pub fn tunnel_args(control: &Path, host: &str, local: u16, remote: u16) -> Vec<String> {
    let mut args = shared(control);
    args.extend([
        "-f".into(),
        "-N".into(),
        // A tunnel that cannot end where asked is no tunnel.
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        // A connection that is lost ends, so none is left looking alive.
        "-o".into(),
        "ServerAliveInterval=15".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
        "-L".into(),
        format!("127.0.0.1:{local}:127.0.0.1:{remote}"),
        "--".into(),
        host.into(),
    ]);
    args
}

/// What runs `command` on `host`.
pub fn command_args(control: &Path, host: &str, command: &str) -> Vec<String> {
    let mut args = shared(control);
    args.extend(["--".into(), host.into(), command.into()]);
    args
}

/// What the work machine runs to serve its gateway on its loopback alone.
pub fn enable_command(remote: u16) -> String {
    format!("tsumugi remote enable --listen 127.0.0.1:{remote} --no-advertise")
}

fn listening(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// Connects to the work machine `ask` names through `ssh`, the program, with
/// its shared connection kept in `control`, and has `pair` pair with the
/// gateway at the address the tunnel gives. `show` tells how far it has come.
pub fn run(
    ask: &Ask,
    ssh: &str,
    control: &Path,
    show: &dyn Fn(&str),
    pair: impl FnOnce(&str, &str) -> Result<String, String>,
) -> Result<String, String> {
    if !valid_host(&ask.host) {
        return Err(format!(
            "`{}` is not a destination ssh can take: give a name from ~/.ssh/config, a host name or user@host",
            ask.host
        ));
    }
    let host = &ask.host;
    let local = ask.local_port.unwrap_or_else(|| local_port_for(host));
    std::fs::create_dir_all(control).map_err(|e| format!("cannot keep ssh's connection: {e}"))?;
    if listening(local) {
        show(&format!(
            "Port {local} on this computer answers already; using it as the tunnel to {host}"
        ));
    } else {
        show(&format!("Opening a tunnel to {host} with ssh"));
        // ssh keeps the terminal, to ask the host key, the passphrase and
        // the password.
        let status = Command::new(ssh)
            .args(tunnel_args(control, host, local, ask.remote_port))
            .status()
            .map_err(|e| format!("cannot run `{ssh}`: {e}; is OpenSSH installed?"))?;
        if !status.success() {
            return Err(format!(
                "ssh to {host} failed ({status}), so nothing was paired; check that `ssh {host}` works by itself"
            ));
        }
        let until = Instant::now() + COMES_UP;
        while !listening(local) {
            if Instant::now() > until {
                return Err(format!(
                    "ssh to {host} ran, but nothing answers on 127.0.0.1:{local}; is the port in use by another tunnel?"
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let found = Command::new(ssh)
        .args(command_args(control, host, "command -v tsumugi"))
        .stdout(Stdio::null())
        .status()
        .map_err(|e| format!("cannot run `{ssh}`: {e}"))?
        .success();
    if !found {
        return Err(format!(
            "tsumugi is not installed on {host}, or not on the PATH of a command run over ssh; install it there, then connect again"
        ));
    }
    if ask.start {
        show(&format!(
            "Starting the work machine's hosts and gateway on {host}"
        ));
        let status = Command::new(ssh)
            .args(command_args(
                control,
                host,
                &enable_command(ask.remote_port),
            ))
            .status()
            .map_err(|e| format!("cannot run `{ssh}`: {e}"))?;
        if !status.success() {
            return Err(format!(
                "`tsumugi remote enable` failed on {host} ({status}); its message is above"
            ));
        }
    }
    let address = format!("127.0.0.1:{local}");
    pair(&address, &format!("{host} (ssh)")).map_err(|error| {
        format!(
            "{error}; the gateway must listen on 127.0.0.1:{} there: run `{}` on {host}, or connect with --start",
            ask.remote_port,
            enable_command(ask.remote_port)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_destination_is_a_name_that_cannot_be_taken_for_an_option_or_a_command() {
        for good in ["ai-box", "me@ai-box.local", "10.0.0.2", "[::1]", "box_1"] {
            assert!(valid_host(good), "{good}");
        }
        for bad in [
            "",
            "-oProxyCommand=sh",
            "-J",
            "a b",
            "a;b",
            "a$(b)",
            "a\nb",
            "a'b",
        ] {
            assert!(!valid_host(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_host_s_tunnel_ends_in_the_same_port_every_time_within_the_range_kept_for_it() {
        assert_eq!(local_port_for("ai-box"), local_port_for("ai-box"));
        assert_ne!(local_port_for("ai-box"), local_port_for("other-box"));
        for host in ["a", "ai-box", "me@ai-box.local", "10.0.0.2"] {
            let port = local_port_for(host);
            assert!((FIRST_PORT..FIRST_PORT + PORTS).contains(&port), "{port}");
        }
    }

    #[test]
    fn the_tunnel_ends_on_this_computer_s_loopback_alone_and_ends_when_it_cannot_or_the_connection_is_lost()
     {
        let args = tunnel_args(Path::new("/c"), "ai-box", 47201, 47130);
        let at = |flag: &str| args.iter().position(|a| a == flag).unwrap();
        assert_eq!(args[at("-L") + 1], "127.0.0.1:47201:127.0.0.1:47130");
        assert!(args.iter().any(|a| a == "ExitOnForwardFailure=yes"));
        assert!(args.iter().any(|a| a == "ServerAliveInterval=15"));
        assert!(args.contains(&"-N".to_string()) && args.contains(&"-f".to_string()));
        // The destination follows `--`, so no host name is an option.
        assert_eq!(&args[args.len() - 2..], ["--", "ai-box"]);
        assert!(args.iter().any(|a| a == "ControlPath=/c/%C"));
    }

    #[test]
    fn a_command_over_ssh_follows_the_destination_after_the_end_of_options() {
        let args = command_args(Path::new("/c"), "ai-box", "command -v tsumugi");
        assert_eq!(
            &args[args.len() - 3..],
            ["--", "ai-box", "command -v tsumugi"]
        );
        assert_eq!(
            enable_command(47130),
            "tsumugi remote enable --listen 127.0.0.1:47130 --no-advertise"
        );
    }

    #[cfg(unix)]
    mod running {
        use super::*;
        use std::cell::RefCell;
        use std::os::unix::fs::PermissionsExt;

        /// A stand-in for ssh that records what it was asked and answers the
        /// commands it is given with `check` and `start`, and a tunnel with
        /// `tunnel`.
        fn fake(dir: &Path, tunnel: u8, check: u8, start: u8) -> std::path::PathBuf {
            let path = dir.join("ssh");
            let script = format!(
                "#!/bin/sh\necho \"$*\" >> \"{log}\"\ncase \"$*\" in\n  *\"-L \"*) exit {tunnel};;\n  *\"command -v\"*) exit {check};;\n  *\"remote enable\"*) exit {start};;\nesac\nexit 0\n",
                log = dir.join("log").display()
            );
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        fn log(dir: &Path) -> String {
            std::fs::read_to_string(dir.join("log")).unwrap_or_default()
        }

        fn asking(host: &str, local: Option<u16>, start: bool) -> Ask {
            Ask {
                host: host.into(),
                local_port: local,
                remote_port: GATEWAY_PORT,
                start,
                name: None,
            }
        }

        fn answering() -> (std::net::TcpListener, u16) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            (listener, port)
        }

        #[test]
        fn a_tunnel_that_ssh_cannot_open_fails_saying_so_and_pairs_nothing() {
            let dir = tempfile::tempdir().unwrap();
            let ssh = fake(dir.path(), 255, 0, 0);
            let (listener, port) = answering();
            drop(listener);
            let paired = RefCell::new(false);
            let error = run(
                &asking("ai-box", Some(port), false),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| {
                    *paired.borrow_mut() = true;
                    Ok(String::new())
                },
            )
            .unwrap_err();
            assert!(error.contains("ssh to ai-box failed"), "{error}");
            assert!(!*paired.borrow());
        }

        #[test]
        fn a_host_without_tsumugi_is_told_apart_from_a_host_ssh_cannot_reach() {
            let dir = tempfile::tempdir().unwrap();
            let ssh = fake(dir.path(), 0, 1, 0);
            let (_listener, port) = answering();
            let error = run(
                &asking("ai-box", Some(port), false),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap_err();
            assert!(
                error.contains("tsumugi is not installed on ai-box"),
                "{error}"
            );
        }

        #[test]
        fn a_tunnel_already_answering_is_used_and_the_gateway_is_paired_at_its_end() {
            let dir = tempfile::tempdir().unwrap();
            let ssh = fake(dir.path(), 255, 0, 0);
            let (_listener, port) = answering();
            let told = RefCell::new(Vec::new());
            let done = run(
                &asking("me@ai-box", Some(port), false),
                ssh.to_str().unwrap(),
                dir.path(),
                &|text| told.borrow_mut().push(text.to_string()),
                |address, environment| Ok(format!("paired {address} as {environment}")),
            )
            .unwrap();
            assert_eq!(done, format!("paired 127.0.0.1:{port} as me@ai-box (ssh)"));
            assert!(told.borrow()[0].contains("answers already"), "{:?}", told);
            let log = log(dir.path());
            assert!(!log.contains("-L "), "no second tunnel: {log}");
            assert!(log.contains("command -v tsumugi"), "{log}");
            assert!(!log.contains("remote enable"), "{log}");
        }

        #[test]
        fn starting_the_gateway_runs_enable_on_the_loopback_there_and_a_failure_is_reported() {
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = fake(dir.path(), 0, 0, 0);
            run(
                &asking("ai-box", Some(port), true),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap();
            assert!(
                log(dir.path())
                    .contains("tsumugi remote enable --listen 127.0.0.1:47130 --no-advertise"),
                "{}",
                log(dir.path())
            );
            let broken = tempfile::tempdir().unwrap();
            let ssh = fake(broken.path(), 0, 0, 3);
            let error = run(
                &asking("ai-box", Some(port), true),
                ssh.to_str().unwrap(),
                broken.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap_err();
            assert!(error.contains("`tsumugi remote enable` failed"), "{error}");
        }

        #[test]
        fn a_gateway_that_does_not_answer_through_the_tunnel_says_how_to_serve_it_there() {
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = fake(dir.path(), 0, 0, 0);
            let error = run(
                &asking("ai-box", Some(port), false),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Err("cannot reach 127.0.0.1: closed".into()),
            )
            .unwrap_err();
            assert!(error.contains("closed"), "{error}");
            assert!(
                error.contains("tsumugi remote enable --listen 127.0.0.1:47130"),
                "{error}"
            );
            assert!(error.contains("--start"), "{error}");
        }

        #[test]
        fn a_destination_ssh_could_take_for_an_option_is_refused_before_ssh_runs() {
            let dir = tempfile::tempdir().unwrap();
            let ssh = fake(dir.path(), 0, 0, 0);
            let error = run(
                &asking("-oProxyCommand=x", None, false),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap_err();
            assert!(error.contains("not a destination"), "{error}");
            assert_eq!(log(dir.path()), "");
        }
    }
}
