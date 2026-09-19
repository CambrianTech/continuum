//! Every airc id THIS process wears — so a node recognises itself under any of them.
//!
//! One core speaks in several airc scopes (the project room, the operator's seat, the
//! agent seat, every persona it hosts), and each scope carries a DIFFERENT transport peer
//! id. The M5 (2026-09-19, card 2500d2f1) beaconed capacity as `2f0aed7f` in its project
//! scope while its placement switch knew itself only as the operator handle's id: its own
//! seat read as a foreign one, it spilled Demetri, Aris, Solomon and Aiko to ITSELF, asked
//! its own seat for slots, and wrote `remote_peer = 2f0aed7f` into their durable overrides
//! — so every boot since bore them on a loopback lane and "fell them home" a minute later
//! ("seat silent: no capacity beacon", the seat being this very core). #4219 stopped the
//! live half (an offer carries its process origin); this is the durable half: ONE resolver
//! over every id the process wears, asked wherever a peer id is about to be treated as
//! another machine — the override at build, the offers a placement pass may seat on, the
//! fall-home/return rule, and the reservation ask.
//!
//! Sources, unioned: every `Airc` handle this process attaches (registered at attach), the
//! capacity ledger's rows that carry this process's own origin nonce (the ids the grid
//! hears us under, whatever scope stamped them), and the nonce itself.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use uuid::Uuid;

fn registered() -> &'static Mutex<HashSet<Uuid>> {
    static OWN: OnceLock<Mutex<HashSet<Uuid>>> = OnceLock::new();
    OWN.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Record one more id this process answers to — called wherever an airc handle is
/// attached (a persona/operator/agent runtime, the core's own project-scope handle).
pub fn register(peer: Uuid) {
    registered().lock().unwrap_or_else(|p| p.into_inner()).insert(peer); // JUSTIFIED unwrap_or_else: a poisoned set still holds the ids; membership is bookkeeping, never truth
}

/// Is `peer` one of the ids this process wears? A peer that is this node is never a
/// remote seat: not for an offload, not for a return, not for a reservation ask, and a
/// durable override naming it is a record of the old confusion, not an assignment.
pub fn is_this_node(peer: Uuid) -> bool {
    if peer == crate::capacity::gossip::this_process_origin() {
        return true;
    }
    if registered().lock().unwrap_or_else(|p| p.into_inner()).contains(&peer) { // JUSTIFIED unwrap_or_else: a poisoned set still holds the ids; membership is bookkeeping, never truth
        return true;
    }
    crate::capacity::gossip::global_ledger().own_peer_ids().contains(&peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 2500d2f1): a node that wears N airc ids must answer "me"
    // for every one of them and for nobody else — the M5 knew one of its three and
    // seated minds on the other two.
    #[test]
    fn a_node_recognises_every_id_it_wears_and_no_other() {
        let (project_scope, operator_scope, agent_scope, stranger) =
            (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        for id in [project_scope, operator_scope, agent_scope] {
            register(id);
        }
        assert!(is_this_node(project_scope) && is_this_node(operator_scope) && is_this_node(agent_scope));
        assert!(is_this_node(crate::capacity::gossip::this_process_origin()), "the process nonce is me too");
        assert!(!is_this_node(stranger), "a peer this process never wore is another machine");
    }
}
