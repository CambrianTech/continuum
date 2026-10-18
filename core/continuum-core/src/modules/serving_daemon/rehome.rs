//! Re-home evidence: when a live lane is worth relaunching. The relative gain margin,
//! the sustained-tick streak, the cooldown, the hold override, the knee and the slot
//! shortfall, the paging refusal, the settle edge, and the model-change gate — all pure
//! decisions over numbers, with their measured constants.

use super::*;

/// The margin, in percent, by which the plan must exceed the LIVE lane before a
/// re-home is worth killing in-flight turns for.
///
/// RELATIVE, not absolute: the same 2048-token shortfall is a third of a small
/// lane and noise on a large one, so an absolute bar means something different at
/// every window size.
///
/// MEASURED against 6,322 live `serving.plan` samples (probes.jsonl, 2026-08-06),
/// after BigMama flagged that she had proposed 15% from the shape of the problem
/// with no data behind it: consecutive-sample plan jitter runs p50 0%, p90 1%, with
/// a fat tail (p99 22%, max 106%). 15% sits an order of magnitude above the p90
/// noise floor while staying far under the 60% shortfall of the real incident.
///
/// Note which parameter is actually load-bearing, because it is NOT this one: at a
/// 10%, 15% OR 20% bar the longest run of consecutive qualifying rises in the whole
/// sample was 2. The jitter tail is a SPIKE, not a sustained climb, so
/// [`REHOME_SUSTAINED_TICKS`] is what rejects it and this margin only has to clear
/// the p90. Widen it only with evidence; loosening the tick count is the change that
/// would actually let noise through.
pub(super) const REHOME_MIN_GAIN_PCT: u32 = 15;
/// A steady hold (an eval, a detached solve) suppresses the OPTIONAL grow-back — a
/// re-home that would buy a few percent is not worth dropping a held slot for. It does
/// not get to pin a lane serving at a FRACTION of its plan: measured 2026-09-17 07:5xZ
/// on the M5, the boot plan came up 8 lanes × 12,800 under the outgoing server's memory
/// (card 40f53419), the plan wanted 3 × 67,340, and four re-fired control solves held
/// the lane steady for 255 ticks in a row — every one of the 16 minds, the solves
/// included, on a 12.8k window because the solves were "protecting" it. Past this gain
/// the re-home goes ahead and the probe says so; the held work retries its flickered
/// act (the deliberation retry budget exists for exactly one relaunch).
pub(super) const REHOME_HOLD_OVERRIDE_PCT: u32 = 75;
/// A hold DELAYS a smaller re-home, it never denies it: after this many consecutive
/// held ticks (5 s each — about one act of held work) a sustained re-home goes ahead
/// anyway. Measured 2026-09-17 08:3xZ: with the window restored the knee moved 3 → 4
/// and the plan wanted a fourth lane (+33%, under the override), and the re-fired
/// solves' hold kept 16 minds on 3 lanes — the health line read STARVED for an hour
/// while the planner "owed lanes" it had already planned.
pub(super) const REHOME_HOLD_DEFER_TICKS: u32 = 24;

/// How many consecutive ticks the plan must exceed the lane by
/// [`REHOME_MIN_GAIN_PCT`] before a re-home is justified.
///
/// Sustained-ness is the anti-thrash property the old single-sample `live * 2 <=
/// plan` ratio was reaching for, stated directly — and it is deliberately SHORT.
/// It is not what limits the relaunch RATE (that is [`REHOME_COOLDOWN_TICKS`],
/// enforced independently below); it only has to outlast jitter.
///
/// THE load-bearing parameter, and measured as such: across 6,322 live
/// `serving.plan` samples (probes.jsonl, 2026-08-06) the longest run of consecutive
/// qualifying rises was **2**, at a 10%, 15% or 20% margin alike — zero runs of 3 in
/// the entire sample. Real plan jitter spikes for a sample or two and falls back; it
/// does not climb. So 3 is the first value that provably rejects every observed
/// noise event, and 2 would not have. A dip RESETS the streak, so noise cannot
/// accumulate, while a genuine capacity change is honoured in 15s instead of being
/// stranded forever.
///
/// Lowering this to 2 re-admits the entire measured jitter tail. Do not, without
/// re-running that measurement against fresher receipts.
pub(super) const REHOME_SUSTAINED_TICKS: u32 = 3;

/// PURE: record `plan` among the last [`REHOME_SUSTAINED_TICKS`] plans and answer whether it is
/// their high. Bounded by the evidence span, so a high ages out with the evidence it came from
/// and a settled plan's own best tick is always reachable.
pub(super) fn at_recent_plan_high(recent: &mut std::collections::VecDeque<u64>, plan: u64) -> bool {
    recent.push_back(plan);
    while recent.len() > REHOME_SUSTAINED_TICKS as usize {
        recent.pop_front();
    }
    recent.iter().all(|&seen| plan >= seen)
}

/// One line per 5 s tick makes the decline probe the defect at the other end, so
/// it speaks on ENTRY and roughly once a minute thereafter.
///
/// `ticks` MUST be a counter that advances for each consecutive declined tick.
/// Returning false for 0 is deliberate: it makes "the clock never advanced" mean
/// SILENCE rather than "log forever", which is the exact failure #3751 shipped.
const DECLINE_LOG_EVERY_TICKS: u32 = 12; // 5 s ticks -> ~1/min
pub(super) fn decline_should_log(ticks: u32) -> bool {
    ticks == 1 || (ticks > 0 && ticks % DECLINE_LOG_EVERY_TICKS == 0)
}

/// Re-home evidence: `(live_space, plan_space, gain, worth_it)` in TOTAL
/// token-space (window × lanes) — never the per-slot window alone.
///
/// Measured live 2026-09-01: the plan moved to 4 lanes × 47,426 while the
/// running server held 2 × 57,600 — 65% more total space and two more warm
/// slots, but the per-slot window SHRANK, so a window-only gain was negative
/// and the relaunch never fired. Five residents thrashed two slots at
/// hit_rate 0.0 until an operator cycled the server by hand. Lane count must
/// be part of the evidence
/// ([[the-same-bug-at-two-sites-is-a-missing-constraint-not-two-bugs]]).
///
/// Integer arithmetic that cannot overflow: u64 space on a 1M-token × 8-lane
/// server is ~8e6; ×100 still fits comfortably.
/// A steady hold yields when the plan's space exceeds the live space by
/// [`REHOME_HOLD_OVERRIDE_PCT`] or more — the hold protects a slot, never a starvation.
pub(super) fn hold_overridden_by_shortfall(live_space: u64, gain: u64) -> bool {
    live_space > 0 && gain.saturating_mul(100) >= live_space.saturating_mul(REHOME_HOLD_OVERRIDE_PCT as u64)
}

pub(super) fn rehome_gain_evidence(
    live_window: u32,
    live_lanes: u32,
    plan_window: u32,
    plan_lanes: u32,
) -> (u64, u64, u64, bool) {
    let live_space = (live_window as u64).saturating_mul(live_lanes.max(1) as u64);
    let plan_space = (plan_window as u64).saturating_mul(plan_lanes.max(1) as u64);
    let gain = plan_space.saturating_sub(live_space);
    let worth_it = live_space > 0
        && (gain.saturating_mul(100) >= live_space.saturating_mul(REHOME_MIN_GAIN_PCT as u64)
            || lane_over_its_knee(live_window, live_lanes, plan_window, plan_lanes));
    (live_space, plan_space, gain, worth_it)
}

/// The plan trades LANES for WIDTH: fewer slots, each at least as wide. That is the
/// decode knee clamping the roster (`inference::decode_knee`) — the live lane is
/// serving MORE streams than the box decodes at speed, every stream is slow, and the
/// only cure is a relaunch at the knee. Token-space says such a plan is "not a gain"
/// (8 × 51,712 → 7 × 67,340 is +14%, under the margin; → 4 × 67,340 is a LOSS), so
/// without this arm the knee only ever took effect at boot: measured 2026-09-17, the
/// M5 sat at 8 lanes / 7.9 t/s per stream all morning while the knee read 7 and the
/// reconcile declined every tick on the margin. The sustain streak and the cooldown
/// still gate the relaunch; this only makes a knee shrink COUNT as evidence.
pub(super) fn lane_over_its_knee(live_window: u32, live_lanes: u32, plan_window: u32, plan_lanes: u32) -> bool {
    live_lanes > 0 && plan_lanes > 0 && plan_lanes < live_lanes && plan_window >= live_window
}

/// The knee's mirror: the plan wants MORE warm slots than the live lane serves. The plan
/// never asks for more lanes than the roster demands (`lane_cap` is the demand, then the
/// fit), so a plan with more lanes than are live is minds WITHOUT a warm slot — the #266
/// shape: every turn of an unslotted mind re-prefills cold. Token-space calls such a plan
/// a wash or a loss (measured 2026-09-19 15:4xZ, the M5: a cold boot launched 1 × 161,792
/// under a transient 44 GB budget; the steady plan said 2 × 67,340 — 17% LESS space —
/// and the reconcile declined it every tick, so a second coder lane could never appear
/// at runtime, and the settled 1-lane geometry then capped the next boot: a one-way
/// ratchet with no mover the other way). The sustain streak, the flat-plan check and the
/// cooldown still gate the relaunch; this only makes a slot shortfall COUNT as evidence.
/// A plan may shrink while the host pages out; it never grows into it. A grow is more
/// KV space than the lane holds; paging out is swap rising since the last decision.
/// `swap_prev == u64::MAX` is "no previous reading" and never reads as rising. The swap
/// LEVEL is not a term (Cormac on #4415): a page in swap already gave up its RAM, so
/// `available` reflects it, and macOS keeps idle pages swapped for hours after the
/// pressure is gone — subtracting the level would under-plan by the whole residue.
pub(super) fn grow_refused_while_paging(
    live_space: u64,
    plan_space: u64,
    swap_prev: u64,
    swap_now: u64,
) -> bool {
    plan_space > live_space && swap_prev != u64::MAX && swap_now > swap_prev
}

pub(super) fn roster_short_of_slots(live_lanes: u32, plan_lanes: u32) -> bool {
    live_lanes > 0 && plan_lanes > live_lanes
}

/// Ticks a re-home must wait before another may fire, enforced INDEPENDENTLY of
/// the sustained-delta test above.
///
/// The two guards answer different questions and must not be collapsed into one:
/// sustained-delta asks "is this gain real?", the cooldown asks "may we pay for it
/// yet?". A relaunch kills every in-flight turn on the lane, so the rate limit has
/// to hold even when the evidence is perfect. Anchored to a lane's own readiness
/// budget ([`READY_TIMEOUT`], 90s) in ticks: never start a second re-home before
/// the first could possibly have come up and proven itself.
/// [[never-thrash-sticky-hysteresis-on-every-lane]]
pub(super) const REHOME_COOLDOWN_TICKS: u32 = (READY_TIMEOUT.as_secs() / TICK.as_secs()) as u32;

/// THE LANE DEMAND IS THE LIVE ROSTER (2026-09-14). `set_lane_demand` runs once at
/// boot with the persona floor and is a no-op afterwards, so a roster that grows to 16
/// after the decode-ready edge kept planning for the floor: measured 16 resident minds
/// on 3 lanes (2 + scratch) with 40 GB of KV budget idle, a lane every ~20 min per
/// mind, and the "let me take stock" loops that come from re-orienting every turn.
/// The roster is the demand; the boot floor is only its lower bound; a measurement
/// lease still wins while it is held.
/// The tick on which a launch SETTLES: its spawn cooldown reaches zero on this tick with
/// the lane ready and serving. Only a settled geometry is remembered for the next boot.
pub fn settles_this_tick(cooling_before: u32, ready: bool, lanes: u32) -> bool {
    cooling_before == 1 && ready && lanes > 0
}

/// PURE: has a launch just come READY that the reconcile has not settled on? `settled` is
/// the ready stamp last settled on (0 = none); `now` is the live snapshot's stamp (`None` =
/// not ready). A stamp not seen before is a new launch and re-arms the settle cooldown;
/// the same stamp again, or no stamp, is not.
pub fn ready_edge(settled: u64, now: Option<u64>) -> bool {
    matches!(now, Some(t) if t != settled)
}

/// L10 (#438) pure gate: may the reconcile commit a BASE-MODEL swap this tick?
///
/// A swap tears down the lane and re-homes every persona to different weights, so a
/// plan that names a different model than the READY incumbent must persist for
/// `needs` consecutive ticks. Changing the *target* mid-streak restarts the count
/// (a plan flapping 0.5B->27B->0.5B never accumulates); a tick where plan == live
/// clears any pending change. Boot / wedge-recovery (no ready incumbent) commits
/// immediately — recovery must not wait out a streak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelChangeGate {
    /// Not a ready-lane model change (same model, or no ready incumbent) — proceed.
    NotAChange,
    /// A change, but not yet sustained — do nothing this tick.
    Defer { streak: u32 },
    /// Sustained disagreement — commit the swap (streak consumed).
    Commit { streak: u32 },
}

pub(super) fn model_change_gate(
    live_ready: bool,
    live_model: Option<&str>,
    desired: &str,
    pending: &mut Option<String>,
    streak: &std::sync::atomic::AtomicU32,
    needs: u32,
) -> ModelChangeGate {
    use std::sync::atomic::Ordering;
    if !(live_ready && live_model.is_some() && live_model != Some(desired)) {
        streak.store(0, Ordering::Relaxed);
        *pending = None;
        return ModelChangeGate::NotAChange;
    }
    let n = if pending.as_deref() == Some(desired) {
        streak.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    } else {
        *pending = Some(desired.to_string());
        streak.store(1, Ordering::Relaxed);
        1
    };
    if n < needs {
        ModelChangeGate::Defer { streak: n }
    } else {
        streak.store(0, Ordering::Relaxed);
        *pending = None;
        ModelChangeGate::Commit { streak: n }
    }
}

impl ServingDaemonModule {
    /// A re-home streak starts over: the count and the plan high it was measured

    /// against together, so a stale high can never make a later climb read as flat.

    pub(super) fn reset_rehome_streak(&self) {

        self.rehome_streak.store(0, Ordering::Relaxed);

        self.rehome_plan_high.store(0, Ordering::Relaxed);

    }
}
