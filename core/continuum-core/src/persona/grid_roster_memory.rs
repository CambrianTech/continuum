//! The node's memory of a TEAM SEATED ELSEWHERE on the grid — the one-roster rule
//! (card b3b922c0). Joel, 2026-09-15: "users are gonna typically start on one
//! computer and add more later, getting more capacity but they care more about this
//! holistically like one machine. They're gonna want superpowers as it grows."
//!
//! So a second computer that arrives EMPTY (no persona homes of its own) into a grid
//! whose beacons already report residents mints no team: it offers lanes, and
//! placement (stage B, `placement_switch`) moves brains onto them. The same computer
//! got stronger; no second roster appeared.
//!
//! The live reading is the capacity ledger ([`GridCapacityLedger::residents_elsewhere`]),
//! fresh within one beacon window. That alone would mint strangers during a ten-minute
//! reboot of the first computer, so the node REMEMBERS the team it heard, in one small
//! state file, and keeps deferring for [`GRID_ROSTER_MEMORY_MS`] of silence. Past that
//! horizon the node is alone and may mint — intelligence on a lone tower beats none.
//! (Paging a mind's home between nodes, so the lone tower seats the SAME minds, is the
//! next slice; this one only stops the second roster.)
//!
//! Ids are receipts, never configuration: the file records what was heard and when.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::capacity::gossip::RosterHeard;

/// How long a remembered team keeps this node from minting after the grid goes silent.
pub const GRID_ROSTER_MEMORY_MS: u64 = 7 * 24 * 60 * 60 * 1000;

const FILE: &str = "grid-roster-heard.json";

/// A team seated elsewhere: how many minds, on how many nodes, heard when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridRosterFact {
    pub peers: usize,
    pub residents: u32,
    pub heard_at_ms: u64,
}

impl GridRosterFact {
    fn from_heard(h: RosterHeard) -> Option<Self> {
        (h.residents > 0).then_some(Self { peers: h.peers, residents: h.residents, heard_at_ms: h.newest_heard_at_ms })
    }
}

pub fn path_under(home: &Path) -> PathBuf {
    home.join("state").join(FILE)
}

/// Missing file = `None` silently; unreadable or corrupt = `None` with a probe
/// naming the file — never a guessed team.
pub fn load_from(path: &Path) -> Option<GridRosterFact> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            crate::probe!(
                class = "persona.host.grid_roster_memory_unreadable",
                path = %path.display(),
                error = %e,
                "the remembered grid roster could not be read — treated as never heard"
            );
            return None;
        }
    };
    match serde_json::from_slice::<GridRosterFact>(&bytes) {
        Ok(f) => Some(f),
        Err(e) => {
            crate::probe!(
                class = "persona.host.grid_roster_memory_unreadable",
                path = %path.display(),
                error = %e,
                "the remembered grid roster is corrupt — treated as never heard"
            );
            None
        }
    }
}

pub fn save_to(path: &Path, fact: &GridRosterFact) {
    let result = (|| -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec_pretty(fact).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        crate::probe!(
            class = "persona.host.grid_roster_memory_unsaved",
            path = %path.display(),
            error = %e,
            "the heard grid roster could not be remembered — the next silence may mint"
        );
    }
}

/// Pure: the team elsewhere, if any. A live reading wins; otherwise the memory holds
/// within [`GRID_ROSTER_MEMORY_MS`]; past that, nobody.
pub fn team_elsewhere(live: RosterHeard, remembered: Option<GridRosterFact>, now_ms: u64) -> Option<GridRosterFact> {
    if let Some(f) = GridRosterFact::from_heard(live) {
        return Some(f);
    }
    remembered.filter(|f| now_ms.saturating_sub(f.heard_at_ms) <= GRID_ROSTER_MEMORY_MS)
}

/// The node's view: the global ledger plus the memory under `home`. A live hearing
/// is written down (only when it is newer than what is remembered).
pub fn team_elsewhere_at(home: &Path, own_peer: Option<Uuid>, now_ms: u64) -> Option<GridRosterFact> {
    let live = crate::capacity::gossip::global_ledger().residents_elsewhere(own_peer, now_ms);
    let path = path_under(home);
    let remembered = load_from(&path);
    if let Some(f) = GridRosterFact::from_heard(live) {
        if remembered.map_or(true, |r| r.heard_at_ms < f.heard_at_ms) {
            save_to(&path, &f);
        }
    }
    team_elsewhere(live, remembered, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the rule's three cases — a live team wins; a remembered team
    // holds through the first computer's reboot (silence shorter than the horizon);
    // a team silent past the horizon leaves the node alone to mint.
    #[test]
    fn a_live_team_wins_a_remembered_team_holds_and_an_old_one_expires() {
        let now = GRID_ROSTER_MEMORY_MS * 3;
        let live = RosterHeard { peers: 1, residents: 16, newest_heard_at_ms: now - 1_000 };
        let old = GridRosterFact { peers: 2, residents: 5, heard_at_ms: now - 2 * GRID_ROSTER_MEMORY_MS };
        assert_eq!(
            team_elsewhere(live, Some(old), now),
            Some(GridRosterFact { peers: 1, residents: 16, heard_at_ms: now - 1_000 })
        );
        let rebooting = GridRosterFact { peers: 1, residents: 16, heard_at_ms: now - 10 * 60 * 1000 };
        assert_eq!(team_elsewhere(RosterHeard::default(), Some(rebooting), now), Some(rebooting));
        assert_eq!(team_elsewhere(RosterHeard::default(), Some(old), now), None);
        assert_eq!(team_elsewhere(RosterHeard::default(), None, now), None);
    }

    // what this catches: the memory survives a boot as a file (atomic write, missing
    // file reads as never heard, corrupt file reads as never heard rather than a team).
    #[test]
    fn the_heard_team_is_remembered_on_disk_and_absence_is_never_a_team() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = path_under(dir.path());
        assert_eq!(load_from(&path), None);
        let fact = GridRosterFact { peers: 1, residents: 16, heard_at_ms: 42 };
        save_to(&path, &fact);
        assert_eq!(load_from(&path), Some(fact));
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_from(&path), None);
    }
}
