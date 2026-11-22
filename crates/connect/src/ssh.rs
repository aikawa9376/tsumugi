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
    /// Whether to send this computer's tsumugi to the work machine when it
    /// has none, or one of another version.
    pub install: bool,
    pub this: Local,
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

/// What the work machine runs to serve its gateway on its loopback alone,
/// as the tsumugi `program` names.
pub fn enable_command(program: &str, remote: u16) -> String {
    format!("{program} remote enable --listen 127.0.0.1:{remote} --no-advertise")
}

/// What opens `path` of the work machine `host` in `editor`, for an editor
/// that opens ssh hosts itself: VS Code, VS Code Insiders and Cursor through
/// Remote-SSH, Zed by its ssh address. `editor` is the program, alone or by a
/// path; any other editor, and any word that is not one, gets none.
pub fn remote_editor(editor: &str, host: &str, path: Option<&str>) -> Option<Vec<String>> {
    if !valid_host(host) || editor.contains(char::is_whitespace) {
        return None;
    }
    let name = Path::new(editor).file_name()?.to_str()?;
    let name = name.strip_suffix(".cmd").unwrap_or(name);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    match name {
        "code" | "code-insiders" | "cursor" => {
            let mut words = vec![
                editor.to_string(),
                "--remote".into(),
                format!("ssh-remote+{host}"),
            ];
            words.extend(path.map(String::from));
            Some(words)
        }
        "zed" => Some(vec![
            editor.to_string(),
            format!(
                "ssh://{host}/{}",
                path.unwrap_or("").trim_start_matches('/')
            ),
        ]),
        _ => None,
    }
}

/// Where this computer's tsumugi is put on a work machine: a folder of the
/// user's own, which a command run over ssh may not have on its PATH.
const INSTALLED: &str = "~/.local/bin/tsumugi";

/// What the tsumugi of this computer is, for sending it to a work machine
/// that has none that matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    /// What `tsumugi --version` says here, which the one there must say too.
    pub version: String,
    /// The program that runs here.
    pub binary: std::path::PathBuf,
    /// The system it runs on, as `uname -sm` names it; none where it cannot
    /// be sent to another machine.
    pub system: Option<String>,
}

impl Local {
    /// This running tsumugi.
    pub fn this(version: String) -> Self {
        Self {
            version,
            binary: std::env::current_exe().unwrap_or_default(),
            system: system_of(std::env::consts::OS, std::env::consts::ARCH),
        }
    }
}

/// `os` and `arch`, as Rust names them, as `uname -sm` names the same
/// system; none for one a binary is not sent from.
pub fn system_of(os: &str, arch: &str) -> Option<String> {
    let name = match os {
        "linux" => "Linux",
        "macos" => "Darwin",
        _ => return None,
    };
    let arch = match (os, arch) {
        ("macos", "aarch64") => "arm64",
        (_, arch) => arch,
    };
    Some(format!("{name} {arch}"))
}

fn listening(port: u16) -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// What the work machine has of tsumugi.
enum Remote {
    /// A tsumugi of this version, started as `program`.
    Matches(&'static str),
    /// One that says something else.
    Differs(String),
    Missing,
}

impl Remote {
    fn refusal(&self, host: &str, here: &str) -> String {
        match self {
            Remote::Differs(there) => format!(
                "{host} has {there}, which is not this computer's {here}; connect with --install to send it this one, or install the same version there"
            ),
            _ => format!(
                "tsumugi is not installed on {host}, or not on the PATH of a command run over ssh; connect with --install to send it this computer's, or install it there"
            ),
        }
    }
}

/// The first line `program --version` says on the work machine, when it
/// runs there.
fn version_there(
    ssh: &str,
    control: &Path,
    host: &str,
    program: &str,
) -> Result<Option<String>, String> {
    Ok(
        captured(ssh, control, host, &format!("{program} --version"))?
            .and_then(|out| out.lines().next().map(|line| line.trim().to_string()))
            .filter(|line| !line.is_empty()),
    )
}

/// What a command prints on the work machine when it succeeds there.
fn captured(
    ssh: &str,
    control: &Path,
    host: &str,
    command: &str,
) -> Result<Option<String>, String> {
    let output = Command::new(ssh)
        .args(command_args(control, host, command))
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run `{ssh}`: {e}"))?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
}

/// Which tsumugi of the work machine is of the version asked for, looking on
/// its PATH and where an install puts it.
fn remote_program(ssh: &str, control: &Path, host: &str, version: &str) -> Result<Remote, String> {
    let mut other = None;
    for program in ["tsumugi", INSTALLED] {
        match version_there(ssh, control, host, program)? {
            Some(there) if there == version => return Ok(Remote::Matches(program)),
            Some(there) => other = other.or(Some(there)),
            None => {}
        }
    }
    Ok(other.map_or(Remote::Missing, Remote::Differs))
}

/// Sends this computer's tsumugi to the work machine, where the system is
/// the same, and checks that it runs there.
fn install(ask: &Ask, ssh: &str, control: &Path) -> Result<(), String> {
    let host = &ask.host;
    let Some(here) = ask.this.system.as_deref() else {
        return Err(
            "this computer's tsumugi cannot be sent to another machine; install it there".into(),
        );
    };
    let there = captured(ssh, control, host, "uname -sm")?
        .map(|out| out.trim().to_string())
        .ok_or_else(|| format!("cannot tell which system {host} runs"))?;
    if there != here {
        return Err(format!(
            "{host} runs {there}, but this computer's tsumugi is for {here}, which does not run there; install the one for {there} on {host}"
        ));
    }
    let binary = std::fs::File::open(&ask.this.binary)
        .map_err(|e| format!("cannot read {}: {e}", ask.this.binary.display()))?;
    let command = format!(
        "mkdir -p ~/.local/bin && cat > {INSTALLED}.part && chmod 755 {INSTALLED}.part && mv {INSTALLED}.part {INSTALLED}"
    );
    let status = Command::new(ssh)
        .args(command_args(control, host, &command))
        .stdin(binary)
        .status()
        .map_err(|e| format!("cannot run `{ssh}`: {e}"))?;
    if !status.success() {
        return Err(format!(
            "could not install tsumugi on {host} ({status}); is ~/.local/bin writable there?"
        ));
    }
    match version_there(ssh, control, host, INSTALLED)? {
        Some(said) if said == ask.this.version => Ok(()),
        said => Err(format!(
            "tsumugi was sent to {host}, but runs there as {}, not {}",
            said.as_deref().unwrap_or("nothing"),
            ask.this.version
        )),
    }
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
    let program = match remote_program(ssh, control, host, &ask.this.version)? {
        Remote::Matches(program) => program,
        remote if !ask.install => return Err(remote.refusal(host, &ask.this.version)),
        _ => {
            show(&format!("Installing tsumugi on {host} with ssh"));
            install(ask, ssh, control)?;
            INSTALLED
        }
    };
    if ask.start {
        show(&format!(
            "Starting the work machine's hosts and gateway on {host}"
        ));
        let status = Command::new(ssh)
            .args(command_args(
                control,
                host,
                &enable_command(program, ask.remote_port),
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
            enable_command(program, ask.remote_port)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_editor_is_pointed_at_a_host_the_way_it_opens_ssh_hosts_and_no_other_is_run() {
        let at = |editor, host, path| remote_editor(editor, host, path);
        let words = |words: &[&str]| Some(words.iter().map(|w| w.to_string()).collect::<Vec<_>>());
        assert_eq!(
            at("code", "ai-box", Some("/w/a")),
            words(&["code", "--remote", "ssh-remote+ai-box", "/w/a"])
        );
        assert_eq!(
            at("/usr/bin/cursor", "me@ai-box", Some("/w")),
            words(&["/usr/bin/cursor", "--remote", "ssh-remote+me@ai-box", "/w"])
        );
        assert_eq!(
            at("code-insiders", "ai-box", None),
            words(&["code-insiders", "--remote", "ssh-remote+ai-box"])
        );
        assert_eq!(
            at("zed", "me@ai-box", Some("/w/a")),
            words(&["zed", "ssh://me@ai-box/w/a"])
        );
        assert_eq!(at("zed", "ai-box", None), words(&["zed", "ssh://ai-box/"]));
        // Another editor, a command with arguments and a host ssh could take
        // for an option are not run.
        assert_eq!(at("vim", "ai-box", None), None);
        assert_eq!(at("code --wait", "ai-box", None), None);
        assert_eq!(at("code", "-oProxyCommand=x", None), None);
    }

    #[test]
    fn a_system_is_named_as_uname_names_it_and_only_those_a_binary_is_sent_from() {
        assert_eq!(
            system_of("linux", "x86_64").as_deref(),
            Some("Linux x86_64")
        );
        assert_eq!(
            system_of("linux", "aarch64").as_deref(),
            Some("Linux aarch64")
        );
        assert_eq!(
            system_of("macos", "aarch64").as_deref(),
            Some("Darwin arm64")
        );
        assert_eq!(
            system_of("macos", "x86_64").as_deref(),
            Some("Darwin x86_64")
        );
        assert_eq!(system_of("windows", "x86_64"), None);
    }

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
        let args = command_args(Path::new("/c"), "ai-box", "tsumugi --version");
        assert_eq!(
            &args[args.len() - 3..],
            ["--", "ai-box", "tsumugi --version"]
        );
        assert_eq!(
            enable_command("tsumugi", 47130),
            "tsumugi remote enable --listen 127.0.0.1:47130 --no-advertise"
        );
    }

    #[cfg(unix)]
    mod running {
        use super::*;
        use std::cell::RefCell;
        use std::os::unix::fs::PermissionsExt;

        /// The version line this computer's tsumugi says, and the system it
        /// runs on, as the fakes below know them.
        /// A fake is a script written and then run, which another test's
        /// child process holding the file open as it is written makes fail
        /// with "text file busy"; so the tests that run one take turns.
        fn serial() -> std::sync::MutexGuard<'static, ()> {
            static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
            TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        const HERE: &str = "tsumugi 1.2.3";
        const SYSTEM: &str = "Linux x86_64";

        /// A stand-in for ssh that records what it was asked and answers the
        /// commands it is given: a tunnel with `tunnel`, `tsumugi --version`
        /// with `version` (none when no tsumugi is there, until one is
        /// uploaded, which then says `HERE`), `uname` with `system`, an upload
        /// with `upload`, which keeps what it is given in `uploaded`, and
        /// `remote enable` with `start`.
        struct Fake {
            tunnel: u8,
            version: Option<&'static str>,
            system: &'static str,
            upload: u8,
            start: u8,
        }

        impl Fake {
            fn new() -> Self {
                Self {
                    tunnel: 0,
                    version: Some(HERE),
                    system: SYSTEM,
                    upload: 0,
                    start: 0,
                }
            }

            fn at(&self, dir: &Path) -> std::path::PathBuf {
                let path = dir.join("ssh");
                let answer = match self.version {
                    Some(version) => format!("echo '{version}'; exit 0"),
                    None => "exit 127".to_string(),
                };
                let script = format!(
                    "#!/bin/sh\necho \"$*\" >> \"{log}\"\ncase \"$*\" in\n  *\"-L \"*) exit {tunnel};;\n  *\"cat >\"*) cat > \"{uploaded}\"; exit {upload};;\n  *\"--version\"*) if [ -f \"{uploaded}\" ]; then echo '{HERE}'; exit 0; fi; {answer};;\n  *uname*) echo '{system}'; exit 0;;\n  *\"remote enable\"*) exit {start};;\nesac\nexit 0\n",
                    log = dir.join("log").display(),
                    uploaded = dir.join("uploaded").display(),
                    tunnel = self.tunnel,
                    upload = self.upload,
                    system = self.system,
                    start = self.start,
                );
                std::fs::write(&path, script).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
                path
            }
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
                install: false,
                this: Local {
                    version: HERE.into(),
                    binary: std::path::PathBuf::from("/nonexistent/tsumugi"),
                    system: Some(SYSTEM.into()),
                },
            }
        }

        /// `asking`, to install, with a binary of `bytes` to send.
        fn installing(host: &str, local: u16, dir: &Path, bytes: &str) -> Ask {
            let binary = dir.join("this-tsumugi");
            std::fs::write(&binary, bytes).unwrap();
            let mut ask = asking(host, Some(local), false);
            ask.install = true;
            ask.this.binary = binary;
            ask
        }

        fn answering() -> (std::net::TcpListener, u16) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            (listener, port)
        }

        #[test]
        fn a_tunnel_that_ssh_cannot_open_fails_saying_so_and_pairs_nothing() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let ssh = Fake {
                tunnel: 255,
                ..Fake::new()
            }
            .at(dir.path());
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
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let ssh = Fake {
                version: None,
                ..Fake::new()
            }
            .at(dir.path());
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
            assert!(error.contains("--install"), "{error}");
            assert!(
                !log(dir.path()).contains("cat >"),
                "nothing is sent unasked"
            );
        }

        #[test]
        fn a_tunnel_already_answering_is_used_and_the_gateway_is_paired_at_its_end() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let ssh = Fake {
                tunnel: 255,
                ..Fake::new()
            }
            .at(dir.path());
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
            assert!(log.contains("tsumugi --version"), "{log}");
            assert!(!log.contains("remote enable"), "{log}");
        }

        #[test]
        fn starting_the_gateway_runs_enable_on_the_loopback_there_and_a_failure_is_reported() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake::new().at(dir.path());
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
            let ssh = Fake {
                start: 3,
                ..Fake::new()
            }
            .at(broken.path());
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
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake::new().at(dir.path());
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
        fn a_tsumugi_of_another_version_is_told_apart_from_none_and_not_replaced_unasked() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake {
                version: Some("tsumugi 0.9.0"),
                ..Fake::new()
            }
            .at(dir.path());
            let error = run(
                &asking("ai-box", Some(port), false),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap_err();
            assert!(error.contains("tsumugi 0.9.0"), "{error}");
            assert!(error.contains(HERE), "{error}");
            assert!(error.contains("--install"), "{error}");
            assert!(!log(dir.path()).contains("cat >"), "{}", log(dir.path()));
        }

        #[test]
        fn installing_sends_this_computer_s_tsumugi_where_the_system_matches_and_uses_it() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake {
                version: None,
                ..Fake::new()
            }
            .at(dir.path());
            let mut ask = installing("ai-box", port, dir.path(), "the binary");
            ask.start = true;
            let told = RefCell::new(Vec::new());
            run(
                &ask,
                ssh.to_str().unwrap(),
                dir.path(),
                &|text| told.borrow_mut().push(text.to_string()),
                |_, _| Ok(String::new()),
            )
            .unwrap();
            assert_eq!(
                std::fs::read_to_string(dir.path().join("uploaded")).unwrap(),
                "the binary"
            );
            let log = log(dir.path());
            assert!(log.contains("~/.local/bin/tsumugi"), "{log}");
            // What is served there is the copy just sent, which may not be on PATH.
            assert!(
                log.contains("~/.local/bin/tsumugi remote enable --listen 127.0.0.1:47130"),
                "{log}"
            );
            assert!(
                told.borrow().iter().any(|text| text.contains("Installing")),
                "{:?}",
                told
            );
        }

        #[test]
        fn a_tsumugi_of_another_version_is_replaced_when_asked_to_install() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake {
                version: Some("tsumugi 0.9.0"),
                ..Fake::new()
            }
            .at(dir.path());
            run(
                &installing("ai-box", port, dir.path(), "new"),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap();
            assert!(dir.path().join("uploaded").exists());
        }

        #[test]
        fn a_matching_tsumugi_is_left_alone_even_when_asked_to_install() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake::new().at(dir.path());
            run(
                &installing("ai-box", port, dir.path(), "new"),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap();
            assert!(!dir.path().join("uploaded").exists());
        }

        #[test]
        fn a_work_machine_of_another_system_is_not_sent_a_binary_that_cannot_run_there() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake {
                version: None,
                system: "Darwin arm64",
                ..Fake::new()
            }
            .at(dir.path());
            let error = run(
                &installing("ai-box", port, dir.path(), "x"),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| Ok(String::new()),
            )
            .unwrap_err();
            assert!(error.contains("Darwin arm64"), "{error}");
            assert!(error.contains(SYSTEM), "{error}");
            assert!(!dir.path().join("uploaded").exists());
        }

        #[test]
        fn a_failed_upload_is_reported_and_pairs_nothing() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let (_listener, port) = answering();
            let ssh = Fake {
                version: None,
                upload: 1,
                ..Fake::new()
            }
            .at(dir.path());
            let error = run(
                &installing("ai-box", port, dir.path(), "x"),
                ssh.to_str().unwrap(),
                dir.path(),
                &|_| {},
                |_, _| panic!("nothing is paired"),
            )
            .unwrap_err();
            assert!(error.contains("could not install"), "{error}");
        }

        #[test]
        fn a_destination_ssh_could_take_for_an_option_is_refused_before_ssh_runs() {
            let _turn = serial();
            let dir = tempfile::tempdir().unwrap();
            let ssh = Fake::new().at(dir.path());
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
