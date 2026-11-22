//! How a page reaches what it shows, and what it keeps of that while the work
//! machine is away: the address in its bar stays the work machine's own, and a
//! reload goes through a forward made again.

use std::collections::HashMap;

use crate::link::{RemotePhase, RemoteStatus};

/// How a page reaches its address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Route {
    /// Straight from this computer.
    Here,
    /// Through a forward to the loopback of the work machine `host`.
    Through { host: String, connected: bool },
}

impl Route {
    /// Whether the page's address is out of reach now.
    pub(super) fn is_away(&self) -> bool {
        matches!(
            self,
            Self::Through {
                connected: false,
                ..
            }
        )
    }
}

/// The port of the loopback address `url`, which is the loopback of the work
/// machine for an environment on one.
pub(super) fn loopback_port(url: &str) -> Option<u16> {
    let (_, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, port) = rest[..end].rsplit_once(':')?;
    let loopback = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "[::1]";
    port.parse().ok().filter(|_| loopback)
}

/// How a page of an environment that stands as `status` with its work
/// machine reaches `url`: only the loopback of a work machine is reached
/// through it.
pub(super) fn route(status: Option<&RemoteStatus>, url: &str) -> Route {
    match status {
        Some(status) if loopback_port(url).is_some() => Route::Through {
            host: status.host.clone(),
            connected: status.phase == RemotePhase::Connected,
        },
        _ => Route::Here,
    }
}

/// How a page is loaded again.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Reload<'a> {
    /// As it is: its address does not depend on a forward.
    InPlace,
    /// At `home`, the work machine's own address, through a forward made
    /// again.
    Through(&'a str),
}

/// How to load again a page whose work machine's own address is `home`, when
/// the view shows nothing (`blank`) or something. A forward is gone with the
/// connection that made it, so a page loaded through one, and one that never
/// loaded for lack of one, are loaded at their address again.
pub(super) fn reload(home: &str, blank: bool) -> Reload<'_> {
    if home.is_empty() || (!blank && loopback_port(home).is_none()) {
        Reload::InPlace
    } else {
        Reload::Through(home)
    }
}

/// The ports of this computer that forwards reached the work machine's by,
/// as the page has met them: a forward is gone with the connection, and the
/// address of a page that went through it must still be told as the work
/// machine's own.
#[derive(Debug, Default)]
pub(super) struct Ports(HashMap<u16, u16>);

impl Ports {
    /// `url` as the environment itself writes it. `work_port` says which
    /// port of the work machine a forwarded `local` one reaches while it is
    /// connected.
    pub(super) fn shown(&mut self, url: &str, work_port: impl Fn(u16) -> Option<u16>) -> String {
        if let Some(local) = loopback_port(url)
            && let Some(work) = work_port(local)
        {
            self.0.insert(local, work);
        }
        super::shown_address(url, |local| {
            work_port(local).or_else(|| self.0.get(&local).copied())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(phase: RemotePhase) -> RemoteStatus {
        RemoteStatus {
            phase,
            ..RemoteStatus::connected("work")
        }
    }

    #[test]
    fn only_the_loopback_of_a_work_machine_is_reached_through_it() {
        let connected = status(RemotePhase::Connected);
        let through = Route::Through {
            host: "work".into(),
            connected: true,
        };
        for url in [
            "http://localhost:3000/a",
            "http://127.0.0.1:8080",
            "http://[::1]:5173/",
        ] {
            assert_eq!(route(Some(&connected), url), through, "{url}");
            assert_eq!(route(None, url), Route::Here, "{url}");
        }
        for url in [
            "https://example.com/",
            "http://example.com:3000/",
            "http://localhost/",
            "about:blank",
        ] {
            assert_eq!(route(Some(&connected), url), Route::Here, "{url}");
        }
    }

    #[test]
    fn a_page_is_away_whenever_its_work_machine_is_not_connected() {
        for phase in [
            RemotePhase::Connecting,
            RemotePhase::Reconnecting,
            RemotePhase::Error,
        ] {
            assert!(route(Some(&status(phase)), "http://localhost:3000/").is_away());
        }
        assert!(
            !route(
                Some(&status(RemotePhase::Connected)),
                "http://localhost:3000/"
            )
            .is_away()
        );
        assert!(!route(Some(&status(RemotePhase::Error)), "https://example.com/").is_away());
    }

    #[test]
    fn the_address_stays_the_work_machines_own_while_it_is_away_and_after_it_returns() {
        let mut ports = Ports::default();
        let connected = |local: u16| (local == 41000).then_some(3000);
        let away = |_: u16| None;
        let url = "http://localhost:41000/a?b#c";
        assert_eq!(ports.shown(url, connected), "http://localhost:3000/a?b#c");
        // The forward is gone with the connection.
        assert_eq!(ports.shown(url, away), "http://localhost:3000/a?b#c");
        // Made again at another port, it is the same address of the work machine.
        let again = |local: u16| (local == 42000).then_some(3000);
        assert_eq!(
            ports.shown("http://localhost:42000/a?b#c", again),
            "http://localhost:3000/a?b#c"
        );
    }

    #[test]
    fn a_retry_loads_a_page_through_a_forward_made_again_and_reloads_any_other_as_it_is() {
        let home = "http://localhost:3000/a";
        assert_eq!(reload(home, false), Reload::Through(home));
        // A page restored while its work machine was away never loaded.
        assert_eq!(reload(home, true), Reload::Through(home));
        assert_eq!(reload("https://example.com/", false), Reload::InPlace);
        assert_eq!(reload("", true), Reload::InPlace);
    }

    /// The window's stand at one moment: the address in its bar, whether the
    /// page is covered by the notice, and what a retry would load.
    fn stand(
        ports: &mut Ports,
        status: &RemoteStatus,
        forwarded: impl Fn(u16) -> Option<u16>,
        url: &str,
    ) -> (String, bool) {
        let address = ports.shown(url, forwarded);
        let away = route(Some(status), &address).is_away();
        (address, away)
    }

    #[test]
    fn the_address_bar_survives_a_disconnection_and_the_page_comes_back_on_a_retry() {
        let mut ports = Ports::default();
        let page = "http://localhost:41000/a";
        let work = "http://localhost:3000/a";
        let up = |local: u16| (local == 41000).then_some(3000);
        let down = |_: u16| None;

        let connected = status(RemotePhase::Connected);
        assert_eq!(
            stand(&mut ports, &connected, up, page),
            (work.into(), false)
        );

        // The connection drops: the notice shows and the bar still reads
        // the work machine's address, not this computer's stale port.
        let lost = status(RemotePhase::Reconnecting);
        assert_eq!(stand(&mut ports, &lost, down, page), (work.into(), true));
        assert!(matches!(reload(work, false), Reload::Through(home) if home == work));

        // It returns with the forward at another port: the retry loads the
        // work machine's address through it, and the bar reads the same.
        let again = |local: u16| (local == 42000).then_some(3000);
        let back = "http://localhost:42000/a";
        assert_eq!(
            stand(&mut ports, &connected, again, back),
            (work.into(), false)
        );
    }

    #[test]
    fn an_address_that_never_went_through_a_forward_is_shown_as_it_is() {
        let mut ports = Ports::default();
        for url in [
            "https://example.com/",
            "http://localhost:5173/",
            "about:blank",
        ] {
            assert_eq!(ports.shown(url, |_| None), url);
        }
    }
}
