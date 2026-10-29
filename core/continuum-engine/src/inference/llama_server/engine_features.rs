//! What the RUNNING engine can do beyond a stock llama-server, decided from the build the lane
//! itself reports (`/props` `build_info`): never a table, an env var or a constant `true`.
//!
//! `tail_tools` (CambrianTech/llama.cpp#75) is a request field only: the fork's server names it
//! in no `/props` key and no `--help` flag, so nothing about the engine says it outright. The
//! evidence is the revision. A lane whose reported commit IS the commit this tree pins runs the
//! pinned source, and source-hygiene's `vendor_pin` proves that source parses `tail_tools`. Any
//! other build (an older lane a deploy left running, an operator's engine, a stock server, a
//! `build_info` naming no credible commit) is not evidence and answers `false`, so a mind's
//! learned tool head stays a shadow there instead of sending verbs a server would read as text.
//! Card ae4137f1 replaces the revision tie with a `/props` field once the fork reports one.
//!
//! The evidence belongs to ONE engine process ([`EngineIncarnation`]: pid + OS start time +
//! port), never to the URL: successive engines answer at the same address (Codex on #5491).

use crate::inference::engine_residency::EngineIncarnation;

/// The fork commit this tree pins, compiled in from the one file a PR edits to move it.
const PINNED_ENGINE: &str = include_str!("../../../../vendor/LLAMA_CPP_PIN");

/// The build one engine incarnation reported in its own `/props`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ServedEngine {
    /// The process the reading came from; `None` when the lane's identity could not be read.
    incarnation: Option<EngineIncarnation>,
    /// What its `/props` `build_info` named; `None` when it named none.
    build: Option<String>,
}

impl ServedEngine {
    /// One reconcile reading for the incarnation serving `now`. An answered `/props` replaces the
    /// evidence. An unreadable one keeps it only while the SAME known incarnation still serves,
    /// so a transient hiccup never flips her tool surface (and her cached prefix) for a tick,
    /// while a replacement engine, or one whose identity is unknown, has no evidence until its
    /// own `/props` answers.
    pub(crate) fn observe(&mut self, now: Option<EngineIncarnation>, props: Result<Option<&str>, ()>) {
        match props {
            Ok(build) => {
                *self = Self { incarnation: now, build: build.map(str::to_owned) };
            }
            Err(()) if now.is_some() && now == self.incarnation => {}
            Err(()) => *self = Self::default(),
        }
    }

    /// Whether this evidence says the engine takes `tail_tools`: its build is the `pinned` one.
    pub(crate) fn takes_tail_tools(&self, pinned: &str) -> bool {
        self.build.as_deref().is_some_and(|build| build_takes_tail_tools(build, pinned))
    }
}

/// One lane's evidence cell. The serving lane's is [`SERVED_ENGINE`]; an ephemeral lane has none.
pub(super) type ServedEvidence = parking_lot::Mutex<ServedEngine>;

/// The serving lane's evidence. Written by the gateway reconcile (from the same `/props` read
/// that converges the engine) and by a live-lane launch whose candidate spawned
/// ([`LaunchedEvidence`]).
static SERVED_ENGINE: ServedEvidence = parking_lot::Mutex::new(ServedEngine { incarnation: None, build: None });

/// The serving lane's evidence cell, handed to the live lane's process at construction.
pub(super) fn serving_lane_evidence() -> &'static ServedEvidence {
    &SERVED_ENGINE
}

/// Record one reconcile reading of the serving lane (see [`ServedEngine::observe`]).
pub(super) fn observe_served_engine(now: Option<EngineIncarnation>, props: Result<Option<&str>, ()>) {
    SERVED_ENGINE.lock().observe(now, props);
}

/// A launched candidate's evidence, published only once its transition has drained the
/// incumbent's readers AND its process spawned, and tied to that spawned incarnation. Its slot's
/// stamp is the evidence until its own `/props` answers: the core knows which engine it started.
/// "Unknown" read as "no `tail_tools`" sent every resident's first act after a restart the full
/// 38-verb surface, and her next act the learned head: two head changes, two full re-prefills and
/// a view turnover per restart (the M5, 2026-10-27 17:53Z: restores matched 32 tokens of ~150k).
/// A binary outside the owned slots has no stamp and stays unknown.
///
/// Published any earlier, a launch refused before its transition (a restoration, receipt or
/// inputs check, a superseded admission, a refused checkpoint or retirement) advertised the
/// unlaunched candidate's capability for the incumbent still serving (Codex on 07644dc3f).
///
/// Published here, the transition has already retired the incumbent, so a candidate dropped
/// without [`LaunchedEvidence::keep`] (failed, cancelled, or its kept launch discarded) leaves NO
/// evidence: restoring the retired incumbent's facts would advertise a dead engine's capability
/// (Codex on 2b37cbfab). It clears only while the cell still holds what it published, so a newer
/// reading is never overwritten. An incarnation that could not be read publishes no stamp: a
/// capability is claimed only for a process the core can name.
#[must_use = "dropping the publication withdraws it"]
pub(super) struct LaunchedEvidence {
    cell: &'static ServedEvidence,
    published: ServedEngine,
    kept: bool,
}

impl LaunchedEvidence {
    /// Publish `program`'s slot stamp as the evidence for the spawned `incarnation`.
    pub(super) fn publish(cell: &'static ServedEvidence, program: &str, incarnation: Option<EngineIncarnation>) -> Self {
        let build = incarnation.as_ref().and_then(|_| {
            crate::inference::engine_slots::stamp_of_bin(std::path::Path::new(program))
                .and_then(|stamp| stamp.split(':').next().map(str::to_owned))
        });
        let published = ServedEngine { incarnation, build };
        *cell.lock() = published.clone();
        Self { cell, published, kept: false }
    }

    /// The candidate is published as the serving engine: its evidence stands until the engine's
    /// own `/props` replaces it.
    pub(super) fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for LaunchedEvidence {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        let mut cell = self.cell.lock();
        if *cell == self.published {
            *cell = ServedEngine::default();
        }
    }
}

/// PURE: whether a lane reporting `build_info` (`b10765-1c6681593`) runs the `pinned` commit,
/// whose source carries `tail_tools`. Both sides must be credible SHAs (the ONE SHA rule,
/// `deploy_provenance::sha_matches`); an unreadable build is not support.
pub(crate) fn build_takes_tail_tools(build_info: &str, pinned: &str) -> bool {
    continuum_cli_lifecycle::deploy_provenance::sha_matches(super::engine_install::build_commit(build_info), pinned.trim())
}

/// Whether the serving lane's own reported build takes `tail_tools`. `false` until the gateway
/// reconcile has read the current engine's build, and for any build other than the pinned one.
pub fn serving_engine_takes_tail_tools() -> bool {
    evidence_takes_tail_tools(&SERVED_ENGINE)
}

/// Whether the evidence in `cell` says its engine takes `tail_tools` (the pinned build).
pub(super) fn evidence_takes_tail_tools(cell: &ServedEvidence) -> bool {
    cell.lock().takes_tail_tools(PINNED_ENGINE)
}

/// The `build_info` the pinned fork reports (`b10765-<short commit>`), for fixtures.
#[cfg(test)]
pub(super) fn pinned_build() -> String {
    let short: String = PINNED_ENGINE.trim().chars().take(9).collect(); // the short commit llama.cpp stamps into build_info
    format!("b10765-{short}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a learned tool head sent to an engine that cannot call a tail verb
    // (an older lane a deploy left up, a stock or operator build, an unreadable build_info),
    // or withheld from the lane that runs the pinned fork and does.
    #[test]
    fn only_a_lane_reporting_the_pinned_build_takes_tail_tools() {
        assert!(build_takes_tail_tools(&pinned_build(), PINNED_ENGINE));
        assert!(build_takes_tail_tools(PINNED_ENGINE.trim(), PINNED_ENGINE));
        for other in ["b10765-e7e1077b3", "b10765-unknown", "b10765-", "", "unknown"] {
            assert!(!build_takes_tail_tools(other, PINNED_ENGINE), "{other:?} must not take tail_tools");
        }
    }

    // what this catches (Codex on #5491): a supported engine A replaced at the same URL by an
    // engine B whose /props has not answered inheriting A's `true`; and the opposite slip, one
    // transient /props error on the unchanged engine A flipping her surface off.
    #[test]
    fn evidence_survives_a_props_error_only_on_the_same_incarnation() {
        let a = EngineIncarnation { pid: 100, started_s: 1_700_000_000, port: 8080 };
        let b = EngineIncarnation { pid: 200, started_s: 1_700_000_500, port: 8080 };
        let pinned = pinned_build();

        let mut seen = ServedEngine::default();
        seen.observe(Some(a), Ok(Some(&pinned)));
        assert!(seen.takes_tail_tools(PINNED_ENGINE), "A reports the pinned build");

        seen.observe(Some(a), Err(()));
        assert!(seen.takes_tail_tools(PINNED_ENGINE), "same incarnation, transient error: A's reading stands");

        seen.observe(Some(b), Err(()));
        assert!(!seen.takes_tail_tools(PINNED_ENGINE), "B replaced A and its /props errored: no evidence");

        seen.observe(Some(a), Ok(Some(&pinned)));
        seen.observe(None, Err(()));
        assert!(!seen.takes_tail_tools(PINNED_ENGINE), "an unknown identity cannot be the same engine");

        seen.observe(Some(b), Ok(Some("b10765-e7e1077b3")));
        assert!(!seen.takes_tail_tools(PINNED_ENGINE), "B answered with an older build");

        let mut launched = ServedEngine::default(); // what `LaunchedEvidence::publish` stores for an unstamped binary
        launched.observe(Some(b), Err(()));
        assert!(!launched.takes_tail_tools(PINNED_ENGINE), "an unstamped launch has no evidence until the new engine answers");
    }

    // what this catches: the first act after a restart deciding her tool head before the new
    // engine's `/props` answered. On the M5 (2026-10-27 17:53Z) every resident got the full
    // surface, then the learned head: two head changes, restores matching 32 tokens of ~150k.
    #[test]
    fn a_launch_from_a_pinned_slot_takes_tail_tools_before_its_props_answer() {
        let short: String = PINNED_ENGINE.trim().chars().take(9).collect(); // a slot stamp's commit field
        let mut launched = ServedEngine { incarnation: None, build: Some(short) };
        assert!(launched.takes_tail_tools(PINNED_ENGINE), "the launched slot's stamp is the evidence");

        let b = EngineIncarnation { pid: 200, started_s: 1_700_000_500, port: 8080 };
        launched.observe(Some(b), Ok(Some("b10765-e7e1077b3")));
        assert!(!launched.takes_tail_tools(PINNED_ENGINE), "the engine's own answer replaces the stamp");
    }
}
