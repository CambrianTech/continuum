//! Self-determined attention allocation — the persona's steering wheel.
//!
//! Distinct from [`PersonaState`](super::types::PersonaState)'s `attention`,
//! which is *capacity* (depletes with work, recovers with rest — an economy the
//! substrate computes). `FocusState` is *allocation*: HOW the persona chooses to
//! spend the attention she has. A single self-set scalar of focus, a sticky
//! thread cursor, and time-boxed mutes. She tunes it sparsely ("choose your own
//! adventure" — not every turn); the substrate rides the chosen level between her
//! choices. It is never computed FOR her and never reads her output to steer her
//! ([[no-hardcoded-heuristics-to-steer-cognition]]); it only honors choices she
//! has made, and guarantees she is never blind to a direct address
//! ([[focus-is-self-allocation-not-siloing]]).
//!
//! ONE scalar feeds THREE surfaces — the interpretation lives in each *consumer*,
//! not here:
//!   1. RAG cross-thread breadth (`build_workspace_burst` / `compose_for_turn`),
//!   2. tool-catalog expansion (`render_tool_catalog`),
//!   3. servicing / wake cadence (`burst_fingerprint`).
//! Recipes PRESET the posture (e.g. a coding recipe ships a high focus, code-biased
//! cursor); she inherits the default and keeps the wheel.
//!
//! Persistence lives in airc per-(persona,room) state (#89); this is the in-memory
//! primitive + its honoring semantics, which are not blocked on that store.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

/// How far a mute navigates her ambient attention away from a lane. Mute is attention
/// allocation, never sensory shutoff: the inviolable interrupt floor (a direct address)
/// cuts through EVERY level — "I don't turn off my eyes and ears." The levels differ
/// only in how hard the lane's ambient chatter is down-weighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MuteLevel {
    /// Turn part-way from a lane's ambient / backlog chatter — its pull on her attention
    /// drops but she keeps some peripheral awareness. A direct address still reaches her:
    /// "turn from the chatter, not deaf to the alarm."
    Soft,
    /// Turn fully from a lane's ambient chatter — the allocation kernel pools no ambient
    /// attention there at all. She is still NOT blind to it: a direct address pierces (the
    /// inviolable interrupt floor). A sane mind navigates away from noise; it does not go
    /// numb to it.
    Hard,
}

/// One muted lane. `expires_at_ms == None` = held until she un-mutes; `Some(t)` =
/// a snooze that auto-restores awareness at unix-ms `t` so she never has to
/// remember to un-mute (the substrate reasserts "never blind" on lapse).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mute {
    pub lane: Uuid,
    pub level: MuteLevel,
    pub expires_at_ms: Option<u64>,
}

impl Mute {
    /// Active at `now_ms`? A snooze whose deadline has passed is NOT active.
    pub fn is_active(&self, now_ms: u64) -> bool {
        match self.expires_at_ms {
            None => true,
            Some(t) => now_ms < t,
        }
    }
}

/// The persona's self-determined attention allocation. Held state, tuned sparsely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusState {
    /// The focus scalar, clamped to `0.0..=1.0`. HIGH = tight / this-thread-deep
    /// (narrow breadth, fewer cross-thread items, deeper single tool category,
    /// rare backlog reminders); LOW = associative (wide cross-thread mixing,
    /// broad catalog, ambient awareness). One dial — each surface reads it.
    focus: f32,
    /// The sticky thread/room cursor — the lane currently in focus. `None` until
    /// she settles on one. The breadth gradient is measured *relative* to this lane.
    cursor: Option<Uuid>,
    /// Time-boxed (or held) per-lane mutes; at most one entry per lane.
    mutes: Vec<Mute>,
}

/// The resting focus setpoint — the balanced posture every consumer anchors on
/// (the cognition focus-policy junction returns its calibrated constants exactly
/// here). Single-sourced; the future genome-borne trait baseline replaces it.
pub const RESTING_FOCUS: f32 = 0.5;

impl Default for FocusState {
    fn default() -> Self {
        // Balanced posture: neither heads-down nor maximally associative, no lane
        // chosen yet, nothing muted. A recipe may PRESET a different default; she
        // overrides from there.
        Self {
            focus: RESTING_FOCUS,
            cursor: None,
            mutes: Vec::new(),
        }
    }
}

impl FocusState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current focus scalar (`0.0..=1.0`).
    pub fn focus(&self) -> f32 {
        self.focus
    }

    /// Set the focus scalar; clamped to `0.0..=1.0` (a value outside the range is
    /// her intent saturated, not an error — clamping is the contract, not a
    /// silent fallback over a missing precondition).
    pub fn set_focus(&mut self, focus: f32) {
        self.focus = focus.clamp(0.0, 1.0);
    }

    /// Return ONLY the focus scalar to its resting setpoint (the value [`Default`]
    /// carries — single-sourced here, the future genome-borne trait baseline replaces
    /// it). Cursor and mutes are independent steering inputs and are left untouched:
    /// "return my concentration to rest" is not "forget where I was / un-hush
    /// everything". The homeostatic counterpart to a `focus/nudge` lean.
    pub fn reset_focus(&mut self) {
        self.focus = Self::default().focus;
    }

    /// The lane currently in focus, if she has settled on one.
    pub fn cursor(&self) -> Option<Uuid> {
        self.cursor
    }

    /// Move the sticky cursor to `lane`.
    pub fn set_cursor(&mut self, lane: Uuid) {
        self.cursor = Some(lane);
    }

    /// Drop the cursor (back to no single lane in focus).
    pub fn clear_cursor(&mut self) {
        self.cursor = None;
    }

    /// Mute `lane` at `level`, optionally as a snooze expiring at `expires_at_ms`.
    /// Replaces any existing mute for the same lane (one mute per lane — re-muting
    /// updates level/duration rather than stacking; [[compression-principle]]).
    pub fn mute(&mut self, lane: Uuid, level: MuteLevel, expires_at_ms: Option<u64>) {
        self.mutes.retain(|m| m.lane != lane);
        self.mutes.push(Mute {
            lane,
            level,
            expires_at_ms,
        });
    }

    /// Un-mute `lane` immediately (regardless of any remaining snooze).
    pub fn unmute(&mut self, lane: Uuid) {
        self.mutes.retain(|m| m.lane != lane);
    }

    /// Drop expired snoozes — housekeeping so the mute list stays bounded. Pure
    /// effect on inactive entries; active mutes are untouched.
    pub fn prune_expired(&mut self, now_ms: u64) {
        self.mutes.retain(|m| m.is_active(now_ms));
    }

    /// The currently-active mute level for `lane` at `now_ms` (`None` if unmuted
    /// or the snooze has lapsed).
    pub fn active_mute(&self, lane: Uuid, now_ms: u64) -> Option<MuteLevel> {
        self.mutes
            .iter()
            .find(|m| m.lane == lane && m.is_active(now_ms))
            .map(|m| m.level)
    }

    /// THE wake floor: should a *change* on `lane` wake her this slice?
    ///
    /// This honors a choice SHE made (the mute) plus a structural fact (`addressed`
    /// — derived elsewhere from the actual content via the identity-aware `mentions`
    /// primitive). It gates scheduling, not her decision, and never reads her output
    /// ([[no-hardcoded-heuristics-to-steer-cognition]]):
    ///   * **A direct address** → ALWAYS wakes her. The interrupt floor is INVIOLABLE:
    ///     "I don't turn off my eyes and ears." A sane mind navigates away from noise; it
    ///     never goes numb to it — so not even her own self-set mute can blind her to a
    ///     direct address. This is a codified protection that keeps the mind sane
    ///     ([[commands-are-agency-algs-are-pathways]]), the boundary her agency lives
    ///     within.
    ///   * **A muted lane's ambient change** (Soft *or* Hard) → does NOT wake her: she
    ///     has navigated her attention away from that lane's chatter. The two levels
    ///     differ only in how far the allocation kernel pools ambient attention off the
    ///     lane, never in whether an address pierces. Mute is attention allocation, never
    ///     sensory shutoff.
    ///   * **No active mute** → always wakes on change.
    pub fn wakes_on(&self, lane: Uuid, addressed: bool, now_ms: u64) -> bool {
        if addressed {
            return true;
        }
        self.active_mute(lane, now_ms).is_none()
    }

    /// Materialize the focus KERNEL: a normalized allocation `a` over `threads`, the
    /// single object the design converged on — *the same `a`* is read as felt RAG
    /// breadth (the salience gradient a perception composer applies) and as the
    /// compute/time schedule (stride weights). Each thread is supplied with its base
    /// `relevance` (≥ 0 — the salience the substrate already surfaced: recency,
    /// addressing, her-own-work, recall match); this turns that raw salience into a
    /// share distribution shaped by HER state.
    ///
    /// The focus scalar is the **concentration** (an inverse-temperature, not merely
    /// "breadth"): `0.0` → near-uniform (associative, wide cross-thread bleed — the
    /// post-endorphin broad pole), `1.0` → peaked on the cursor / highest-relevance
    /// thread (heads-down — the locked-in pole). The sticky `cursor` gets a proximity
    /// bonus so that rising focus collapses toward HER chosen lane, not merely the
    /// loudest one. A **hard-muted** lane (at `now_ms`) is excluded from *ambient*
    /// allocation — weight `0.0`, outside the normalization — UNLESS it is her `cursor`:
    /// a lane she has actively turned toward is no longer ambient, so her own focus
    /// pierces the mute. That is the allocation-surface twin of the inviolable interrupt
    /// floor in [`wakes_on`] (mute is attention allocation, never sensory shutoff — she
    /// is never numb to a lane she or another has brought to the fore). (Soft mute's
    /// allocation effect is the caller's: it passes that lane's relevance net of ambient
    /// — soft mute's load-bearing job is the wake floor, not allocation. Address-level
    /// piercing of an ambient hard mute arrives when `allocate`'s perception consumer
    /// lands — multi-lane #43 — supplying addressing-bearing relevance per lane.)
    ///
    /// Returns one [`ThreadShare`] per input thread, in input order, weights summing to
    /// ~1.0 (or all `0.0` only if every thread is hard-muted — a degenerate, honest
    /// "nothing to allocate", never a silent fallback). Pure: this is exactly where a
    /// trained policy later replaces the hand-set scalar — same signature, learned
    /// concentration ([[no-hardcoded-heuristics-to-steer-cognition]]).
    pub fn allocate(&self, threads: &[(Uuid, f32)], now_ms: u64) -> Vec<ThreadShare> {
        // gain = the softmax inverse-temperature. β=0 → 0 (every score collapses to
        // exp(0)=1 → uniform → maximally associative); β=1 → MAX_GAIN (the top score
        // dominates → peaked). Monotonic in the scalar, so concentration rises smoothly.
        let gain = MAX_GAIN * self.focus;

        // Score each thread: base relevance + a cursor proximity bonus (binary today —
        // "is this the lane she settled on"; a semantic thread-distance can replace the
        // indicator later without touching the kernel's shape). A hard-muted lane scores
        // NaN as a sentinel for "excluded" so it never enters the normalization — but her
        // cursor pierces a hard mute (a lane she has turned toward is not ambient), the
        // allocation twin of the inviolable wake floor.
        let scores: Vec<f32> = threads
            .iter()
            .map(|(id, relevance)| {
                if self.cursor != Some(*id)
                    && self.active_mute(*id, now_ms) == Some(MuteLevel::Hard)
                {
                    f32::NAN
                } else {
                    let cursor_bonus = if self.cursor == Some(*id) {
                        CURSOR_BONUS
                    } else {
                        0.0
                    };
                    gain * (relevance.max(0.0) + cursor_bonus)
                }
            })
            .collect();

        // Softmax with the standard max-shift for numerical stability (gain*score can be
        // large at high focus). Excluded (NaN) lanes are skipped from the max and the sum.
        let max_score = scores
            .iter()
            .copied()
            .filter(|s| !s.is_nan())
            .fold(f32::NEG_INFINITY, f32::max);

        let exps: Vec<f32> = scores
            .iter()
            .map(|&s| {
                if s.is_nan() {
                    0.0
                } else {
                    (s - max_score).exp()
                }
            })
            .collect();
        let sum: f32 = exps.iter().sum();

        threads
            .iter()
            .zip(exps)
            .map(|((id, _), e)| ThreadShare {
                thread: *id,
                // sum == 0 only when every lane is excluded; then weight is honestly 0.0.
                weight: if sum > 0.0 { e / sum } else { 0.0 },
            })
            .collect()
    }
}

/// The softmax gain at full focus (`β = 1.0`). With base relevances on a ~`0..1` scale
/// and a unit [`CURSOR_BONUS`], a gain of 8 makes the focused lane dominate by ≈ e⁸
/// (~3000×) — effectively heads-down — while `β = 0.5` (gain 4, ~55×) keeps the others
/// present. Tuned to the relevance scale, not a magic constant: change it with the scale.
const MAX_GAIN: f32 = 8.0;

/// The score bonus the sticky cursor lane receives, on the same scale as base relevance.
/// Unity means "as salient as a maximally-relevant thread" — so once she settles a
/// cursor, rising focus concentrates on HER lane rather than the loudest one.
const CURSOR_BONUS: f32 = 1.0;

/// One thread's share of the focus kernel — its slice of `a`. The substrate reads this
/// as both perceptual breadth weight and compute/time weight (the two projections of the
/// one allocation). Internal substrate math, not a wire type (no `TS` until a command
/// surfaces it).
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadShare {
    pub thread: Uuid,
    /// Normalized allocation weight in `0.0..=1.0`; the returned set sums to ~1.0.
    pub weight: f32,
}

/// Process-global per-persona focus registry — the by-`persona_id` seam.
///
/// `FocusState` lives behind no global handle on the brain ([`PersonaCognition`] is
/// reached only through the serve loop's mutex), but self-determination needs TWO
/// reachers of the SAME state: the never-stop serve loop (reads the wake floor) and a
/// self-set TOOL she invokes through the command registry (writes the scalar / mutes),
/// which knows only her `persona_id`. This registry is that seam — one `FocusState`
/// per persona, resolved by id from both sides. It is NOT a parallel copy of brain
/// state; it is the single home for focus, the same way `persona_workspace::global()`
/// is the single home for a persona's mind. #89 (airc per-(persona,room) state)
/// persists behind this same handle later; the in-memory authority lives here.
#[derive(Default)]
pub struct FocusRegistry {
    states: Mutex<HashMap<Uuid, Arc<Mutex<FocusState>>>>,
}

impl FocusRegistry {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve (get-or-create) the persona's focus handle. Idempotent: the first
    /// caller for a persona installs a default `FocusState` (balanced, no mutes — the
    /// correct birth posture, not a silent fallback over a missing precondition); every
    /// later caller — serve loop, tool — gets the SAME `Arc`, so her steering and the
    /// wake floor read one state. A poisoned lock is a prior panic mid-mutation and is
    /// propagated (fail loud), never swallowed.
    pub fn handle(&self, persona_id: Uuid) -> Arc<Mutex<FocusState>> {
        self.states
            .lock()
            .expect("focus registry mutex poisoned by a prior panic")
            .entry(persona_id)
            .or_insert_with(|| Arc::new(Mutex::new(FocusState::new())))
            .clone()
    }

    /// Peek the handle without creating one — `None` if she has never touched focus.
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<Mutex<FocusState>>> {
        self.states
            .lock()
            .expect("focus registry mutex poisoned by a prior panic")
            .get(persona_id)
            .cloned()
    }
}

/// Process-global focus registry. Same `OnceLock` shape as
/// `persona_workspace::global()` — the shared seam between the serve loop that honors
/// focus and the tool that sets it.
pub fn registry() -> Arc<FocusRegistry> {
    static GLOBAL: OnceLock<Arc<FocusRegistry>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(FocusRegistry::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane() -> Uuid {
        Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888)
    }

    // what this catches: a focus value outside [0,1] saturates to the range
    // (her intent clamped — the contract), never escapes it to corrupt a consumer.
    #[test]
    fn focus_clamps_to_unit_range() {
        let mut f = FocusState::new();
        f.set_focus(2.5);
        assert_eq!(f.focus(), 1.0);
        f.set_focus(-0.3);
        assert_eq!(f.focus(), 0.0);
        f.set_focus(0.7);
        assert!((f.focus() - 0.7).abs() < f32::EPSILON);
    }

    // what this catches: re-muting a lane REPLACES rather than stacks (one mute
    // per lane), and unmute clears it.
    #[test]
    fn one_mute_per_lane_replace_then_clear() {
        let mut f = FocusState::new();
        f.mute(lane(), MuteLevel::Soft, None);
        f.mute(lane(), MuteLevel::Hard, Some(1_000));
        assert_eq!(f.mutes.len(), 1);
        assert_eq!(f.active_mute(lane(), 0), Some(MuteLevel::Hard));
        f.unmute(lane());
        assert_eq!(f.active_mute(lane(), 0), None);
    }

    // what this catches: a snooze auto-restores awareness once its deadline passes
    // — the substrate reasserts "never blind" without her un-muting by hand.
    #[test]
    fn snooze_auto_expires() {
        let mut f = FocusState::new();
        f.mute(lane(), MuteLevel::Hard, Some(1_000));
        assert_eq!(f.active_mute(lane(), 999), Some(MuteLevel::Hard));
        assert_eq!(f.active_mute(lane(), 1_000), None); // boundary: deadline reached
        assert_eq!(f.active_mute(lane(), 5_000), None);
    }

    // what this catches: the INVIOLABLE interrupt floor — a direct address always wakes
    // her at EVERY mute level ("I don't turn off my eyes and ears"); both soft and hard
    // suppress only ambient (non-addressed) change; an expired or absent mute always
    // wakes. This is the load-bearing "navigate away without going numb" invariant — not
    // even her own self-set hard mute can blind her to a direct address.
    #[test]
    fn wake_floor_honors_mute_level_and_address() {
        let mut f = FocusState::new();
        // unmuted: always wakes
        assert!(f.wakes_on(lane(), false, 0));
        assert!(f.wakes_on(lane(), true, 0));
        // soft: ambient suppressed, address cuts through
        f.mute(lane(), MuteLevel::Soft, None);
        assert!(!f.wakes_on(lane(), false, 0));
        assert!(f.wakes_on(lane(), true, 0));
        // hard: ambient suppressed MORE, but the address still pierces — never numb
        f.mute(lane(), MuteLevel::Hard, Some(1_000));
        assert!(!f.wakes_on(lane(), false, 0));
        assert!(f.wakes_on(lane(), true, 0));
        // ...and once the snooze lapses, even ambient change wakes her again
        assert!(f.wakes_on(lane(), false, 1_000));
    }

    // what this catches: prune drops only lapsed snoozes, leaving held + still-live
    // mutes intact (bounded list, no accidental un-mute of an active lane).
    #[test]
    fn prune_drops_only_expired() {
        let mut f = FocusState::new();
        let held = Uuid::from_u128(1);
        let live = Uuid::from_u128(2);
        let dead = Uuid::from_u128(3);
        f.mute(held, MuteLevel::Soft, None);
        f.mute(live, MuteLevel::Soft, Some(10_000));
        f.mute(dead, MuteLevel::Soft, Some(100));
        f.prune_expired(5_000);
        assert_eq!(f.mutes.len(), 2);
        assert!(f.active_mute(held, 5_000).is_some());
        assert!(f.active_mute(live, 5_000).is_some());
        assert!(f.active_mute(dead, 5_000).is_none());
    }

    fn ids() -> (Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(0xA),
            Uuid::from_u128(0xB),
            Uuid::from_u128(0xC),
        )
    }

    // what this catches: β=0 (maximally associative) yields a UNIFORM allocation —
    // every thread an equal share regardless of relevance or cursor. The broad pole:
    // no concentration at all.
    #[test]
    fn kernel_is_uniform_at_zero_focus() {
        let (a, b, c) = ids();
        let mut f = FocusState::new();
        f.set_focus(0.0);
        f.set_cursor(a); // cursor present but must NOT pull at β=0
        let shares = f.allocate(&[(a, 0.9), (b, 0.1), (c, 0.0)], 0);
        for s in &shares {
            assert!(
                (s.weight - 1.0 / 3.0).abs() < 1e-5,
                "β=0 → uniform, got {}",
                s.weight
            );
        }
    }

    // what this catches: β=1 (locked in) collapses almost all mass onto the CURSOR lane
    // — even when another thread is louder (higher relevance), the cursor bonus means
    // rising focus concentrates on HER chosen lane, not the loudest one.
    #[test]
    fn kernel_peaks_on_cursor_at_full_focus() {
        let (a, b, c) = ids();
        let mut f = FocusState::new();
        f.set_focus(1.0);
        f.set_cursor(a);
        // b is louder than a by relevance, but a is the cursor.
        let shares = f.allocate(&[(a, 0.3), (b, 0.9), (c, 0.0)], 0);
        let total: f32 = shares.iter().map(|s| s.weight).sum();
        assert!((total - 1.0).abs() < 1e-4, "normalized");
        let wa = shares.iter().find(|s| s.thread == a).unwrap().weight;
        assert!(wa > 0.8, "cursor lane dominates at full focus, got {wa}");
    }

    // what this catches: higher focus is MORE concentrated — the cursor's share rises
    // monotonically as β climbs (the dial actually tightens the kernel).
    #[test]
    fn kernel_concentration_rises_with_focus() {
        let (a, b, c) = ids();
        let weight_on_cursor = |beta: f32| {
            let mut f = FocusState::new();
            f.set_focus(beta);
            f.set_cursor(a);
            f.allocate(&[(a, 0.5), (b, 0.5), (c, 0.5)], 0)
                .iter()
                .find(|s| s.thread == a)
                .unwrap()
                .weight
        };
        let (low, mid, high) = (weight_on_cursor(0.1), weight_on_cursor(0.5), weight_on_cursor(0.9));
        assert!(low < mid && mid < high, "concentration rises: {low} < {mid} < {high}");
    }

    // what this catches: a HARD-muted lane is excluded from the kernel (weight 0,
    // outside the normalization) while the surviving lanes still sum to 1 — the mute
    // removes a thread from allocation, it doesn't just down-weight it.
    #[test]
    fn kernel_excludes_hard_muted_lane() {
        let (a, b, c) = ids();
        let mut f = FocusState::new();
        f.set_focus(0.5);
        f.mute(b, MuteLevel::Hard, None);
        let shares = f.allocate(&[(a, 0.5), (b, 0.9), (c, 0.5)], 0);
        let wb = shares.iter().find(|s| s.thread == b).unwrap().weight;
        assert_eq!(wb, 0.0, "hard-muted lane gets no allocation");
        let total: f32 = shares.iter().map(|s| s.weight).sum();
        assert!((total - 1.0).abs() < 1e-4, "survivors still normalize to 1");
    }

    // what this catches: degenerate inputs are honest, never a silent fallback —
    // empty thread set → empty allocation; all-hard-muted → all-zero (nothing to
    // allocate), not a fabricated uniform.
    #[test]
    fn kernel_degenerate_inputs_are_honest() {
        let (a, b, _) = ids();
        let mut f = FocusState::new();
        assert!(f.allocate(&[], 0).is_empty(), "no threads → no shares");

        f.mute(a, MuteLevel::Hard, None);
        f.mute(b, MuteLevel::Hard, None);
        let shares = f.allocate(&[(a, 0.5), (b, 0.5)], 0);
        assert!(
            shares.iter().all(|s| s.weight == 0.0),
            "all lanes muted → all-zero, not a fabricated distribution"
        );
    }

    // what this catches: with no cursor settled, focus concentrates on the highest-
    // RELEVANCE thread (the loudest), so the dial still works before she picks a lane.
    #[test]
    fn kernel_without_cursor_follows_relevance() {
        let (a, b, c) = ids();
        let mut f = FocusState::new();
        f.set_focus(0.9); // no cursor
        let shares = f.allocate(&[(a, 0.1), (b, 0.95), (c, 0.2)], 0);
        let top = shares
            .iter()
            .max_by(|x, y| x.weight.partial_cmp(&y.weight).unwrap())
            .unwrap();
        assert_eq!(top.thread, b, "highest relevance wins absent a cursor");
    }

    // what this catches: the registry hands the SAME Arc back for one persona (serve
    // loop + tool mutate one shared state), while distinct personas stay independent.
    // This is the load-bearing "she steers it, the loop honors it" seam.
    #[test]
    fn registry_shares_one_handle_per_persona() {
        let reg = FocusRegistry::new();
        let p = Uuid::from_u128(0xAA);
        let q = Uuid::from_u128(0xBB);

        let h1 = reg.handle(p);
        h1.lock().unwrap().set_focus(0.9);
        let h2 = reg.handle(p); // same persona → same state
        assert!(Arc::ptr_eq(&h1, &h2));
        assert!((h2.lock().unwrap().focus() - 0.9).abs() < f32::EPSILON);

        // a distinct persona is untouched (its own default), and `get` sees what
        // `handle` installed.
        let q_focus = reg.handle(q).lock().unwrap().focus();
        assert!((q_focus - 0.5).abs() < f32::EPSILON);
        assert!(reg.get(&p).is_some());
        assert!(reg.get(&Uuid::from_u128(0xCC)).is_none());
    }
}
