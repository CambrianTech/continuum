//! EVERY holder of memory declares itself, or the table says why not yet.
//!
//! # Why this exists (measured 2026-08-19, M5, live `resources/board`)
//!
//! ```text
//! vram: physicalUsed 50.95G   attributed 6.81G (serving)   UNOWNED 44.14G
//! ram:  physicalUsed 50.95G   attributed 0.00G             UNOWNED 50.95G
//! ```
//!
//! 87% of resident memory had no declared owner. The governor could see the bytes were
//! gone and could not say who had them, so it could not ask anyone to give them back.
//!
//! That is not merely an observability gap — it forces the planner's behaviour. A
//! consumer deciding what it can afford sees `capacity − everything resident`, and with
//! nothing attributed, "everything resident" is indistinguishable from "everything
//! immovable". A planner facing all-immovable memory can only shrink. That is the
//! mechanism behind the downshift to a 0.5B on a 64 GB machine (#438) and behind
//! grow-back never happening (#214): both are the same missing attribution, seen from
//! two ends.
//!
//! # The shape is borrowed, deliberately
//!
//! [`disk_eviction::every_cache_class_has_a_decided_eviction_story`](crate::system_resources::disk_eviction)
//! already does exactly this for disk: enumerate every class the substrate writes to,
//! and fail the build on any class with no decided owner. CLAUDE.md calls that test
//! "the difference between a red test and a user's trashed machine". Memory had no
//! twin. This is it.
//!
//! A holder is either **Declared** (it registers as a [`ResourceConsumer`] and shows up
//! in the board's attributions) or **Undeclared** with the task that will fix it. There
//! is no third state, and a new holder that is neither fails
//! [`tests::every_memory_holder_has_a_decided_owner`]. Undeclared is not a shrug: it is
//! a promise that the bytes are COUNTED in the reconciliation below even while nobody
//! can reclaim them, so the class can never be the silent one again.

use super::lease::ResourceKind;

/// Whether a holder can be asked for its bytes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolderStatus {
    /// Registers as a `ResourceConsumer`: it appears in board attributions, the
    /// governor can measure its footprint and ask it to reclaim.
    Declared,
    /// Holds real bytes and declares nothing. The governor sees the consumption in
    /// `physical_used` and cannot attribute or reclaim it. The string names the task
    /// that closes it — never a bare "TODO".
    Undeclared(&'static str),
}

/// One thing that holds memory on this machine.
#[derive(Debug, Clone, Copy)]
pub struct MemoryHolder {
    /// The `consumer_id` it registers under, or the name it WOULD register under.
    pub id: &'static str,
    /// Which axes it draws on. On a unified host these are the same physical pool —
    /// see [`UnifiedMemoryPool`](super::capacity::UnifiedMemoryPool).
    pub kinds: &'static [ResourceKind],
    pub status: HolderStatus,
    /// What it actually holds, in bytes-on-the-machine terms, so a reader can check the
    /// claim against `ps` rather than trust the table.
    pub what: &'static str,
}

const VRAM: &[ResourceKind] = &[ResourceKind::Vram];
const BOTH: &[ResourceKind] = &[ResourceKind::Vram, ResourceKind::Ram];
const RAM: &[ResourceKind] = &[ResourceKind::Ram];

/// THE table. Every process or subsystem on this box that holds non-trivial memory.
///
/// Adding a holder to the substrate without adding it here fails the guard test. That is
/// the point: the failure mode being prevented is a component quietly holding gigabytes
/// that nothing can account for, which is exactly how 44 GB went unowned.
pub fn standard_memory_holders() -> &'static [MemoryHolder] {
    &[
        MemoryHolder {
            id: "serving",
            kinds: BOTH,
            status: HolderStatus::Declared,
            what: "the persona lane's llama-server — model weights + KV across its slots",
        },
        MemoryHolder {
            id: "render",
            kinds: VRAM,
            status: HolderStatus::Declared,
            what: "bevy render surfaces + textures",
        },
        MemoryHolder {
            id: "voice",
            kinds: BOTH,
            status: HolderStatus::Declared,
            what: "STT/TTS models and audio buffers",
        },
        MemoryHolder {
            id: "perception",
            kinds: RAM,
            status: HolderStatus::Declared,
            what: "perception surface buffers",
        },
        MemoryHolder {
            id: "benchmark-staging",
            kinds: RAM,
            status: HolderStatus::Declared,
            what: "benchmark rows held during a fetch + projection; idle between runs",
        },
        // ---- undeclared: real bytes, no owner ------------------------------------
        MemoryHolder {
            id: "vision",
            kinds: BOTH,
            status: HolderStatus::Undeclared(
                "#106/#395: the VL lane is a SECOND llama-server (Qwen2.5-VL-7B on :58091) \
                 measured at 9.4 GB resident and registers nothing. It is the single \
                 largest unowned holder, and because it never yields, a 27B and vision \
                 cannot be planned against the same pool. VL-as-consumer is the keystone \
                 #395 names: it must lease, report a footprint, and tier down or unload \
                 when a larger base is planned",
            ),
            what: "vision llama-server: VL weights + mmproj + its own KV",
        },
        MemoryHolder {
            id: "embed",
            kinds: VRAM,
            status: HolderStatus::Undeclared(
                "#225: EMBED_LANE_CONSUMER_ID exists and the lane holds a RESERVATION \
                 floor the governor honours, but it registers no consumer — so its \
                 weights are reserved-against yet never attributed. Half-wired is the \
                 worst state: serving budgets around a floor for bytes nobody reports",
            ),
            what: "embedding model weights in the llama --embedding lane",
        },
        MemoryHolder {
            id: "core",
            kinds: RAM,
            status: HolderStatus::Undeclared(
                "#178: the core process itself (~1 GB RSS, and the memleak tracker \
                 reports per-command growth). It is the one holder that can never be \
                 reclaimed by asking, so its job is to be COUNTED — otherwise its \
                 growth silently shrinks every other consumer's budget",
            ),
            what: "the continuum-core process RSS",
        },
        MemoryHolder {
            id: "grade-subprocess",
            kinds: RAM,
            status: HolderStatus::Undeclared(
                "#381: pytest/venv subprocesses under a grade run. Bounded and \
                 short-lived, but they escape their parent's death today, so a \
                 crashed run leaves them holding — unowned AND unreaped",
            ),
            what: "benchmark grading subprocesses (pytest, venv builds)",
        },
        MemoryHolder {
            id: "forge-train",
            kinds: BOTH,
            status: HolderStatus::Undeclared(
                "#99/#137: mlx_lm training jobs hold GPU memory for the duration of a \
                 forge and are the most likely thing to collide with a big resident \
                 base. They already fail loud when orphaned across reboots; they still \
                 do not declare what they hold while running",
            ),
            what: "training/forge jobs (mlx_lm) during a genome run",
        },
    ]
}

/// The reconciliation, per kind: what the machine says is resident, versus what anyone
/// admits to holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconciliation {
    pub kind: ResourceKind,
    /// Measured resident bytes for this axis (`CapacitySource::used_bytes`).
    pub physical_used: u64,
    /// Sum of every consumer's declared footprint.
    pub attributed: u64,
    /// Bytes the board believes belong to processes outside this substrate.
    pub external: u64,
    /// `physical_used − attributed − external`, floored at 0. Bytes that ARE resident,
    /// are NOT ours by the board's reckoning, and are NOT claimed by anyone — i.e. an
    /// undeclared holder of ours, or an external process the external estimate missed.
    /// Either way the planner cannot reason about them.
    pub unowned: u64,
}

impl Reconciliation {
    pub fn compute(
        kind: ResourceKind,
        physical_used: u64,
        attributed: u64,
        external: u64,
    ) -> Self {
        let unowned = physical_used
            .saturating_sub(attributed)
            .saturating_sub(external);
        Self {
            kind,
            physical_used,
            attributed,
            external,
            unowned,
        }
    }

    /// Unowned bytes as a fraction of resident. This is the number to watch: it was
    /// 0.87 on the day this module was written.
    pub fn unowned_fraction(&self) -> f64 {
        if self.physical_used == 0 {
            return 0.0;
        }
        self.unowned as f64 / self.physical_used as f64
    }

    /// A one-line operator-readable verdict. Deliberately states the CONSEQUENCE, not
    /// just the number — an unowned fraction means nothing to a reader who does not
    /// know it forces the planner to treat memory as immovable.
    pub fn explain(&self) -> String {
        format!(
            "{}: {:.2}G resident, {:.2}G attributed, {:.2}G external, {:.2}G UNOWNED ({:.0}%) \
             — unowned bytes cannot be reclaimed or planned against, so they read to the \
             planner as immovable and only ever shrink what it will choose",
            self.kind.label(),
            self.physical_used as f64 / (1024.0 * 1024.0 * 1024.0),
            self.attributed as f64 / (1024.0 * 1024.0 * 1024.0),
            self.external as f64 / (1024.0 * 1024.0 * 1024.0),
            self.unowned as f64 / (1024.0 * 1024.0 * 1024.0),
            self.unowned_fraction() * 100.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a new memory holder added to the substrate with no decision
    // about who owns its bytes. This is the memory twin of disk's
    // `every_cache_class_has_a_decided_eviction_story`, and it exists because 44 GB of
    // 51 GB went unowned on a live box without one line of code being wrong — every
    // component worked; nothing was accountable.
    #[test]
    fn every_memory_holder_has_a_decided_owner() {
        for h in standard_memory_holders() {
            match h.status {
                HolderStatus::Declared => {}
                HolderStatus::Undeclared(reason) => {
                    assert!(
                        reason.contains('#'),
                        "holder '{}' is undeclared without naming the task that closes it — \
                         'undeclared' is a tracked promise, never a shrug",
                        h.id
                    );
                    assert!(
                        reason.len() > 40,
                        "holder '{}' names a task but does not say what it holds or why it \
                         is not yet declared; a future reader must be able to act on this \
                         without re-deriving the incident",
                        h.id
                    );
                }
            }
            assert!(!h.kinds.is_empty(), "holder '{}' declares no axis", h.id);
            assert!(!h.what.is_empty(), "holder '{}' says nothing about what it holds", h.id);
        }
    }

    // what this catches: the table drifting out of sync with the constants the code
    // actually registers under. A rename of SERVING_CONSUMER_ID that left this table
    // saying "serving" would silently make the reconciliation attribute nothing —
    // prose cannot be allowed to fake wiring (the #344 property).
    #[test]
    fn declared_holders_use_the_ids_the_code_registers_under() {
        let declared: Vec<&str> = standard_memory_holders()
            .iter()
            .filter(|h| h.status == HolderStatus::Declared)
            .map(|h| h.id)
            .collect();
        assert!(declared.contains(&crate::modules::serving_consumer::SERVING_CONSUMER_ID));
        assert!(declared.contains(&crate::modules::perception_consumer::PERCEPTION_CONSUMER_ID));
        assert!(declared.contains(&crate::cognition::bench_staging::CONSUMER_ID));
        // `embed` has a constant but is deliberately NOT declared (#225) — if someone
        // registers it as a consumer, this assertion is the reminder to move its row.
        assert!(
            !declared.contains(&crate::inference::llamacpp_adapter::EMBED_LANE_CONSUMER_ID),
            "embed now registers as a consumer — move its row from Undeclared to Declared"
        );
    }

    // what this catches: the reconciliation arithmetic silently underflowing or
    // over-crediting. Uses the REAL board numbers from the incident so the test is the
    // measurement, not an invented example.
    #[test]
    fn reconciliation_reproduces_the_incident() {
        // vram, 2026-08-19: 50.95G resident, 6.81G attributed to serving, external not
        // yet separated (0) — the state that made the planner choose a 0.5B.
        let r = Reconciliation::compute(
            ResourceKind::Vram,
            54_712_046_387, // 50.95 GiB
            7_312_233_267,  // 6.81 GiB
            0,
        );
        assert!(
            r.unowned_fraction() > 0.85,
            "the incident had ~87% unowned; got {:.0}%",
            r.unowned_fraction() * 100.0
        );
        assert!(r.explain().contains("UNOWNED"));
    }

    // what this catches: attribution exceeding residency (double-counted consumers)
    // producing a wrapped, enormous `unowned`. Saturating, never wrapping.
    #[test]
    fn over_attribution_floors_at_zero_rather_than_wrapping() {
        let r = Reconciliation::compute(ResourceKind::Ram, 1_000, 800, 400);
        assert_eq!(r.unowned, 0);
        assert_eq!(r.unowned_fraction(), 0.0);
    }
}
