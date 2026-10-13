//! Local TCP ports: the ONE "is this port free" probe.
//!
//! A successful bind-then-drop proves the port is free for that address at that instant; the
//! gap until the real owner binds it is the caller's to absorb (a lost race surfaces as that
//! owner's bind error, never as a silent wrong-port serve). Used by the ephemeral serving lanes
//! and by the citizen port leases (`modules::ports`), so the two never disagree on "free".

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// How long a loopback connect may take to count as "someone is listening". A listener on
/// loopback accepts at once; the bound is for Windows, where a refused loopback connect can
/// take seconds of SYN retries.
const LISTEN_PROBE: Duration = Duration::from_millis(200);

/// The first of `candidates`, in order, that `host` can bind right now.
pub fn first_bindable(host: &str, candidates: impl IntoIterator<Item = u16>) -> Option<u16> {
    candidates.into_iter().find(|&port| std::net::TcpListener::bind((host, port)).is_ok())
}

/// Whether a server is listening on `port` where a loopback client would reach it. A server on
/// the wildcard (`0.0.0.0` or `::`, most dev servers' default) accepts a loopback connect, and
/// that is how it is seen on Windows, where binding `127.0.0.1:P` SUCCEEDS beside another
/// process's wildcard listener (Fable on #4540). Probed by connecting rather than by binding
/// the wildcard, so host firewalls never prompt for a probe. BLOCKING: up to
/// [`LISTEN_PROBE`] per address; call it off the async runtime.
pub fn listened_on(port: u16) -> bool {
    [IpAddr::V4(Ipv4Addr::LOCALHOST), IpAddr::V6(Ipv6Addr::LOCALHOST)]
        .into_iter()
        .any(|ip| TcpStream::connect_timeout(&SocketAddr::new(ip, port), LISTEN_PROBE).is_ok())
}

/// The first of `candidates` that a new loopback server can take: `127.0.0.1` binds AND nobody
/// is listening on it through a wildcard. BLOCKING, as [`listened_on`].
pub fn first_unused(candidates: impl IntoIterator<Item = u16>) -> Option<u16> {
    candidates.into_iter().find(|&port| first_bindable("127.0.0.1", [port]).is_some() && !listened_on(port))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a probe that returns a port someone holds, or that ignores the
    // candidate order the callers rely on (the lane scan walks up from its base).
    #[test]
    fn the_first_bindable_candidate_skips_a_held_port() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral");
        let taken = held.local_addr().expect("addr").port();
        assert_eq!(first_bindable("127.0.0.1", [taken]), None, "a held port is not bindable");
        let chosen = first_bindable("127.0.0.1", [taken, 0]).expect("port 0 always binds");
        assert_eq!(chosen, 0, "the next candidate in order");
    }

    // what this catches (Fable on #4540): a server listening on the WILDCARD read as a free
    // port, because on Windows a loopback bind succeeds beside it. Held on 0.0.0.0, the port
    // is seen as listened on and never offered as unused, on every platform.
    #[test]
    fn a_wildcard_listener_makes_its_port_used() {
        let server = std::net::TcpListener::bind(("0.0.0.0", 0)).expect("bind wildcard");
        let port = server.local_addr().expect("addr").port();
        assert!(listened_on(port), "a wildcard server answers a loopback connect");
        assert_eq!(first_unused([port]), None, "its port is not offered");
        drop(server);
        assert!(!listened_on(port), "once it stops, nobody answers");
    }
}
