//! `FacultyPulse` — the glass-box "which faculty is firing" signal.
//!
//! The four cognition axes a persona tile draws as a live compass — **Focus,
//! Reason, Recall, Act** — each carry a `0..=100` activation level that a
//! cognition-stage tap bumps toward full and that DECAYS with wall-clock. The
//! vitals radiator ([`crate::ipc::vitals_emitter`]) samples the decayed levels
//! every couple seconds and folds them into the persona's roster vitals, so the
//! tile shows a live-but-smooth "shape of the mind": Reason bright while she
//! deliberates, Act bright while she runs tools, fading back to dark at rest.
//!
//! This is PURE OBSERVABILITY. Nothing in any decision path ever reads a pulse;
//! it is a read-tap on faculties the workspace already runs + times, radiated
//! outward ([[design-the-persona-as-a-being]], OBSERVABILITY-AS-SUBSTRATE). The
//! instantaneous per-event FLASH is a separate lane (the stream rail); this is
//! the steady-state glow the flash rides on.
//!
//! Interior-mutable (a `std::sync::Mutex` over four small cells): the living
//! cycle is shared via `&self`, bumped from the tick seam, sampled by the
//! radiator. The lock is held only for a cheap read/write, never across an await.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::workspace::FacultyId;

/// The four axes a persona tile's cognition compass draws. Named to the tile, not
/// to the internal [`FacultyId`] set — [`CognitionAxis::of`] maps the faculties the
/// workspace actually runs onto these four display axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionAxis {
    /// Attention / world-model — orienting on the room as it is now.
    Focus,
    /// The reasoner (deliberation) — producing the turn's decision.
    Reason,
    /// Hippocampal recall — surfacing memory.
    Recall,
    /// Acting: tool execution, the hands moving.
    Act,
}

impl CognitionAxis {
    /// Every axis, in the compass order the tile draws (N/E/S/W).
    pub const ALL: [CognitionAxis; 4] = [
        CognitionAxis::Focus,
        CognitionAxis::Reason,
        CognitionAxis::Recall,
        CognitionAxis::Act,
    ];

    /// Index into a 4-cell pulse array. Stable — the vitals key order depends on it.
    fn idx(self) -> usize {
        match self {
            CognitionAxis::Focus => 0,
            CognitionAxis::Reason => 1,
            CognitionAxis::Recall => 2,
            CognitionAxis::Act => 3,
        }
    }

    /// The open-vocabulary vitals-map key this axis radiates under (folded into
    /// `RosterSlotView.vitals`; the tile's `cognitionDiamond` reads these exact keys).
    pub fn vital_key(self) -> &'static str {
        match self {
            CognitionAxis::Focus => "focus",
            CognitionAxis::Reason => "reason",
            CognitionAxis::Recall => "recall",
            CognitionAxis::Act => "act",
        }
    }

    /// Map a faculty the workspace ran onto its display axis — `None` for faculties
    /// with no compass home (Affect/Volition/Salience are neuromodulatory/scheduling,
    /// not one of the four the tile draws). Deliberation is the reasoner; Recall is
    /// recall; WorldModel is the persona's orienting attention → Focus. Act has no
    /// `FacultyId` (the hands run after deliberation) — it is bumped explicitly by the
    /// acting seam via [`FacultyPulse::note`], not through this map.
    pub fn of(faculty: &FacultyId) -> Option<CognitionAxis> {
        match faculty {
            FacultyId::Recall => Some(CognitionAxis::Recall),
            FacultyId::Deliberation => Some(CognitionAxis::Reason),
            FacultyId::WorldModel => Some(CognitionAxis::Focus),
            FacultyId::Affect | FacultyId::Volition | FacultyId::Salience => None,
            FacultyId::Custom(_) => None,
        }
    }
}

/// How fast a bumped axis fades, in level-points per second. A full bump (100)
/// decays to dark in ~2.5s — long enough that the 2s radiator sample catches the
/// glow, short enough that the compass visibly settles between turns. Calibrated to
/// the emit cadence, never guessed blind ([[never-blind-feedback-driven-iteration]]).
const DECAY_PER_SEC: f32 = 40.0;

/// One axis cell: its level at the last bump + when that bump happened, so the
/// current level is derived by decay at read time (no background ticker needed).
#[derive(Clone, Copy)]
struct Cell {
    level_at_bump: f32,
    bumped: Instant,
}

impl Cell {
    fn decayed(&self, now: Instant) -> f32 {
        let dt = now.saturating_duration_since(self.bumped).as_secs_f32();
        (self.level_at_bump - DECAY_PER_SEC * dt).max(0.0)
    }
}

/// A decaying four-axis activation accumulator — one per living [`WorkspaceCycle`].
pub struct FacultyPulse {
    cells: Mutex<[Cell; 4]>,
}

impl Default for FacultyPulse {
    fn default() -> Self {
        Self::new()
    }
}

impl FacultyPulse {
    pub fn new() -> Self {
        let now = Instant::now();
        let seed = Cell { level_at_bump: 0.0, bumped: now };
        Self { cells: Mutex::new([seed; 4]) }
    }

    /// Bump an axis toward `level` (0..=100). Takes the MAX of the current decayed
    /// value and the new level, so a fresh fire never dims an already-brighter axis;
    /// resets that axis's decay clock. Cheap; the lock is never held across an await.
    pub fn note(&self, axis: CognitionAxis, level: u8) {
        let now = Instant::now();
        let l = level.min(100) as f32;
        // A poisoned lock (a bumper panicked mid-write) must not brick observability
        // for the process — recover the guard; a stale cell is a cosmetic glitch, not
        // a correctness fault.
        let mut cells = self.cells.lock().unwrap_or_else(|e| e.into_inner());
        let cell = &mut cells[axis.idx()];
        cell.level_at_bump = cell.decayed(now).max(l);
        cell.bumped = now;
    }

    /// Bump an axis to FULL (100) — the common case for a discrete stage firing.
    pub fn fire(&self, axis: CognitionAxis) {
        self.note(axis, 100);
    }

    /// The current decayed level per axis, in [`CognitionAxis::ALL`] order. What the
    /// radiator samples. Read-only (decay is derived, not stored back).
    pub fn levels(&self) -> [u8; 4] {
        let now = Instant::now();
        let cells = self.cells.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = [0u8; 4];
        for axis in CognitionAxis::ALL {
            out[axis.idx()] = cells[axis.idx()].decayed(now).round() as u8;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the accumulator must (a) light an axis on a fire, (b) map
    // each faculty to its compass axis, and (c) decay toward dark over wall-clock —
    // the three properties the live tile depends on. A pulse that never decayed would
    // pin the compass bright forever; one that dropped the FacultyId→axis map would
    // light the wrong triangle.
    #[test]
    fn fires_maps_and_decays() {
        let pulse = FacultyPulse::new();
        // dark at rest
        assert_eq!(pulse.levels(), [0, 0, 0, 0]);

        // a fire lights exactly its axis to full
        pulse.fire(CognitionAxis::Reason);
        let l = pulse.levels();
        assert_eq!(l[1], 100, "Reason (idx 1) full after fire");
        assert_eq!(l[0] + l[2] + l[3], 0, "no other axis lit");

        // faculty → axis mapping the tick seam relies on
        assert_eq!(CognitionAxis::of(&FacultyId::Recall), Some(CognitionAxis::Recall));
        assert_eq!(CognitionAxis::of(&FacultyId::Deliberation), Some(CognitionAxis::Reason));
        assert_eq!(CognitionAxis::of(&FacultyId::WorldModel), Some(CognitionAxis::Focus));
        assert_eq!(CognitionAxis::of(&FacultyId::Affect), None);

        // max-not-overwrite: a weaker note never dims a brighter live axis
        pulse.note(CognitionAxis::Reason, 20);
        assert_eq!(pulse.levels()[1], 100, "weaker note does not dim the bright axis");

        // decays toward dark: simulate elapsed time by seeding a past bump
        let past = FacultyPulse::new();
        {
            let mut cells = past.cells.lock().unwrap();
            cells[1] = Cell {
                level_at_bump: 100.0,
                bumped: Instant::now() - Duration::from_secs(3),
            };
        }
        assert_eq!(past.levels()[1], 0, "100 fades to 0 after ~3s at 40/s");
    }

    // what this catches: the vital-key vocabulary must match what the tile's
    // cognitionDiamond reads (focus/reason/recall/act) — a rename here silently
    // darkens the compass.
    #[test]
    fn vital_keys_are_the_tile_vocabulary() {
        assert_eq!(CognitionAxis::Focus.vital_key(), "focus");
        assert_eq!(CognitionAxis::Reason.vital_key(), "reason");
        assert_eq!(CognitionAxis::Recall.vital_key(), "recall");
        assert_eq!(CognitionAxis::Act.vital_key(), "act");
    }
}
