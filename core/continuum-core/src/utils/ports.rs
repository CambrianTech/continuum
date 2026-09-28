//! Local TCP ports: the ONE "can this port be bound right now" probe.
//!
//! A successful bind-then-drop proves the port is free at that instant; the gap until the
//! real owner binds it is the caller's to absorb (a lost race surfaces as that owner's bind
//! error, never as a silent wrong-port serve). Used by the ephemeral serving lanes and by the
//! citizen port leases (`modules::ports`), so the two never disagree on what "free" means.

/// The first of `candidates`, in order, that `host` can bind right now.
pub fn first_bindable(host: &str, candidates: impl IntoIterator<Item = u16>) -> Option<u16> {
    candidates.into_iter().find(|&port| std::net::TcpListener::bind((host, port)).is_ok())
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
}
