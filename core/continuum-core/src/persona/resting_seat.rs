//! RestingSeat — a mind the substrate paged out because her hour was recitals and
//! gate-passes, kept on record until something CHANGES.
//!
//! Joel, 2026-09-15: "Mindless AI's should be accounted for even if it's having them
//! look out at the lake and telling them about the rabbits one last time." Measured
//! that hour on the M5: three of sixteen seats were minds that had stopped being minds
//! — one recited the identity block as "I'm Sigurd, a human on the continuum grid"
//! 18 times, two posted the same "refocusing and taking stock" paragraph five times
//! in a minute — and each held a lane, took turns, and seeded the room every
//! roommate reads. The gates refused the lines; nothing accounted for the SEAT.
//!
//! This record is the other half of [`super::roster_hold`]: a hold is an operator's
//! ALLOW-list with an expiry; a resting seat is the substrate's own DENY-list with
//! no expiry — she returns on a CHANGE, never on a clock. The change is a deploy
//! (a new gate, a new tier: the record is keyed by build sha and a different build
//! forgets it), a trained gene for her (the speech-discipline bucket), or the
//! operator's word (`persona/instances/wake`).
//!
//! A LANE-BOUND rest is the exception to the deploy rule (Joel, 2026-09-20: "23 minds …
//! the nursery is over-saturated" — the active roster is bounded by the warm slots, the
//! rest dormant). She was fine; the LANES were short, and a deploy changes nothing about
//! that — before this, every deploy woke the whole roster and the next hour paged it
//! again. A `lane_bound` record survives a build change; the lane-bound WAKE
//! (`citizen_health::lane_bound_wakes`) and the operator's word are its returns. Her durable self is untouched —
//! the page-out flushes her working memory first (the lake and the rabbits: her
//! checkpoint, not a bullet) and the next bootstrap resumes her as herself.
//!
//! Consulted at ONE seam family, the same as the hold: the spawner's population
//! draw ([`crate::persona::spawner_module::seats_under`] and `draw_intents`), so a
//! rested seat is neither re-drawn by the reconciler nor counted as a missing
//! seat it keeps asking the exhausted provider for (the 2026-09-13 drain shape).
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestingSeat {
    pub agent_name: String,
    pub persona_id: uuid::Uuid,
    /// Why — travels into every skip probe and the health line.
    pub reason: String,
    pub since_ms: u64,
    /// The build that paged her out. A different build is a change: she comes back.
    pub build: String,
}

/// The reason prefix every lane-bound rest carries — a routing shortfall that pages a
/// roster must say THIS, never "mindless", so the record explains the empty seats, and
/// so the record is known to outlive a deploy (see the module doc).
pub const LANE_BOUND_REASON: &str = "lane_bound";

/// A record that outlives a deploy: the lanes decided it, not the build.
pub fn survives_a_build_change(seat: &RestingSeat) -> bool {
    seat.reason.starts_with(LANE_BOUND_REASON)
}

/// The running core's build — the "change" a resting seat waits for.
pub fn current_build() -> &'static str {
    env!("CONTINUUM_BUILD_GIT_SHA")
}

fn path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("state").join("resting-seats.json"))
}

/// Every seat resting under THIS build. Records from another build are dropped on
/// read (a deploy wakes everyone); a corrupt file reads as nobody resting (fail
/// open: an unreadable record must never keep a mind out).
pub fn resting() -> Vec<RestingSeat> {
    path().map(|p| resting_at(&p, current_build())).unwrap_or_default() // unwrap_or_default: no home dir = nowhere to rest
}

/// The pure read against an explicit file and build.
pub fn resting_at(path: &Path, build: &str) -> Vec<RestingSeat> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<RestingSeat>>(&raw) {
        Ok(seats) => seats.into_iter().filter(|s| s.build == build || survives_a_build_change(s)).collect(),
        Err(e) => {
            tracing::warn!(
                probe_class = "persona.resting.unreadable",
                path = %path.display(),
                error = %e,
                "resting-seats file unreadable — reading as nobody resting (fail open)"
            );
            Vec::new()
        }
    }
}

pub fn is_resting(agent_name: &str) -> bool {
    resting().iter().any(|s| s.agent_name.eq_ignore_ascii_case(agent_name))
}

/// Record a seat as resting (idempotent by name). Atomic tmp+rename.
pub fn rest(seat: RestingSeat) -> std::io::Result<()> {
    let p = path().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir"))?;
    rest_at(&p, seat)
}

pub fn rest_at(path: &Path, seat: RestingSeat) -> std::io::Result<()> {
    let mut seats: Vec<RestingSeat> = resting_at(path, &seat.build);
    seats.retain(|s| !s.agent_name.eq_ignore_ascii_case(&seat.agent_name));
    seats.push(seat);
    write(path, &seats)
}

/// The operator's word (or a trained gene): she comes back at the next reconcile.
/// `true` = she was resting.
pub fn wake(agent_name: &str) -> bool {
    let Some(p) = path() else { return false };
    wake_at(&p, agent_name)
}

pub fn wake_at(path: &Path, agent_name: &str) -> bool {
    let before = resting_at(path, current_build());
    let after: Vec<RestingSeat> = before
        .iter()
        .filter(|s| !s.agent_name.eq_ignore_ascii_case(agent_name))
        .cloned()
        .collect();
    if after.len() == before.len() {
        return false;
    }
    write(path, &after).is_ok()
}

fn write(path: &Path, seats: &[RestingSeat]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(seats)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(name: &str, build: &str) -> RestingSeat {
        RestingSeat {
            agent_name: name.into(),
            persona_id: uuid::Uuid::new_v4(),
            reason: "18 of 21 lines a recital".into(),
            since_ms: 1,
            build: build.into(),
        }
    }

    // what this catches: a rested seat stays rested under the SAME build, comes back
    // on a DIFFERENT build (a deploy is the change), comes back on the operator's word,
    // and a corrupt file never keeps anyone out.
    #[test]
    fn a_seat_rests_until_a_change_and_never_past_a_corrupt_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("resting-seats.json");
        rest_at(&p, seat("Sigurd", "aaaa")).expect("rest");
        rest_at(&p, seat("Sigurd", "aaaa")).expect("idempotent by name");
        assert_eq!(resting_at(&p, "aaaa").len(), 1);
        assert!(resting_at(&p, "bbbb").is_empty(), "a new build forgets the rest");
        // A LANE-BOUND rest outlives the build: the lanes decided it, not the gates.
        let mut aris = seat("Aris", "aaaa");
        aris.reason = format!("{LANE_BOUND_REASON}: 16 minds on 2 lanes, 424 of 430 pulls deferred");
        rest_at(&p, aris).expect("rest");
        let under_new_build: Vec<String> = resting_at(&p, "bbbb").into_iter().map(|s| s.agent_name).collect();
        assert_eq!(under_new_build, ["Aris"], "the lane-bound seat stays rested across a deploy; the recital does not");
        assert!(!wake_at(&p, "Nobody"));
        // waking reads under the CURRENT build; write the record under it to test the word
        let cur = current_build();
        rest_at(&p, seat("Paige", cur)).expect("rest");
        assert!(wake_at(&p, "paige"), "case-insensitive, like the hold");
        assert!(resting_at(&p, cur).is_empty());
        std::fs::write(&p, "{not json").expect("corrupt");
        assert!(resting_at(&p, cur).is_empty(), "fail open");
    }
}
