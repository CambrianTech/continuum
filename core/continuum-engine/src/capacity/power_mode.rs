//! The user-facing power spectrum (`PowerMode`) and the serving mode derived from memory
//! pressure. Moved down from `provisioning::model_catalog` so the serving planner crate and the
//! provisioning catalog share one owner; core re-exports the old paths.

/// The user-facing power spectrum — exactly a car's drive modes (Eco · Comfort · Sport ·
/// Performance), and "tuners" for the power user: a first-class AI citizen or the human
/// can define their own `ScalingPolicy`. `Comfort` is the
/// default — economical by design (the "32 mpg" that still launches when you floor it).
/// The mode sets how much of the box this one request may use, whether it climbs to a
/// bigger brain, and whether raw float weights are on the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerMode {
    /// Max efficiency — battery, thermal, a crowded call. Small share of the box.
    Eco,
    /// Everyday default — good quality, shares the box with the rest of the call.
    #[default]
    Comfort,
    /// Quality-leaning — most of the machine, and climb to a bigger model.
    Sport,
    /// Floor it — nearly the whole box, the biggest brain + fidelity that fits, F16 on
    /// the table. The teacher that trains the others, a substantial iOS build.
    Performance,
}

impl PowerMode {
    /// Raw float weights (F16) allowed — only when flooring it.
    pub fn allows_float(self) -> bool {
        matches!(self, PowerMode::Performance)
    }
    /// Climb the model-size ladder to a bigger BRAIN, not just a bigger quant.
    pub fn climbs_ladder(self) -> bool {
        matches!(self, PowerMode::Sport | PowerMode::Performance)
    }

    /// Fraction of currently-FREE memory the shared serving base may claim — the rest
    /// is left for the other personas' KV cache, the renderer, and LiveKit. Eco leaves
    /// the most (a crowded call, battery); Performance floors it (a solo demanding
    /// session). This is the resiliency reserve that keeps 14 personas from OOM-ing the
    /// pool: serving never grabs everything, so the KV of the rest of the call still fits.
    /// The EVERYDAY drive mode from the operator's headroom policy
    /// (`CONTINUUM_VRAM_HEADROOM`, "a dedicated foundry sets 1.0"): the mode whose
    /// serving fraction that policy names. A headless inference node at 1.0 plans at
    /// Performance every day, not only while a detached benchmark holds a Ludicrous
    /// lease; a shared desktop at the 0.8 default stays Comfort, byte-identical to
    /// before. Pressure can still force Eco below this; nothing raises above it but
    /// a Ludicrous hold. Measured 2026-09-17 on the 5090: at Comfort a 27B on 32 GB
    /// fits 1 × 37k or 2 × 11.6k, and two residents made the plan pick 2 × 11.6k —
    /// too narrow for a solve turn (128 `over_window` in 25 min on a claimed card)
    /// on a box that ran the same model at 31.9 GB all night.
    pub fn everyday_for_headroom(headroom: f64) -> PowerMode {
        if headroom >= 0.99 {
            PowerMode::Performance
        } else if headroom >= 0.90 {
            PowerMode::Sport
        } else if headroom >= 0.70 {
            PowerMode::Comfort
        } else {
            PowerMode::Eco
        }
    }

    /// The fraction of the replace-myself budget the plan may size to. Since #4230 this
    /// is THE headroom the plan keeps (the board has already subtracted every other
    /// holder's live residency; the plan withholds nothing more), so no mode may hand
    /// the plan 1.0: the footprint has no term for the serving process's own fixed
    /// overhead — the CUDA context + cuBLAS workspace on a discrete card, the Metal heap
    /// + command queues on UMA — which llama-server allocates on top of weights + KV +
    /// compute. Performance (the benchmark's mode, `ServingLudicrousHold`) keeps 4% for
    /// exactly that: measured 2026-09-19 on the 5090, 2.3 GB of slack on a 32.6 GB card
    /// under a 30 GB plan, and a plan sized to 100% of the ceiling is an OOM by
    /// arithmetic (BigMama's condition on #4230).
    pub fn serving_fraction(self) -> f64 {
        match self {
            PowerMode::Eco => 0.55,
            PowerMode::Comfort => 0.80,
            PowerMode::Sport => 0.92,
            PowerMode::Performance => 0.96,
        }
    }
}

/// The shared-base serving mode from live memory pressure — the auto-downshift. When
/// free memory is tight (many personas resident, a game opened), drop to Eco so the base
/// claims less and the rest of the call's KV + render still fit; at normal headroom stay
/// Comfort. This is the load test's exact failure mode (KV ceiling under a crowd) handled
/// automatically: serving reserves harder precisely when the pool is under strain, never
/// grabbing its way into an OOM. 8 GiB matches the `DefaultScalingPolicy` starved-box line.
pub fn serving_mode_for_pressure(available_bytes: u64) -> PowerMode {
    serving_mode_for_pressure_from(available_bytes, None)
}

/// Eco is ENTERED below this much available memory…
pub const ECO_ENTER_BYTES: u64 = 8 * (1 << 30);
/// …and LEFT only above this much: a band, not a line. macOS's "available" on a
/// swapping UMA box wanders across any single threshold once a minute (measured
/// 2026-09-17 09:47–10:10Z: usable_gb 20 ↔ 30, the plan 6 ↔ 7 lanes, `warm-slot-
/// oversubscribed` every other tick — and every flip reset the reconcile's sustain
/// streak, so no re-home could ever fire). A mode change is a plan change is a
/// relaunch; it must cost a real move in pressure, not noise.
pub const ECO_EXIT_BYTES: u64 = 12 * (1 << 30);

/// [`serving_mode_for_pressure`] with hysteresis: from Eco, stay until available clears
/// [`ECO_EXIT_BYTES`]; otherwise Eco below [`ECO_ENTER_BYTES`], Comfort above. A caller
/// with no previous mode gets the plain line.
pub fn serving_mode_for_pressure_from(available_bytes: u64, previous: Option<PowerMode>) -> PowerMode {
    match previous {
        Some(PowerMode::Eco) if available_bytes < ECO_EXIT_BYTES => PowerMode::Eco,
        _ if available_bytes < ECO_ENTER_BYTES => PowerMode::Eco,
        _ => PowerMode::Comfort,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (2026-09-17, the 5090): the operator's headroom policy is the
    // everyday drive mode. The 0.8 default is Comfort — byte-identical to before — and
    // a dedicated node at 1.0 plans at Performance every day, not only under an exam's
    // Ludicrous hold. Below Comfort's band the node asks for Eco on purpose.
    #[test]
    fn the_everyday_drive_mode_is_the_operators_headroom_policy() {
        assert_eq!(PowerMode::everyday_for_headroom(0.8), PowerMode::Comfort);
        assert_eq!(PowerMode::everyday_for_headroom(1.0), PowerMode::Performance);
        assert_eq!(PowerMode::everyday_for_headroom(0.92), PowerMode::Sport);
        assert_eq!(PowerMode::everyday_for_headroom(0.5), PowerMode::Eco);
        assert_eq!(
            PowerMode::everyday_for_headroom(0.8).serving_fraction(),
            PowerMode::Comfort.serving_fraction()
        );
    }

    // what this catches: the serving reserve is monotonic (Eco leaves the most free for
    // the rest of the call, Performance claims nearly all of it) — the knob that keeps a
    // crowded call from OOM-ing the shared pool — and since #4230 it is the plan's ONLY
    // headroom, so even Performance keeps the serving process's own fixed overhead
    // (CUDA context / Metal heap) out of the plan: a mode at 1.0 would size weights + KV
    // + compute to 100% of the ceiling and OOM by arithmetic on the benchmark path.
    #[test]
    fn serving_fraction_is_monotonic_and_even_performance_keeps_the_process_overhead() {
        assert!(PowerMode::Eco.serving_fraction() < PowerMode::Comfort.serving_fraction());
        assert!(PowerMode::Comfort.serving_fraction() < PowerMode::Sport.serving_fraction());
        assert!(PowerMode::Sport.serving_fraction() < PowerMode::Performance.serving_fraction());
        assert!(PowerMode::Performance.serving_fraction() < 1.0, "no mode hands the plan the whole ceiling");
        assert!(PowerMode::Performance.serving_fraction() >= 0.95, "but Performance takes nearly all of it");
    }

    // what this catches: the auto-downshift — tight free memory picks Eco (reserve
    // hardest), roomy picks Comfort. The base reserves harder exactly under strain.
    // what this catches (2026-09-17): a single 8 GiB line under a wandering memory read
    // flipped Eco ↔ Comfort once a minute, the plan 6 ↔ 7 lanes with it, and every flip
    // reset the re-home streak. From Eco the read must clear the EXIT band to leave;
    // from Comfort it must fall below the ENTER line to drop; a caller without history
    // gets the plain line.
    #[test]
    fn the_serving_mode_holds_through_a_flapping_read() {
        let gib = 1u64 << 30;
        // Fresh: the line.
        assert_eq!(serving_mode_for_pressure_from(7 * gib, None), PowerMode::Eco);
        assert_eq!(serving_mode_for_pressure_from(9 * gib, None), PowerMode::Comfort);
        // From Eco, 9 GiB is inside the band: stay Eco; 12 GiB leaves it.
        assert_eq!(serving_mode_for_pressure_from(9 * gib, Some(PowerMode::Eco)), PowerMode::Eco);
        assert_eq!(serving_mode_for_pressure_from(12 * gib, Some(PowerMode::Eco)), PowerMode::Comfort);
        // From Comfort, 9 GiB stays Comfort; 7 GiB drops.
        assert_eq!(serving_mode_for_pressure_from(9 * gib, Some(PowerMode::Comfort)), PowerMode::Comfort);
        assert_eq!(serving_mode_for_pressure_from(7 * gib, Some(PowerMode::Comfort)), PowerMode::Eco);
    }

    #[test]
    fn serving_downshifts_to_eco_under_pressure() {
        assert_eq!(serving_mode_for_pressure(4 * (1 << 30)), PowerMode::Eco);
        assert_eq!(
            serving_mode_for_pressure(32 * (1 << 30)),
            PowerMode::Comfort
        );
    }
}
