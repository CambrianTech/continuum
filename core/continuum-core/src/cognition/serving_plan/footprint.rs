//! A model's FOOTPRINT, derived: the catalog row, the artifact's own KV geometry, the
//! launched cache type and the lane's measured record, folded into one [`ModelFootprint`].
//!
//! This is pure math on the type [`super`] owns, so it lives beside it. It used to sit in
//! `modules/serving_daemon.rs` (12,148 lines), and a dozen modules reached into the
//! DAEMON for it, the model registry, eval, the launcher, the grid allocator, routing and
//! the footprint adapters among them, so every one of them depended on the serving loop to
//! ask how big a model is. The daemon is now one more caller.

use super::ModelFootprint;
use crate::inference::planning::MIN_SERVE_CTX;
use crate::model_registry::types::{Capability, Model};

pub fn footprint_for(model: &Model) -> Option<ModelFootprint> {
    // THE ROW IS THE SOURCE OF TRUTH. `weights_bytes` is stamped once, where the GGUF
    // path is resolved (`artifacts::hydrate_artifact_sizes`). This function runs on the
    // governor's accounting tick, so re-`stat`ing the file per call put filesystem I/O
    // on a hot path for a number that cannot change while the path is valid.
    //
    // The stat REMAINS as the fallback, and it is not dead code: rows built by hand in
    // tests, fixtures, and any construction path that never went through the resolver
    // carry `weights_bytes: None`. Falling back keeps those honest rather than sizing
    // them at zero — but the fallback is the exception, not the steady state.
    let weights_bytes = match model.weights_bytes {
        Some(n) => n,
        None => {
            let path = crate::model_registry::artifacts::resolve_gguf_for_model(model)?;
            crate::model_registry::artifacts::total_gguf_bytes(&path)
        }
    };
    // The VERIFIED serving ceiling wins over the trained maximum: a GGUF that
    // declares 262k trained context can still only be SERVED at the geometry we
    // measured (Flash-Next: 32k verified, 46848 OOM'd all night, 2026-08-29).
    let ctx_ceiling = model
        .serving
        .verified_ctx_ceiling
        .map(|c| model.context_window.min(c))
        .unwrap_or(model.context_window); // unwrap_or: no verified ceiling declared = trained max stands (the pre-2550 behavior)
    let mut fp = footprint_from_parts(
        &model.id,
        weights_bytes,
        ctx_ceiling,
        model.has(Capability::ToolUse),
        model.serving.measured_capability,
    )?;
    // THE ARTIFACT'S OWN KV GEOMETRY beats the weights-scaled guess. Measured
    // 2026-09-01: the heuristic read a 35B fine-grained MoE at ~244 KB/token
    // (experts add weights, zero KV) where the header declares ~61 KB f16 —
    // and the 4× inflation starved --cache-ram to ONE mind's worth (686 MiB,
    // every restore a miss, hit_rate 0.0 fleet-wide) AND under-granted lanes
    // vs resident demand. Header read is memoized per path: this runs on the
    // governor's accounting tick and a GGUF header parse is real I/O.
    let header_rate = crate::model_registry::artifacts::resolve_gguf_for_model(model)
        .and_then(|path| kv_rate_from_header_cached(&path));
    if let Some(rate) = header_rate {
        fp.kv_per_token = rate;
    }
    let fp = apply_kv_quantization(fp);
    // A header-declared KV rate (÷ the launched cache type) is the physical cost of a
    // token: the measurement may not raise it. "The process beats the arithmetic" was
    // written for the weights-scaled GUESS; against the header it can only mis-attribute
    // fixed residency (the draft model's KV, the vision projector, engine buffers) to
    // tokens — and it does so WORST at the smallest geometry, which is exactly when a
    // raised rate pins the shrink. Measured 2026-09-19 on the M5 (97f480e19): the header
    // said 65,536 f16 → 32,768 q8; a one-lane sample at 67,584 read 55,925 B/token,
    // `corrects()` fired, the plan carried 44,740, and two lanes stopped fitting a 30 GB
    // budget where the arithmetic at the header rate holds two (84k) and at 34 GB three.
    // The excess is a real fixed term — named on the probe below so a fixed-residency
    // model can learn it — never a per-token one.
    let mut fp = fp;
    let measured = crate::inference::lane_footprint::measured_record(&fp.model_id);
    fp.kv_per_token = kv_rate_for_plan(
        header_rate.is_some(),
        fp.kv_per_token,
        measured.as_ref().map(|r| r.per_token_bytes),
    );
    if header_rate.is_some() {
        if let Some(record) = &measured {
            fp.fixed_per_lane_bytes = fixed_per_lane_from(fp.kv_per_token, record);
            // One row when the value CHANGES: this runs on every footprint read (measured
            // 2026-09-19 23:2xZ on the M5: 840 identical rows in 12 minutes, 70/min — the
            // ledger-rotation class, the same shape the knee clamp's row had).
            static LAST_FIXED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
            if fp.fixed_per_lane_bytes > 0
                && LAST_FIXED.swap(fp.fixed_per_lane_bytes, std::sync::atomic::Ordering::Relaxed) != fp.fixed_per_lane_bytes
            {
                crate::probe!(
                    class = "serving.footprint.excess_is_fixed",
                    model = fp.model_id.as_str(),
                    header_rate = fp.kv_per_token,
                    measured_per_token = record.per_token_bytes,
                    measured_window = record.window as u64,
                    fixed_per_lane_bytes = fp.fixed_per_lane_bytes,
                    "the lane's measured per-token cost exceeds the header's physical KV rate — the excess is fixed residency (draft KV, projector, buffers) and is carried per lane; the header rate stands"
                );
            }
        }
    }
    Some(fp)
}

/// The residency a sample measured BEYOND the header's per-token rate, as fixed bytes per
/// lane: `(measured − rate) × the window it was measured at`. Zero when the sample agrees
/// with the header. The rate stays physics; the residency stays counted.
pub fn fixed_per_lane_from(header_rate: u64, record: &crate::inference::lane_footprint::MeasuredCost) -> u64 {
    record
        .per_token_bytes
        .saturating_sub(header_rate)
        .saturating_mul(record.window as u64)
}

/// THE PROCESS BEATS THE ARITHMETIC. When the live lane's anonymous footprint, read
/// off the serving process (`inference::lane_footprint`), says a served token costs
/// more than the header + quantization arithmetic claims, the plan's per-token cost
/// rises to the measurement. Measured 2026-09-17 on the M5: arithmetic 10 KB/token,
/// process 36 KB/token (compute buffers at ubatch 2048 over a 412k total context, the
/// hybrid's recurrent state and checkpoints) — the plan fitted 8 × 51,712 into 30 GB,
/// the server took 47 GB, the box swapped and every stream ran at 7.9 t/s. The plan's
/// per-token cost is `kv + kv / PREFILL_COMPUTE_KV_DIVISOR`, so kv is set to make that
/// sum the measurement; a smaller measurement never lowers it (a footprint is a lower
/// bound of the need).
/// The per-token rate the plan carries: a header-declared rate is physics and stands
/// whatever the process measured; only a heuristic estimate is open to correction.
pub fn kv_rate_for_plan(header_known: bool, estimate: u64, measured: Option<u64>) -> u64 {
    match (header_known, measured) {
        (true, _) | (false, None) => estimate,
        (false, Some(m)) => kv_per_token_from_measured(estimate, m),
    }
}


/// Pure half of [`apply_measured_cost`]: the kv rate whose plan cost (`kv + kv / D`)
/// equals `measured`, or the estimate when the measurement does not correct it.
fn kv_per_token_from_measured(estimate: u64, measured: u64) -> u64 {
    use super::PREFILL_COMPUTE_KV_DIVISOR as D;
    let estimated_cost = estimate.saturating_add(estimate / D);
    if !crate::inference::lane_footprint::corrects(estimated_cost, measured) {
        return estimate;
    }
    // kv × (D + 1) / D = measured  →  kv = measured × D / (D + 1)
    measured.saturating_mul(D) / (D + 1)
}

/// Memoized `gguf_keys::kv_bytes_per_token_f16` per artifact path. `None` is
/// cached too (header unreadable / geometry keys absent → the heuristic
/// stands), so a broken artifact costs one parse, not one per tick.
fn kv_rate_from_header_cached(path: &std::path::Path) -> Option<u64> {
    static CACHE: std::sync::OnceLock<
        dashmap::DashMap<std::path::PathBuf, Option<u64>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(dashmap::DashMap::new);
    if let Some(hit) = cache.get(path) {
        return *hit;
    }
    let rate = (|| {
        let mut file = std::fs::File::open(path).ok()?;
        let content =
            candle_core::quantized::gguf_file::Content::read(&mut file).ok()?;
        let arch = crate::inference_capability::gguf_keys::architecture(&content)?;
        crate::inference_capability::gguf_keys::kv_bytes_per_token_f16(&content, &arch)
    })();
    crate::probe!(
        class = "serving.kv_rate.header",
        path = %path.display(),
        rate_f16 = rate.unwrap_or(0), // 0 = header did not answer; the weights heuristic stands (the probe's absence-vs-zero reading rides `answered`)
        answered = rate.is_some(),
        "KV bytes/token from the artifact's own declared geometry — replaces the \
         weights-scaled guess that over-charged MoE models ~4x",
    );
    cache.insert(path.to_path_buf(), rate);
    rate
}

/// KV CACHE QUANTIZATION (#232): a lane running quantized KV holds proportionally
/// fewer bytes/token, so the plan can size a BIGGER window into the same budget —
/// this is what turns the q8_0 cache type into an actual window GROWTH.
/// Divide the f16 rate by the quant factor; f16 → 1 → byte-identical.
///
/// THE DIVISOR IS NOT A SECOND READ OF THE CONFIG. It is the SAME
/// [`crate::cognition::kv_cache_plan::KvCachePlan`] the launcher takes its
/// `--cache-type-k/v` and `--flash-attn` flags from, so the plan's arithmetic and the
/// engine's flags cannot disagree. They shared a *key* before, which is how the 5090 —
/// where nobody ever exported `SERVING_KV_CACHE_TYPE` — planned AND served half-size
/// lanes self-consistently for its whole life (a 26,880-token lane, measured
/// 2026-09-20). Sharing the key made the two halves agree about the wrong number;
/// sharing the DECISION makes them agree about the right one.
///
/// ONE named transform (not inlined in `footprint_for`) so every derivation of the
/// live footprint — production AND the tests that assert against it — rides the SAME
/// resolved plan instead of silently assuming f16. The env-dependent-test incident
/// this prevents: an operator serving `SERVING_KV_CACHE_TYPE=q8_0` ran the footprint
/// test locally and it failed on a number CI called green, because the expectation
/// was built from raw parts while the resolver divided by 2 (2026-08-24).
pub(crate) fn apply_kv_quantization(mut fp: ModelFootprint) -> ModelFootprint {
    fp.kv_per_token = (fp.kv_per_token / kv_cache_quant_divisor()).max(1);
    fp
}

/// The resident-KV divisor of the resolved KV cache decision, so the plan sizes the
/// served window against the KV the lane WILL actually hold, not the f16 default. (#232)
fn kv_cache_quant_divisor() -> u64 {
    crate::cognition::kv_cache_plan::resolve().bytes_per_token_divisor
}

/// Pure footprint estimate from the fields that drive it — split out from the
/// fs/registry IO so the (coarse, tunable) heuristics are unit-testable.
/// `None` when there are no weights to serve.
/// An UNMEASURED model's weight-proxy rank never reaches a measured frontier
/// score: the proxy says "bigger ≈ smarter within a family", nothing across
/// families, so it must lose to any model someone actually measured.
// context-budget-exempt: a rank ceiling on a 0–255 capability scale, not a window or token budget
pub const UNMEASURED_RANK_CAP: u8 = 40;

pub(crate) fn footprint_from_parts(
    id: &str,
    weights_bytes: u64,
    context_window: u32,
    tool_capable: bool,
    measured_capability: Option<u8>,
) -> Option<ModelFootprint> {
    if weights_bytes == 0 {
        return None;
    }
    // Coarse per-token KV RATE: scale with model size (more weights ≈ more
    // layers ≈ more KV/token). ~weights/20k bytes per token, floored. The
    // planner multiplies this by the window IT derives from the host budget —
    // we do NOT pre-collapse it against an assumed serving window here (no
    // PLANNED_CTX clamp; the served window is the planner's call, task #50).
    // Realistic per-token KV. These Qwen coders use grouped-query attention (few KV
    // heads), so KV is SMALL: ~200 KB/token for the 14B, ~260 KB/token for the 32B. The
    // old `weights/20_000` gave ~1 MB/token for the 32B — 4× too high — which made a 32B
    // that comfortably fits a 64 GB box read as ~32 GB of KV at 32k ctx and get falsely
    // rejected by the fit-gate, so the planner kept a weaker model. `weights/80_000`
    // (~110 KB for 14B, ~250 KB for 32B) tracks real Q4 GQA KV; still coarse, still floored.
    let kv_per_token = (weights_bytes / 80_000).max(20_000);

    // Coarse capability rank: GB of weights (bigger ≈ more capable within a
    // family), +bonus for tool/code capability. Saturates into u8.
    let gb = (weights_bytes / 1_000_000_000).min(250) as u16;
    let tool_bonus = if tool_capable { 2 } else { 0 };
    // Measured beats proxy: a catalog score (one scale) IS the rank; the weight
    // proxy is only for models nobody measured, and it caps below any measured
    // frontier so the planner's "most capable that fits" means capable, not big.
    let proxy_rank = gb.saturating_add(tool_bonus).min(u16::from(UNMEASURED_RANK_CAP)) as u8;
    let capability_rank = measured_capability.unwrap_or(proxy_rank); // unwrap_or: measured beats proxy, the proxy ranks only a model nobody measured

    Some(ModelFootprint {
        model_id: id.to_string(),
        weights_bytes,
        kv_per_token,
        // The model's trained ceiling, carried straight through — the planner
        // caps the served window to it. Floored at MIN_SERVE_CTX so a registry
        // entry with a bogus 0 window can't degrade below runnable.
        context_window: context_window.max(MIN_SERVE_CTX),
        capability_rank,
        fixed_per_lane_bytes: 0,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    // what this catches (the M5, 2026-10-10 19:58-20:01Z: per-lane compute read 1.93 GB and
    // 4.40 GB on alternate ticks, and the plan flipped between 2 lanes and 1 every tick): a
    // lane measurement fed its own derived fixed term. The loop the daemon runs is sample
    // (anon minus the floor the sampler subtracts) -> fixed term -> next floor. With the
    // base buffer as that floor, the fixed term is the lane's true fixed residency after
    // ONE sample and stays there; subtracting compute_buffer_per_lane (base + fixed) gives
    // fixed(n+1) = C - fixed(n), which alternates and fails this.
    #[test]
    fn a_lane_measurement_converges_and_never_feeds_back_its_own_fixed_term() {
        let header_rate = 32_768u64;
        let (lanes, window) = (1u32, 56_832u32);
        let mut fp = ModelFootprint {
            model_id: "m5-27b".into(), weights_bytes: 19_000_000_000, kv_per_token: header_rate, context_window: 262_144, capability_rank: 42, fixed_per_lane_bytes: 0,
        };
        let true_fixed = 3_950_000_000u64; // what the engine really holds per lane beyond KV and the base buffer
        let anon = (fp.base_compute_buffer_per_lane() + true_fixed + header_rate * u64::from(window)) * u64::from(lanes);
        let mut seen = Vec::new();
        for _ in 0..4 {
            let per_token = crate::inference::lane_footprint::per_token_from(anon, lanes, window, fp.base_compute_buffer_per_lane(), 0)
                .expect("test: a reading");
            let record = crate::inference::lane_footprint::MeasuredCost { per_token_bytes: per_token, lanes, window, anon_bytes: anon, last_ms: 0 };
            fp.fixed_per_lane_bytes = fixed_per_lane_from(header_rate, &record);
            seen.push(fp.fixed_per_lane_bytes);
        }
        let tolerance = u64::from(window); // integer division loses under one byte per token
        assert!(seen.iter().all(|f| f.abs_diff(true_fixed) <= tolerance), "converges to the true fixed term at once and holds: {seen:?}");
    }

    // what this catches (2026-09-17, the M5 swapping at 8 × 51,712): a fresh measured
    // per-token cost above the arithmetic raises the plan's kv rate so that kv + kv/D
    // equals the measurement; agreement or a smaller reading leaves the estimate alone.
    #[test]
    fn a_measured_per_token_cost_corrects_the_plans_rate_upward_only() {
        use super::kv_per_token_from_measured;
        use super::super::PREFILL_COMPUTE_KV_DIVISOR as D;
        // what this catches (2026-09-19, the M5 pinned at one lane): a header-declared
        // rate is never raised by a measurement — the excess is fixed residency; a
        // heuristic estimate still is.
        assert_eq!(kv_rate_for_plan(true, 32_768, Some(55_925)), 32_768, "header rate is physics");
        assert_eq!(kv_rate_for_plan(true, 32_768, None), 32_768);
        assert_eq!(kv_rate_for_plan(false, 32_768, None), 32_768);
        assert!(kv_rate_for_plan(false, 10_240, Some(36_000)) > 10_240, "a guess still learns from the process");
        // The excess is carried, not dropped: the M5 shape (header 32,768 q8, a one-lane
        // sample of 55,925 at 67,340) yields ≈1.56 GB of fixed residency per lane, and
        // across budgets the plan with it fits no more lanes than the header-only plan
        // and no fewer than the raised-rate plan — strictly between them somewhere.
        let record = crate::inference::lane_footprint::MeasuredCost { per_token_bytes: 55_925, lanes: 1, window: 67_340, anon_bytes: 0, last_ms: 0 };
        let fixed = fixed_per_lane_from(32_768, &record);
        assert!((1_500_000_000..1_620_000_000).contains(&fixed), "{fixed} B per lane");
        assert_eq!(fixed_per_lane_from(65_536, &record), 0, "a sample under the rate carries nothing");
        let base = ModelFootprint {
            model_id: "m5-27b".into(), weights_bytes: 16_500_000_000, kv_per_token: 32_768, context_window: 262_144, capability_rank: 42, fixed_per_lane_bytes: 0,
        };
        let carried = ModelFootprint { fixed_per_lane_bytes: fixed, ..base.clone() };
        let raised = ModelFootprint { kv_per_token: 44_740, ..base.clone() };
        let lanes_at = |fp: &ModelFootprint, usable: u64| (1..=4u32).rev().find(|&l| fp.window_within(usable, l) >= 67_340).unwrap_or(0);
        // Carried never fits MORE than header-only (the fixed term only costs), and costs a
        // lane somewhere in the range. It is NOT ordered against the raised-rate plan: a
        // fixed cost bites hardest at small lane counts (31 GB: carried 1, raised 2) and a
        // raised rate at large ones — which is the whole point of carrying it as fixed.
        let mut strict = false;
        for usable_gb in 28..=46u64 {
            let usable = usable_gb * 1_000_000_000;
            let (h, c) = (lanes_at(&base, usable), lanes_at(&carried, usable));
            assert!(h >= c, "{usable_gb} GB: header {h} ≥ carried {c}");
            strict |= h > c;
        }
        assert!(strict, "the carried term costs at least one lane somewhere in the range");
        // The M5's own numbers, pinned, with the budget sized as handed (no second
        // headroom on top of the mode's — `CACHE_GRANT_HEADROOM`): header-only 3 lanes at
        // 30 GB and 4 at 34; carried 2 at 30 and 3 at 34; the raised rate the old code
        // produced: 2 at 30, 3 at 34. (Under the stacked fractions these read 2/3, 1/2, 1/2.)
        assert_eq!((lanes_at(&base, 30_000_000_000), lanes_at(&base, 34_000_000_000)), (3, 4));
        assert_eq!((lanes_at(&carried, 30_000_000_000), lanes_at(&carried, 34_000_000_000)), (2, 3));
        assert_eq!((lanes_at(&raised, 30_000_000_000), lanes_at(&raised, 34_000_000_000)), (2, 3));
        let kv = kv_per_token_from_measured(10_240, 36_000);
        assert_eq!(kv.saturating_add(kv / D) / 100, 36_000 / 100, "the plan's cost now equals the measurement");
        assert_eq!(kv_per_token_from_measured(10_240, 12_000), 10_240, "within agreement: the estimate stands");
        assert_eq!(kv_per_token_from_measured(36_000, 10_240), 36_000, "a smaller reading never lowers it");
    }

    // what this catches: the plan's resident-KV divisor is the SAME resolved decision
    // the launcher flags with — not a second read of a config key. The two shared a
    // KEY before, which is how the 5090 (and the CPU-serving IntelMac) planned AND
    // served half-size lanes self-consistently: a 26,880-token lane, measured
    // 2026-09-20. The divisor table itself is pinned in cognition/kv_cache_plan.rs.
    #[test]
    fn the_plans_kv_divisor_is_the_resolved_decisions_divisor() {
        assert_eq!(
            kv_cache_quant_divisor(),
            crate::cognition::kv_cache_plan::resolve().bytes_per_token_divisor,
            "the fit math and the launcher flag must be two projections of ONE decision"
        );
    }

    #[test]
    fn footprint_from_parts_is_footprint_aware() {
        let fp = footprint_from_parts("present", 3 * GB, 8192, true, None).unwrap();
        assert_eq!(fp.model_id, "present");
        assert_eq!(fp.weights_bytes, 3 * GB);
        assert!(fp.kv_per_token > 0);
        assert_eq!(
            fp.context_window, 8192,
            "carries the model's trained ceiling, no clamp"
        );
        assert!(
            fp.capability_rank >= 5,
            "3GB + tool bonus, got {}",
            fp.capability_rank
        );

        // A leaner non-tool model ranks below the bigger tool-capable one.
        let small = footprint_from_parts("small", 1 * GB, 4096, false, None).unwrap();
        assert!(small.capability_rank < fp.capability_rank);

        assert!(
            footprint_from_parts("empty", 0, 8192, false, None).is_none(),
            "no weights → not servable"
        );
    }

    // what this catches: the planner's "most capable that fits" meaning BIG again.
    // A measured score is the rank; an unmeasured model keeps the weight proxy,
    // capped below any measured frontier — so a 19 GB model someone measured at
    // 42 outranks a 22 GB model nobody measured, and a 200 GB unmeasured model
    // still cannot outrank it. (Joel 2026-09-05: "You guys aren't understanding
    // how scaling works.")
    #[test]
    fn a_measured_capability_outranks_any_weight_proxy() {
        let measured = footprint_from_parts("qwen3.8-27b", 19 * GB, 8192, true, Some(42)).unwrap();
        let bigger_unmeasured = footprint_from_parts("ornith-35b", 22 * GB, 8192, true, None).unwrap();
        let huge_unmeasured = footprint_from_parts("huge", 200 * GB, 8192, true, None).unwrap();
        assert_eq!(measured.capability_rank, 42, "a measured score is the rank");
        assert!(
            measured.capability_rank > bigger_unmeasured.capability_rank,
            "measured 42 beats the 22 GB proxy ({})",
            bigger_unmeasured.capability_rank
        );
        assert_eq!(
            huge_unmeasured.capability_rank, UNMEASURED_RANK_CAP,
            "an unmeasured model caps at the proxy ceiling"
        );
        assert!(measured.capability_rank > huge_unmeasured.capability_rank);
    }
}
