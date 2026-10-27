//! Moe cache ownership for the serving daemon.

use super::*;

/// Sticky-lease hysteresis band for the host-cache budget: 1/8 of the published
/// value (move only on >12.5% change). Same band shape as the plan hysteresis —
/// KV-sample flutter must not churn the plan file's mtime under her per-token poll.
pub(super) const HOST_CACHE_LEASE_BAND_DIVISOR: u64 = 8;
/// Recency-window tokens carried on the published plan document. Inert while
/// `pin_list` is empty (v1 publishes budget-only); matches the golden v1 wire
/// fixture so the C++ consumer's parse sees the shape it was validated against.
// context-budget-exempt: the EXPERT PAGER's recency window (thousands of tokens of routing history for the pin list), part of the published plan wire format the C++ consumer validates against — not the prompt
pub(super) const MOE_PLAN_WINDOW_K: u32 = 8;

/// What the serve's working set costs HOST RAM, by memory topology.
///
/// On unified memory (Apple Silicon) the engine's on-device residency IS host RAM:
/// weights + per-lane KV + the prefill compute reserve all come out of the one pool,
/// so the cache competes with them. On a discrete GPU they live in VRAM — the mmap'd
/// weight file is reclaimable page cache, KV and compute buffers are on the card —
/// and the host pays nothing for them; a fully-offloaded dense serve (the plan
/// requires `fits_on_gpu`) has a host working set of zero beyond the cache itself.
///
/// The unified arithmetic applied to the 5090 (2026-09-16): physical 63 GB minus a
/// 144k-window 27B's "resident" 55 GB minus the OS floor = 0 affordable → `--cache-ram
/// 256` (the hard floor) → every citizen turn a full re-prefill (Kimi: 44 s, hit 0.0)
/// on a box with 49 GB of host RAM free. No GPU attached (`None`) = CPU serving =
/// everything is host-resident = the unified arithmetic.
pub(super) fn serve_host_bytes(
    fp: Option<&crate::inference::planning::serving_plan::ModelFootprint>,
    served_ctx: u32,
    lanes: u32,
    memory_mode: Option<crate::gpu::monitor::MemoryMode>,
) -> u64 {
    match (fp, memory_mode) {
        (_, Some(crate::gpu::monitor::MemoryMode::Discrete)) => 0,
        (Some(fp), _) => fp.peak_resident_bytes(served_ctx, lanes),
        (None, _) => 0,
    }
}

/// The everyday drive mode this node's operator asked for, from
/// `CONTINUUM_VRAM_HEADROOM` (default 0.8 → Comfort; a dedicated node at 1.0 →
/// Performance). One reader, cached by `config_env`.
pub(super) fn everyday_drive_mode() -> crate::capacity::power_mode::PowerMode {
    crate::capacity::power_mode::PowerMode::everyday_for_headroom(
        continuum_base::config_env::vram_headroom(),
    )
}

pub(super) fn host_os_floor_bytes(physical_bytes: u64) -> u64 {
    (physical_bytes / 8).max(2 * 1024 * 1024 * 1024)
}

/// Windows only: the system-wide commit charge (`GetPerformanceInfo`), the number
/// the pagefile-overcommit clamp binds against — a Windows box under load can have
/// commit far above what a "free RAM" read suggests, and pagefile overcommit
/// thrashes SILENTLY (the 2026-08-01 incident: 95.9GB commit on a 63GB box, fetch
/// collapsed 2.5GB/s→205MB/s with no OOM anywhere). `None` on read failure — the
/// lease then falls back to the headroom bound alone, which is honest but looser.
#[cfg(windows)]
pub(super) fn system_commit_charge_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    // SAFETY: PERFORMANCE_INFORMATION is a plain C struct; zeroed is a valid
    // initial state, and GetPerformanceInfo only writes within `cb` bytes.
    let mut info: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    info.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let ok = unsafe { GetPerformanceInfo(&mut info, info.cb) };
    (ok != 0).then(|| (info.CommitTotal as u64).saturating_mul(info.PageSize as u64))
}

/// macOS/Linux: commit is not the binding regime (no silent pagefile ballooning of
/// this shape) — the lease's headroom arithmetic is the bound. See the law's doc.
#[cfg(not(windows))]
pub(super) fn system_commit_charge_bytes() -> Option<u64> {
    None
}

/// Assemble the host-cache lease inputs for a MoE serve from quantities the planner
/// ALREADY budgets — every term justified, none invented, none read from a
/// free-memory monitor (the mmap subtlety: file-backed weight pages report
/// "available" while load-bearing):
///
/// - `weights_host_bytes` = GGUF file size MINUS the expert-tensor total. Expert
///   bytes are exactly what the governed cache holds (funded BY the lease) or sit
///   hot in VRAM — they are never host working set. Without the split, a 663GB
///   streaming MoE would "cost" 663GB of host RAM and the lease would derive 0.
/// - KV and compute-buffer terms come from the planner's OWN laws (`kv_at`,
///   `prefill_compute_reserve` — one formula, one place), but computed over a
///   footprint built from the HOST-RESIDENT mass, not the full file: those
///   heuristics scale with weight bytes as a proxy for layer/graph size, and for
///   a streaming MoE the dense/attention share is the mass that actually sizes
///   the KV stack and prefill graph (663GB would yield an absurd 68GB KV
///   estimate and a 41GB buffer, deriving a permanent zero lease).
/// - `live_kv_bytes` uses the LIVE served window × LIVE lane count from the
///   snapshot (llama.cpp allocates one full window per slot) — the term whose
///   omission thrashed the 63GB box to 95.9GB commit.
///
/// `None` when the host share is zero (a degenerate all-expert read — no honest
/// footprint to derive from; publish nothing rather than a guess).
pub(super) fn moe_host_cache_lease_inputs(
    model_id: &str,
    file_weights_bytes: u64,
    expert_bytes_total: u64,
    model_context_window: u32,
    served_window: u32,
    lanes: u32,
    physical_bytes: u64,
    commit_charge_bytes: Option<u64>,
    prompt_cache_mib: u32,
) -> Option<crate::capacity::host_cache_lease::HostCacheLeaseInputs> {
    let weights_host_bytes = file_weights_bytes.saturating_sub(expert_bytes_total);
    let fp = footprint_from_parts(
        model_id,
        weights_host_bytes,
        model_context_window,
        false,
        None,
    )?;
    let lanes = lanes.max(1);
    Some(crate::capacity::host_cache_lease::HostCacheLeaseInputs {
        physical_bytes,
        weights_host_bytes,
        live_kv_bytes: fp.kv_at(served_window).saturating_mul(lanes as u64),
        // The llama-server host prompt cache (`--cache-ram`, KV-ECONOMY §4) is a
        // real resident term of the serve's working set — accounted here so the
        // expert-cache lease can never be derived as if that RAM were free.
        compute_buffer_bytes: fp
            .prefill_compute_reserve(served_window, lanes)
            // The DERIVED prompt cache of the current serve — the same number the
            // lane's --cache-ram carries (restore-economy 1.b), never a constant
            // the lease has to work around.
            .saturating_add((prompt_cache_mib as u64) * 1024 * 1024),
        os_floor_bytes: host_os_floor_bytes(physical_bytes),
        commit_charge_bytes,
    })
}

impl ServingDaemonModule {
    pub(super) fn publish_moe_host_cache_lease(&self) {
        let live = self.serving_tx.borrow().clone();
        if !live.ready || live.served_context_window == 0 {
            return;
        }
        let Some(active) = live.active_model.clone() else {
            return;
        };
        let Some(port) = crate::inference::llama_server::port_of_base_url(&live.base_url) else {
            return;
        };
        // Expert geometry for the ACTIVE model — present only for a MoE serve.
        // Read under the same never-across-await sync Mutex as the placement path.
        // top_k + experts-per-layer feed the retention verdict below: the lease
        // is only USEFUL when it exceeds one token's expert working set.
        let (expert_bytes_total, n_experts_per_layer, top_k, n_layers) = {
            let Ok(guard) = self.moe_serving.lock() else {
                return;
            };
            match guard.as_ref() {
                Some((id, Some(ctx))) if *id == active => (
                    ctx.expert_bytes_total,
                    ctx.n_experts_per_layer,
                    ctx.top_k,
                    ctx.n_layers,
                ),
                _ => return, // dense model (or stale context) — no governed cache to lease
            }
        };
        let Some(model) = (self.model_resolver)(&active) else {
            return;
        };
        let Some(fp) = footprint_for(&model) else {
            return;
        };
        let physical = self.system.memory().total_bytes;
        if physical == 0 {
            return;
        }
        let Some(inputs) = moe_host_cache_lease_inputs(
            &active,
            fp.weights_bytes,
            expert_bytes_total,
            model.context_window,
            live.served_context_window,
            live.lanes,
            physical,
            system_commit_charge_bytes(),
            // The running engine's grant; an adopted lane (unknown, 0) accounts the
            // declared prior, as the cold-start derivation does.
            if live.host_prompt_cache_mib > 0 {
                live.host_prompt_cache_mib
            } else {
                crate::inference::lane_args::CACHE_RAM_MIB
            },
        ) else {
            return;
        };
        let derived = crate::capacity::host_cache_lease::host_cache_lease_bytes(&inputs);
        // Sticky band on the BUDGET axis only: `budget_moved` says the
        // published value changed; a held budget reuses the last published
        // value so the ACTUATOR axis (pins, below) can still trigger a
        // write. The no-churn property now lives on the write decision —
        // nothing changed on either axis ⇒ no write, no mtime churn.
        let (budget_bytes, budget_moved) = {
            let Ok(mut sticky) = self.host_cache_lease.lock() else {
                return;
            };
            match sticky.observe(derived) {
                Some(published) => (published, true),
                None => (sticky.published_bytes(), false),
            }
        };
        if budget_bytes == 0 {
            return; // nothing governed yet — never publish a zero lease
        }
        // DEVICE axis (#305, BigMama's measured ask): the copy-stream's
        // expert-slot budget = live free-VRAM-after-fit less the placement
        // margin. The board's `available(Vram)` is already net of the
        // resident serve (#79 registers it as a consumer), so this is
        // exactly "size to the free VRAM AFTER device-fit, not a fixed
        // number". Sticky like the host axis; a zero publishes as ABSENT
        // (the mechanism keeps its own sizing), never as a zero budget.
        let device_derived = governed_vram_ceiling(&self.resource_daemon)
            .unwrap_or(0)
            .saturating_sub(EXPERT_PLACEMENT_MARGIN_BYTES);
        let (device_budget_bytes, device_moved) = {
            let Ok(mut sticky) = self.device_budget_lease.lock() else {
                return;
            };
            match sticky.observe(device_derived) {
                Some(published) => (published, true),
                None => (sticky.published_bytes(), false),
            }
        };
        let Some(gb) = crate::inference::llama_server::moe_glass_box_paths(port) else {
            return;
        };
        // Pin actuator (#281): drain the fork's routed-expert trace into
        // the bandit and carry its hot list on the governed plan. The pin
        // budget is retention-derived — at most half the lease's worth of
        // experts (the recency window keeps the other half) — so a lease
        // too small to retain anything publishes budget-only (v1 shape).
        let (
            pins,
            boundary_crossed,
            tokens_observed,
            pins_moved,
            repeat_recall,
            delta_recall,
            coverage,
        ) = {
            let Ok(mut tail_guard) = self.moe_trace_tail.lock() else {
                return;
            };
            let tail = tail_guard
                .get_or_insert_with(|| crate::capacity::trace_tail::MoeTraceTail::new(n_layers));
            tail.ensure_geometry(n_layers);
            tail.drain(&gb.trace);
            let ceiling = crate::capacity::trace_tail::pin_ceiling(
                budget_bytes,
                expert_bytes_total,
                n_layers,
                n_experts_per_layer,
            );
            let pins = tail.pin_list(ceiling);
            let moved = tail.pins_changed(&pins);
            (
                pins,
                tail.boundary_crossed,
                tail.tokens_observed,
                moved,
                tail.repeat_recall_x100(),
                tail.predicted_delta_recall_x100(),
                tail.schedulable_coverage_x100(),
            )
        };
        // Division actuation (#2 of the resident/cache split, contract 2026-08-03): the
        // warm-started bandit over the discovered `--resident-only` tiers picks which
        // RESIDENT precision the next relaunch should load, and the trace-tail token
        // delta feeds back the measured decode tok/s for the tier actually serving.
        // The tier label is a fourth plan axis; the device budget above remains the
        // one budget authority (a smaller resident frees VRAM the board then sees).
        let division = self.division_tick(
            &active,
            inputs.weights_host_bytes,
            expert_bytes_total,
            n_experts_per_layer,
            top_k,
            n_layers,
            tokens_observed,
        );
        let tier_moved = division.as_ref().is_some_and(|(_, moved)| *moved);
        if !budget_moved && !pins_moved && !boundary_crossed && !device_moved && !tier_moved {
            return; // no axis changed — no write, no mtime churn
        }
        let pins_count = pins.len();
        let mut doc = crate::capacity::plan_file::PlanFileDocument::new(
            budget_bytes,
            MOE_PLAN_WINDOW_K,
            pins,
        );
        if device_budget_bytes > 0 {
            doc = doc.with_device_budget(device_budget_bytes);
        }
        if let Some((tier_label, _)) = &division {
            doc = doc.with_resident_tier(tier_label.clone());
        }
        // Retention verdict (#287): does the published lease retain even one
        // token's expert working set? Below 100 (×100 fixed-point) the cache
        // buys NOTHING — every token evicts the previous token's experts (the
        // resident=0 thrash) — and the lease is honest about it instead of
        // looking healthy while thrashing silently.
        let per_token_ws = crate::capacity::host_cache_lease::per_token_expert_working_set_bytes(
            expert_bytes_total,
            n_experts_per_layer,
            top_k,
        );
        let retention_x100 =
            crate::capacity::host_cache_lease::retention_tokens_x100(budget_bytes, per_token_ws);
        if retention_x100 < 100 {
            continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.moe_host_cache_retention",
                model = active.as_str(),
                budget_bytes,
                per_token_ws_bytes = per_token_ws,
                retention_tokens_x100 = retention_x100,
                "lease retains LESS than one token's expert working set — the \
                 cache thrashes (zero cross-token reuse); free host RAM or accept \
                 streaming-only decode",
            );
        }
        match crate::capacity::plan_file::write_plan_file(&gb.plan, &doc) {
            Ok(()) => continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.moe_host_cache_lease",
                model = active.as_str(),
                budget_bytes,
                device_budget_bytes,
                device_derived_bytes = device_derived,
                derived_bytes = derived,
                weights_host_bytes = inputs.weights_host_bytes,
                live_kv_bytes = inputs.live_kv_bytes,
                per_token_ws_bytes = per_token_ws,
                retention_tokens_x100 = retention_x100,
                pins = pins_count as u64,
                trace_tokens = tokens_observed,
                warm_start = boundary_crossed,
                repeat_recall_x100 = repeat_recall.unwrap_or(0) as u64,
                predicted_delta_recall_x100 = delta_recall.unwrap_or(0) as u64,
                schedulable_coverage_x100 = coverage.unwrap_or(0) as u64,
                plan = gb.plan.display().to_string().as_str(),
                "published governed host-cache lease: {} GiB (raw {} GiB, retains \
                 {}.{:02} tokens' expert set)",
                budget_bytes / (1024 * 1024 * 1024),
                derived / (1024 * 1024 * 1024),
                retention_x100 / 100,
                retention_x100 % 100,
            ),
            Err(e) => continuum_base::probe!(
                target: "continuum_core::modules::serving_daemon", class = "serving.moe_host_cache_lease",
                model = active.as_str(),
                budget_bytes,
                error = e.to_string().as_str(),
                "plan-file write FAILED — actuator frozen at last published budget",
            ),
        }
    }
}
