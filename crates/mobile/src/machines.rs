//! The work machines the phone paired with, known by their keys, each with
//! the name and the address the user gave it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tsumugi_net::Fingerprint;

/// The port a work machine's gateway listens on when the address leaves it
/// out.
pub const GATEWAY_PORT: u16 = 47130;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    /// Its key, the whole of what it is: a new address reaches the same
    /// machine, another key never does.
    pub key: Fingerprint,
    pub name: String,
    /// Where its gateway is reached, as `host:port`.
    pub address: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MachinesError {
    #[error("give the work machine's address, such as work-box.local or 192.168.1.5:47130")]
    Address,
    #[error("no work machine with that key is paired")]
    Unknown,
    #[error("cannot keep the work machines: {0}")]
    File(String),
}

/// `typed` as the address of a gateway: `host:port`, the port the gateway's
/// own where none is given.
pub fn address(typed: &str) -> Result<String, MachinesError> {
    let typed = typed.trim();
    if typed.is_empty() || typed.chars().any(char::is_whitespace) || typed.contains('/') {
        return Err(MachinesError::Address);
    }
    // An IPv6 address is written in brackets when a port follows it.
    if let Some(rest) = typed.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or(MachinesError::Address)?;
        if host.is_empty() {
            return Err(MachinesError::Address);
        }
        return match after {
            "" => Ok(format!("[{host}]:{GATEWAY_PORT}")),
            port => port
                .strip_prefix(':')
                .filter(|port| port.parse::<u16>().is_ok_and(|p| p != 0))
                .map(|port| format!("[{host}]:{port}"))
                .ok_or(MachinesError::Address),
        };
    }
    match typed.matches(':').count() {
        0 => Ok(format!("{typed}:{GATEWAY_PORT}")),
        1 => {
            let (host, port) = typed.split_once(':').unwrap();
            if host.is_empty() || !port.parse::<u16>().is_ok_and(|p| p != 0) {
                return Err(MachinesError::Address);
            }
            Ok(typed.to_string())
        }
        // A bare IPv6 address, with no port.
        _ => Ok(format!("[{typed}]:{GATEWAY_PORT}")),
    }
}

/// The file the paired machines are kept in.
pub struct Machines {
    dir: PathBuf,
    path: PathBuf,
}

impl Machines {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            path: dir.join("machines.json"),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The machines kept, in the order they were paired; none before any
    /// is.
    pub fn list(&self) -> Result<Vec<Machine>, MachinesError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| MachinesError::File(e.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(MachinesError::File(error.to_string())),
        }
    }

    pub fn get(&self, key: &Fingerprint) -> Result<Machine, MachinesError> {
        self.list()?
            .into_iter()
            .find(|m| m.key == *key)
            .ok_or(MachinesError::Unknown)
    }

    /// Keep `machine`, in place of the one of its key if it was paired
    /// before.
    pub fn keep(&self, machine: Machine) -> Result<(), MachinesError> {
        let mut machines = self.list()?;
        match machines.iter_mut().find(|m| m.key == machine.key) {
            Some(kept) => *kept = machine,
            None => machines.push(machine),
        }
        self.write(&machines)
    }

    /// Reach the machine of `key` at `typed` from now on.
    pub fn readdress(&self, key: &Fingerprint, typed: &str) -> Result<Machine, MachinesError> {
        let address = address(typed)?;
        self.change(key, |m| m.address = address)
    }

    pub fn rename(&self, key: &Fingerprint, name: &str) -> Result<Machine, MachinesError> {
        let name = name.trim().to_string();
        self.change(key, |m| {
            if !name.is_empty() {
                m.name = name
            }
        })
    }

    /// Keep the machine of `key` no more. The machine still knows this
    /// phone's key until it is revoked there.
    pub fn forget(&self, key: &Fingerprint) -> Result<(), MachinesError> {
        let mut machines = self.list()?;
        machines.retain(|m| m.key != *key);
        self.write(&machines)
    }

    fn change(
        &self,
        key: &Fingerprint,
        change: impl FnOnce(&mut Machine),
    ) -> Result<Machine, MachinesError> {
        let mut machines = self.list()?;
        let machine = machines
            .iter_mut()
            .find(|m| m.key == *key)
            .ok_or(MachinesError::Unknown)?;
        change(machine);
        let changed = machine.clone();
        self.write(&machines)?;
        Ok(changed)
    }

    fn write(&self, machines: &[Machine]) -> Result<(), MachinesError> {
        let fail = |e: std::io::Error| MachinesError::File(e.to_string());
        let text = serde_json::to_vec_pretty(machines).expect("machines are plain data");
        let written = self.path.with_extension("json.new");
        std::fs::write(&written, text).map_err(fail)?;
        std::fs::rename(&written, &self.path).map_err(fail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> Fingerprint {
        Fingerprint::of([byte])
    }

    fn machine(byte: u8, name: &str) -> Machine {
        Machine {
            key: key(byte),
            name: name.into(),
            address: "work-box.local:47130".into(),
        }
    }

    #[test]
    fn an_address_without_a_port_reaches_the_gateways_own() {
        let cases = [
            ("work-box.local", Ok("work-box.local:47130")),
            (" 192.168.1.5 ", Ok("192.168.1.5:47130")),
            ("192.168.1.5:5000", Ok("192.168.1.5:5000")),
            ("fd00::5", Ok("[fd00::5]:47130")),
            ("[fd00::5]", Ok("[fd00::5]:47130")),
            ("[fd00::5]:5000", Ok("[fd00::5]:5000")),
            ("", Err(MachinesError::Address)),
            ("work box", Err(MachinesError::Address)),
            ("http://work-box", Err(MachinesError::Address)),
            ("work-box:0", Err(MachinesError::Address)),
            ("work-box:port", Err(MachinesError::Address)),
            (":47130", Err(MachinesError::Address)),
            ("[]:47130", Err(MachinesError::Address)),
        ];
        for (typed, want) in cases {
            assert_eq!(address(typed), want.map(String::from), "{typed:?}");
        }
    }

    #[test]
    fn no_machine_is_kept_before_one_pairs() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(Machines::new(dir.path()).list(), Ok(Vec::new()));
    }

    #[test]
    fn a_machine_paired_again_takes_the_place_of_the_one_of_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let machines = Machines::new(dir.path());
        machines.keep(machine(1, "work")).unwrap();
        machines.keep(machine(2, "home")).unwrap();

        machines.keep(machine(1, "work again")).unwrap();

        let names: Vec<_> = machines
            .list()
            .unwrap()
            .into_iter()
            .map(|m| m.name)
            .collect();
        assert_eq!(names, ["work again", "home"]);
    }

    #[test]
    fn a_new_address_keeps_the_machine_known_by_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let machines = Machines::new(dir.path());
        machines.keep(machine(1, "work")).unwrap();

        machines.readdress(&key(1), "100.64.0.7").unwrap();

        let kept = machines.get(&key(1)).unwrap();
        assert_eq!(kept.address, "100.64.0.7:47130");
        assert_eq!(kept.name, "work");
    }

    #[test]
    fn a_machine_is_renamed_but_never_to_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let machines = Machines::new(dir.path());
        machines.keep(machine(1, "work")).unwrap();

        machines.rename(&key(1), "  ").unwrap();
        assert_eq!(machines.get(&key(1)).unwrap().name, "work");
        machines.rename(&key(1), "office").unwrap();
        assert_eq!(machines.get(&key(1)).unwrap().name, "office");
    }

    #[test]
    fn a_machine_not_paired_cannot_be_changed() {
        let dir = tempfile::tempdir().unwrap();
        let machines = Machines::new(dir.path());

        assert_eq!(
            machines.readdress(&key(9), "work-box"),
            Err(MachinesError::Unknown)
        );
    }

    #[test]
    fn a_machine_forgotten_is_kept_no_more() {
        let dir = tempfile::tempdir().unwrap();
        let machines = Machines::new(dir.path());
        machines.keep(machine(1, "work")).unwrap();
        machines.keep(machine(2, "home")).unwrap();

        machines.forget(&key(1)).unwrap();

        assert_eq!(machines.list().unwrap(), [machine(2, "home")]);
    }
}
