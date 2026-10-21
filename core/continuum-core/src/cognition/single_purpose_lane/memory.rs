//! Whether a single-purpose lane fits in this host's memory before it is stood up: the
//! system pressure gate, then a hard free-RAM veto sized from the base's weights.

/// Refuse to stand up a single-purpose lane (a gene eval, a CPU training lane) while the MACHINE is already drowning.
///
/// The VRAM ledger (#56/G1) accounts what the governor GRANTED — but on unified
/// memory the machine can be compressor-deep in swap from everything the ledger
/// doesn't meter (builds, apps, page compression), and the arithmetic still says
/// yes. That exact gap kernel-panicked the dev machine (2026-07-22: watchdog
/// timeout, compressed-pages 100%, 5 swapfiles) while a benchmark battery stood
/// up an eval lane. `PressureLevel`'s own contract already names the rule —
/// High: "aggressive eviction", Critical: "refuse new allocations" — and Whisper
/// honors it before loading its model; a multi-GB ELECTIVE lane must too. Veto at
/// >= High or while the sustained-pressure gate is closed. Fail-loud, never a
/// silent degrade: the detached ledger carries this exact string, and the harness
/// retries when pressure clears. Live serving is untouched — it has its own
/// shedding machinery; this gates only the deferrable measurement load.
pub(crate) fn refuse_lane_under_memory_pressure(footprint: Option<u64>) -> Result<(), String> {
    lane_memory_veto(
        crate::system_resources::MemoryPressureMonitor::current_level(),
        crate::system_resources::is_memory_gate_closed(),
    )?;
    // The pressure LEVEL passed — but on unified memory a macOS "Normal" level can
    // still sit atop only a few GB of real free RAM (it counts compressible/cached
    // pages as available). Standing up a SECOND llama-server of a known footprint into
    // that is a jetsam-SIGKILL wall the level never sees (glass-boxed 2026-07-27: a 24B
    // eval lane co-resident with a 24B live lane, 9.6 GB free → exit 137, zero-byte
    // log). Size against the honest free-bytes number so we REFUSE cleanly (deferrable
    // load, retried by `await_lane_memory_headroom`) instead of OOM-crashing.
    lane_ram_veto(
        crate::system_resources::current_available_bytes(),
        footprint,
        LANE_RAM_HEADROOM_BYTES,
    )
}

/// Weight-file bytes ≈ resident weight bytes; ×1.25 covers KV cache + scratch at the
/// bounded eval ctx. `None` → couldn't size (no GGUF on disk / unstatable). ONE place
/// so the pre-spawn RAM gate and the GPU/CPU placement decision size identically.
pub(crate) fn lane_footprint(base: &crate::model_registry::Model) -> Option<u64> {
    crate::model_registry::artifacts::resolve_gguf_for_model(base)
        .and_then(|p| std::fs::metadata(&p).ok())
        .map(|m| (m.len() as f64 * 1.25) as u64)
}

/// Free physical memory kept in reserve beyond the lane's own footprint — the OS, the
/// live persona lane's in-flight buffers, and page-cache churn all need slack, and a
/// lane placed at the exact edge is the one jetsam picks. If a bigger cushion is ever
/// wanted it belongs here, one named constant, not scattered magic.
pub(crate) const LANE_RAM_HEADROOM_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

/// Whether a lane of `footprint` bytes fits in `available` free physical memory with
/// `headroom` to spare. Pure so the load-bearing "would this OOM the machine" decision
/// is unit-tested without process-global memory state. Refuses ONLY when BOTH numbers
/// are known and it genuinely won't fit — an unknown footprint or an unread free-bytes
/// reading is NOT a veto (the pressure-level gate and the placement lease remain the
/// backstops; we never refuse a lane we couldn't size, to avoid starving a node whose
/// probe is momentarily blind).
pub(crate) fn lane_ram_veto(
    available: Option<u64>,
    footprint: Option<u64>,
    headroom: u64,
) -> Result<(), String> {
    if let (Some(avail), Some(fp)) = (available, footprint) {
        let need = fp.saturating_add(headroom);
        if avail < need {
            return Err(format!(
                "refusing to stand up a single-purpose lane: only {avail} B free physical memory, but this \
                 lane needs ~{fp} B (+{headroom} B headroom = {need} B) — standing it up would be \
                 OOM-killed by the OS. A single-purpose lane is deferrable load; free memory (e.g. unload a \
                 resident model) and retry."
            ));
        }
    }
    Ok(())
}

/// How long a lane bringup will WAIT for a transient pressure spike to clear
/// before failing loud. Sized for the real spikes observed live (2026-07-22): a
/// `continuum reboot` cargo build or a sibling model cold-load pushes used/total past
/// 0.90 for a couple of minutes, then the machine returns to Normal. Deferring
/// briefly turns "battery wiped out by a build" into "battery ran 3 minutes
/// later"; a SUSTAINED squeeze still fails loud at the deadline.
pub(crate) const LANE_PRESSURE_WAIT: std::time::Duration = std::time::Duration::from_secs(180);

/// The deferring form of the memory veto: poll until pressure clears or the
/// bounded deadline lapses (then fail loud with the veto's own error). The
/// veto's error message says "retry when pressure clears" — for a DETACHED
/// solve/eval there is no operator watching to retry, so the system honors its
/// own advice for one bounded window. Never a silent wait: each deferred tick
/// probes, so a stall is visible in the trace, not a mystery hang.
pub(crate) async fn await_lane_memory_headroom(footprint: Option<u64>) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + LANE_PRESSURE_WAIT;
    loop {
        let verdict = refuse_lane_under_memory_pressure(footprint);
        match verdict {
            Ok(()) => return Ok(()),
            Err(e) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(e);
                }
                // Log the REFUSAL, not just the pressure level. These are two different
                // vetoes — a High/sustained pressure gate, and a hard free-RAM shortfall
                // — and only one of them is a spike that clears on its own. Printing
                // `level=Normal` beside "waiting for a transient memory spike" while the
                // real cause was a lane that could never fit sent a reader looking at the
                // pressure monitor for 3 minutes (M5, 2026-08-04) while the answer was in
                // `e` the whole time. Whatever we are waiting OUT is the thing to name.
                continuum_base::probe!(
                    class = "lane.memory.pressure_defer",
                    level = ?crate::system_resources::MemoryPressureMonitor::current_level(),
                    refusal = %e,
                    "lane bringup deferred — retrying the refusal below until it clears \
                     or the bounded window lapses"
                );
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// The pure veto decision — split from the static reads so it's unit-testable
/// without mutating process-global pressure state (same split rationale as
/// [`super::acquire_lane_slot`]).
pub(crate) fn lane_memory_veto(
    level: crate::system_resources::PressureLevel,
    gate_closed: bool,
) -> Result<(), String> {
    use crate::system_resources::PressureLevel;
    if level >= PressureLevel::High || gate_closed {
        return Err(format!(
            "refusing to stand up a single-purpose lane under system memory pressure \
             (level={level:?}, sustained_gate_closed={gate_closed}) — a single-purpose \
             lane is deferrable load; retry when pressure clears"
        ));
    }
    Ok(())
}
