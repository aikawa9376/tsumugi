//! A TCP port a session's work listens on, as the system lists it.

use serde::{Deserialize, Serialize};

use crate::Process;

/// A port listened on by a process of a session's.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    /// What tells it apart from every other port listed.
    pub key: String,
    pub port: u16,
    /// The address the process listens on; a wildcard when it listens on
    /// every address.
    pub bind: String,
    /// The host a client of the work machine reaches it by: `localhost`
    /// where it listens on every address or on loopback.
    pub host: String,
    /// The process that listens, as the session's processes list it.
    pub process: Process,
}

impl Port {
    /// The address to copy, and to open where it serves web pages, in the
    /// work machine's own terms.
    pub fn address(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }

    /// The address as a web address.
    pub fn url(&self) -> String {
        format!("http://{}", self.address())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(host: &str) -> Port {
        Port {
            port: 3000,
            host: host.into(),
            ..Port::default()
        }
    }

    #[test]
    fn an_address_brackets_an_ipv6_host_and_a_url_is_http() {
        assert_eq!(port("localhost").address(), "localhost:3000");
        assert_eq!(port("::1").address(), "[::1]:3000");
        assert_eq!(port("192.168.0.2").url(), "http://192.168.0.2:3000");
    }
}
