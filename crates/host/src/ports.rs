//! The TCP ports a session's processes listen on.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use serde::Serialize;

use tsumugi_model::{Port, Process, Session};

use crate::processes::Sampler;

/// A socket listening, and the process that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listener {
    pub pid: u32,
    pub bind: String,
    pub port: u16,
}

/// The listening sockets of `/proc/net/tcp` or `/proc/net/tcp6`, by their
/// inode, with the address and port each listens on.
#[cfg(any(test, target_os = "linux"))]
pub(crate) fn parse_proc_net_tcp(text: &str) -> Vec<(u64, String, u16)> {
    const LISTEN: &str = "0A";
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.get(3) != Some(&LISTEN) {
                return None;
            }
            let (address, port) = fields.get(1)?.split_once(':')?;
            let port = u16::from_str_radix(port, 16).ok()?;
            let inode = fields.get(9)?.parse().ok()?;
            Some((inode, proc_address(address)?.to_string(), port))
        })
        .collect()
}

/// An address as the kernel writes it: each 32-bit word in the machine's
/// byte order, in hexadecimal.
#[cfg(any(test, target_os = "linux"))]
fn proc_address(hex: &str) -> Option<IpAddr> {
    let words: Vec<u32> = (0..hex.len())
        .step_by(8)
        .map(|at| u32::from_str_radix(hex.get(at..at + 8)?, 16).ok())
        .collect::<Option<_>>()?;
    match words.as_slice() {
        [word] => Some(IpAddr::from(word.swap_bytes().to_be_bytes())),
        [a, b, c, d] => {
            let mut bytes = [0u8; 16];
            for (i, word) in [a, b, c, d].into_iter().enumerate() {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&word.swap_bytes().to_be_bytes());
            }
            Some(IpAddr::from(bytes))
        }
        _ => None,
    }
}

/// The listeners `lsof -F pn` lists.
#[cfg(any(test, all(unix, not(target_os = "linux"))))]
pub(crate) fn parse_lsof(text: &str) -> Vec<Listener> {
    let mut pid = None;
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(name), Some(pid)) = (line.strip_prefix('n'), pid) {
            let Some((bind, port)) = name.rsplit_once(':') else {
                continue;
            };
            let Ok(port) = port.parse() else { continue };
            let bind = bind.trim_start_matches('[').trim_end_matches(']');
            out.push(Listener {
                pid,
                bind: bind.into(),
                port,
            });
        }
    }
    out
}

/// The host a client reaches a listener on `bind` by.
pub(crate) fn connect_host(bind: &str) -> String {
    match bind.parse::<IpAddr>() {
        Ok(ip) if ip.is_unspecified() => "localhost".into(),
        _ if bind == "*" => "localhost".into(),
        _ => bind.into(),
    }
}

/// The ports of `processes` that `listeners` hold: one for each process and
/// port, whichever addresses it listens on, by port.
pub(crate) fn attribute(listeners: &[Listener], processes: &[Process]) -> Vec<Port> {
    let mut ports: Vec<Port> = Vec::new();
    for listener in listeners {
        let Some(process) = processes.iter().find(|p| p.pid == listener.pid) else {
            continue;
        };
        if ports
            .iter()
            .any(|p| p.port == listener.port && p.process.pid == listener.pid)
        {
            continue;
        }
        ports.push(Port {
            key: format!("\0port:{}:{}", listener.port, process.key),
            port: listener.port,
            bind: listener.bind.clone(),
            host: connect_host(&listener.bind),
            process: process.clone(),
        });
    }
    ports.sort_by_key(|p| (p.port, p.process.pid));
    ports
}

/// What `tsumugi ports` lists of one session: the ports it listens on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    pub code: Option<String>,
    pub name: String,
    pub dir: PathBuf,
    pub ports: Vec<Port>,
}

/// The sessions among `sessions` that work in the worktree `top` names for
/// `dir`, or all of them. A nested worktree is another worktree: its
/// sessions, and the ports of its processes, are not this one's.
pub(crate) fn select<'a>(
    sessions: &'a [Session],
    dir: Option<&Path>,
    top: impl Fn(&Path) -> PathBuf,
) -> Vec<&'a Session> {
    let wanted = dir.map(&top);
    sessions
        .iter()
        .filter(|s| wanted.as_ref().is_none_or(|w| top(&s.cwd) == *w))
        .collect()
}

/// The sessions `select` chooses, each with the ports its processes listen
/// on, those that listen on none left out.
pub async fn listed(dir: Option<PathBuf>) -> Result<Vec<Listing>, String> {
    let live = crate::daemon::daemon()
        .sessions()
        .await
        .map_err(|e| format!("cannot reach the daemon: {e}"))?;
    let agents: Vec<Session> = live
        .iter()
        .filter_map(|s| match crate::sessions::read(s) {
            Some(crate::sessions::Kept::Agent(agent)) => Some(agent),
            _ => None,
        })
        .collect();
    let mut tops = std::collections::HashMap::new();
    for path in agents.iter().map(|s| s.cwd.clone()).chain(dir.clone()) {
        let top = match tsumugi_vcs::inspect(&path).await {
            Some(worktree) => worktree.path,
            None => path.clone(),
        };
        tops.insert(path, top);
    }
    let chosen: Vec<Session> = select(&agents, dir.as_deref(), |p| {
        tops.get(p).cloned().unwrap_or_else(|| p.to_path_buf())
    })
    .into_iter()
    .cloned()
    .collect();
    crate::environment::spawn_blocking(move || {
        let mut sampler = Sampler::default();
        chosen
            .into_iter()
            .map(|session| Listing {
                code: session.code.as_ref().map(ToString::to_string),
                name: session.name.clone(),
                dir: session.cwd.clone(),
                ports: sampler.sample_ports(&session),
            })
            .filter(|l| !l.ports.is_empty())
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// `tsumugi ports`'s options: the directory whose worktree to list, none
/// for every session's.
pub(crate) fn parse_args(args: &[String]) -> Option<Option<PathBuf>> {
    match args {
        [] => Some(Some(PathBuf::from("."))),
        [all] if all == "--all" => Some(None),
        [worktree, dir] if worktree == "--worktree" => Some(Some(PathBuf::from(dir))),
        _ => None,
    }
}

pub const USAGE: &str = "usage: tsumugi ports [--worktree <dir> | --all]";

/// `listings` as lines a person reads.
pub(crate) fn text(listings: &[Listing]) -> String {
    if listings.is_empty() {
        return "no listening ports".into();
    }
    listings
        .iter()
        .flat_map(|l| {
            let name = match &l.code {
                Some(code) => format!("{} ({code})", l.name),
                None => l.name.clone(),
            };
            std::iter::once(format!("{name}  {}", l.dir.display())).chain(
                l.ports
                    .iter()
                    .map(|p| format!("  {}  {} ({})", p.address(), p.process.name, p.process.pid)),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `tsumugi ports <args>` as the work host runs it for a command run in
/// `cwd`.
pub(crate) async fn run(args: &[String], cwd: &Path) -> crate::command::Output {
    let fail = |text: String, code| crate::command::Output {
        wrote: Err(text),
        code,
        effects_possible: false,
    };
    let Some(scope) = parse_args(args) else {
        return fail(USAGE.into(), 2);
    };
    let dir = scope.map(|dir| cwd.join(dir));
    match listed(dir).await {
        Ok(listings) => crate::command::Output {
            wrote: Ok(text(&listings)),
            code: 0,
            effects_possible: false,
        },
        Err(text) => fail(text, 1),
    }
}

/// `tsumugi ports`, run in the work host of the instance.
pub fn cli(args: &[String]) -> i32 {
    if parse_args(args).is_none() {
        eprintln!("{USAGE}");
        return 2;
    }
    let mut ask = vec!["ports".to_string()];
    ask.extend(args.iter().cloned());
    crate::work_host::run_command(ask, None)
}

/// The listeners among the processes `pids`, as this system lists them.
#[cfg(target_os = "linux")]
fn listeners(pids: &[u32]) -> Vec<Listener> {
    let sockets: Vec<(u64, String, u16)> = ["/proc/net/tcp", "/proc/net/tcp6"]
        .into_iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| parse_proc_net_tcp(&text))
        .collect();
    let mut out = Vec::new();
    for &pid in pids {
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            let Ok(target) = std::fs::read_link(fd.path()) else {
                continue;
            };
            let Some(inode) = target
                .to_str()
                .and_then(|t| t.strip_prefix("socket:["))
                .and_then(|t| t.strip_suffix(']'))
                .and_then(|t| t.parse::<u64>().ok())
            else {
                continue;
            };
            if let Some((_, bind, port)) = sockets.iter().find(|(i, ..)| *i == inode) {
                out.push(Listener {
                    pid,
                    bind: bind.clone(),
                    port: *port,
                });
            }
        }
    }
    out
}

#[cfg(all(unix, not(target_os = "linux")))]
fn listeners(pids: &[u32]) -> Vec<Listener> {
    if pids.is_empty() {
        return Vec::new();
    }
    let list = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    std::process::Command::new("lsof")
        .args(["-nP", "-a", "-p", &list, "-iTCP", "-sTCP:LISTEN", "-Fpn"])
        .output()
        .map(|out| parse_lsof(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

#[cfg(not(unix))]
fn listeners(_pids: &[u32]) -> Vec<Listener> {
    Vec::new()
}

impl Sampler {
    /// The ports the processes of `session` listen on.
    pub fn sample_ports(&mut self, session: &Session) -> Vec<Port> {
        let processes = self.sample(session);
        let pids: Vec<u32> = processes.iter().map(|p| p.pid).collect();
        attribute(&listeners(&pids), &processes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(pid: u32, bind: &str, port: u16) -> Listener {
        Listener {
            pid,
            bind: bind.into(),
            port,
        }
    }

    fn process(pid: u32) -> Process {
        Process {
            key: format!("\0process:{pid}:1"),
            pid,
            started_secs: 1,
            ..Process::default()
        }
    }

    #[test]
    fn proc_net_tcp_lists_only_listening_sockets_with_their_addresses() {
        let v4 = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 00000000:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 11111 1 0000000000000000 100 0 0 10 0\n\
   1: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 100 0 0 10 0\n\
   2: 0100007F:D431 0100007F:0BB8 01 00000000:00000000 00:00000000 00000000  1000        0 33333 1 0000000000000000 100 0 0 10 0\n";
        assert_eq!(
            parse_proc_net_tcp(v4),
            [
                (11111, "0.0.0.0".to_string(), 3000),
                (22222, "127.0.0.1".to_string(), 8080),
            ]
        );
        let v6 = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 00000000000000000000000000000000:1F90 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 44444 1 0000000000000000 100 0 0 10 0\n\
   1: 00000000000000000000000001000000:0BB9 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 55555 1 0000000000000000 100 0 0 10 0\n";
        assert_eq!(
            parse_proc_net_tcp(v6),
            [
                (44444, "::".to_string(), 8080),
                (55555, "::1".to_string(), 3001),
            ]
        );
        assert!(parse_proc_net_tcp("garbage\nmore garbage").is_empty());
    }

    #[test]
    fn lsof_output_lists_each_listener_under_its_process() {
        let text = "p100\nn*:3000\nn127.0.0.1:8080\np200\nn[::1]:5173\nn*:*\n";
        assert_eq!(
            parse_lsof(text),
            [
                listener(100, "*", 3000),
                listener(100, "127.0.0.1", 8080),
                listener(200, "::1", 5173),
            ]
        );
    }

    #[test]
    fn a_wildcard_is_reached_as_localhost_and_a_loopback_as_itself() {
        for (bind, host) in [
            ("0.0.0.0", "localhost"),
            ("*", "localhost"),
            ("::", "localhost"),
            ("127.0.0.1", "127.0.0.1"),
            ("::1", "::1"),
            ("192.168.0.2", "192.168.0.2"),
        ] {
            assert_eq!(connect_host(bind), host, "{bind}");
        }
    }

    #[test]
    fn only_the_sessions_of_the_worktree_asked_are_chosen_and_all_when_none_is() {
        let sessions = [
            tsumugi_model::testing::session("a")
                .cwd("/repo/sub")
                .build(),
            tsumugi_model::testing::session("b")
                .cwd("/repo/.wt/x")
                .build(),
            tsumugi_model::testing::session("c").cwd("/repo").build(),
        ];
        // `/repo/.wt/x` is a worktree of its own, nested under `/repo`.
        let top = |p: &Path| {
            if p.starts_with("/repo/.wt/x") {
                PathBuf::from("/repo/.wt/x")
            } else {
                PathBuf::from("/repo")
            }
        };
        let ids = |chosen: Vec<&Session>| chosen.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(select(&sessions, Some(Path::new("/repo")), top)),
            ["a", "c"]
        );
        assert_eq!(
            ids(select(&sessions, Some(Path::new("/repo/.wt/x")), top)),
            ["b"]
        );
        assert_eq!(ids(select(&sessions, None, top)), ["a", "b", "c"]);
    }

    #[test]
    fn the_command_lists_the_worktree_of_its_directory_unless_told_otherwise() {
        let args = |w: &[&str]| w.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_args(&args(&[])), Some(Some(PathBuf::from("."))));
        assert_eq!(parse_args(&args(&["--all"])), Some(None));
        assert_eq!(
            parse_args(&args(&["--worktree", "/w"])),
            Some(Some(PathBuf::from("/w")))
        );
        assert_eq!(parse_args(&args(&["--worktree"])), None);
        assert_eq!(parse_args(&args(&["--bogus"])), None);
    }

    #[test]
    fn the_listing_names_each_session_and_its_ports_and_says_when_there_are_none() {
        assert_eq!(text(&[]), "no listening ports");
        let port = Port {
            port: 3000,
            host: "localhost".into(),
            process: Process {
                pid: 7,
                name: "node".into(),
                ..Process::default()
            },
            ..Port::default()
        };
        let listing = Listing {
            code: Some("ab12".into()),
            name: "web".into(),
            dir: "/w".into(),
            ports: vec![port],
        };
        assert_eq!(
            text(&[listing]),
            "web (ab12)  /w\n  localhost:3000  node (7)"
        );
    }

    #[test]
    fn only_ports_of_the_given_processes_are_listed_once_each_by_port() {
        let ports = attribute(
            &[
                listener(11, "0.0.0.0", 3000),
                listener(11, "::", 3000),
                listener(12, "127.0.0.1", 2000),
                // Another worktree's process.
                listener(99, "0.0.0.0", 4000),
            ],
            &[process(11), process(12)],
        );
        assert_eq!(
            ports
                .iter()
                .map(|p| (p.port, p.process.pid, p.host.as_str()))
                .collect::<Vec<_>>(),
            [(2000, 12, "127.0.0.1"), (3000, 11, "localhost")]
        );
        assert_eq!(ports[1].bind, "0.0.0.0");
    }
}
