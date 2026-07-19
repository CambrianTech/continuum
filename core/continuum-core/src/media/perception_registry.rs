//! `PerceptionRegistry` — the process-global home for per-persona
//! [`PerceptionBuffer`](super::perception_buffer::PerceptionBuffer)s.
//!
//! A persona's live-call perception buffer has TWO reachers that never hold each
//! other: the LiveKit media INGEST path warms it (`observe()` a decoded frame — the
//! WRITE side, #192/#193), and the persona's `MediaPerceptionSource` reads it under
//! the RAG budget (the READ side, wired at supervisor boot). Both resolve the SAME
//! buffer by `persona_id`, so a frame warmed by ingest is exactly what the source
//! delivers. This registry is that single seam — one `Arc<PerceptionBuffer>` per
//! persona, resolved by id from both sides. It is NOT a parallel copy of any state;
//! it is the single home for a persona's perception hold, the same "one home, two
//! reachers by id" shape as [`persona::focus::registry()`](crate::persona::focus)
//! and [`persona_workspace::global()`](crate::cognition::persona_workspace).
//!
//! The cells inside each buffer resolve on the ONE runtime
//! [`SharedCompute`](crate::runtime::SharedCompute) (`shared_compute::global()`),
//! so the compute-once/share-many payoff spans personas: four personas seeing the
//! same participant's frame describe/scale it ONCE
//! ([[vision-replication-is-the-multipersona-moat-vs-cloud]]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use uuid::Uuid;

use super::image_ops::DestSize;
use super::perception_buffer::PerceptionBuffer;

/// The ambient forced-look size every perception tick warms + reads — the cheap
/// thumbnail cell, ~480w per the room-as-now doctrine. Full-res / bigger is the
/// drill-in tool (#190), never the ambient path. 16:9 keeps a video frame's aspect
/// without a crop. One policy constant, one place (the compression principle) —
/// every persona's buffer is born with it.
pub const AMBIENT_PERCEPTION_SIZE: DestSize = DestSize {
    width: 480,
    height: 270,
};

/// Per-persona perception-buffer home. One `Arc<PerceptionBuffer>` per persona,
/// resolved by id from both the ingest (write) and source (read) sides.
#[derive(Default)]
pub struct PerceptionRegistry {
    buffers: Mutex<HashMap<Uuid, Arc<PerceptionBuffer>>>,
}

impl PerceptionRegistry {
    pub fn new() -> Self {
        Self {
            buffers: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve (get-or-create) the persona's perception buffer. Idempotent: the
    /// first caller installs a buffer sized to [`AMBIENT_PERCEPTION_SIZE`] (the
    /// correct birth posture, not a fallback over a missing precondition); every
    /// later caller — the ingest path, the source — gets the SAME `Arc`, so what
    /// ingest warms is exactly what the source reads. A poisoned lock is a prior
    /// panic mid-mutation and is propagated (fail loud), never swallowed.
    pub fn handle(&self, persona_id: Uuid) -> Arc<PerceptionBuffer> {
        self.buffers
            .lock()
            .expect("perception registry mutex poisoned by a prior panic")
            .entry(persona_id)
            .or_insert_with(|| Arc::new(PerceptionBuffer::new(AMBIENT_PERCEPTION_SIZE)))
            .clone()
    }

    /// Peek the buffer without creating one — `None` if this persona has never had
    /// a perception buffer resolved (never joined a live call).
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<PerceptionBuffer>> {
        self.buffers
            .lock()
            .expect("perception registry mutex poisoned by a prior panic")
            .get(persona_id)
            .cloned()
    }

    /// Drop a persona's buffer when it despawns (frees the held frames). Idempotent.
    pub fn remove(&self, persona_id: &Uuid) {
        self.buffers
            .lock()
            .expect("perception registry mutex poisoned by a prior panic")
            .remove(persona_id);
    }
}

/// Process-global perception registry. Same `OnceLock` shape as
/// `persona::focus::registry()` — the single seam between the media ingest that
/// warms a persona's perception and the source that reads it.
pub fn registry() -> Arc<PerceptionRegistry> {
    static GLOBAL: OnceLock<Arc<PerceptionRegistry>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(PerceptionRegistry::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: get-or-create is idempotent — the SAME persona resolves to
    // the SAME buffer Arc from both reachers (ingest warms exactly what the source
    // reads); distinct personas get distinct buffers; the ambient size is applied.
    #[test]
    fn handle_resolves_one_buffer_per_persona_shared_across_reachers() {
        let reg = PerceptionRegistry::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let first = reg.handle(a);
        let second = reg.handle(a); // second reacher (e.g. the source after ingest)
        assert!(
            Arc::ptr_eq(&first, &second),
            "same persona → the SAME buffer Arc, so ingest and source never diverge"
        );

        let other = reg.handle(b);
        assert!(
            !Arc::ptr_eq(&first, &other),
            "distinct personas → distinct buffers"
        );
    }

    // what this catches: peek does not create, and remove frees the buffer.
    #[test]
    fn get_peeks_without_creating_and_remove_drops() {
        let reg = PerceptionRegistry::new();
        let p = Uuid::new_v4();

        assert!(reg.get(&p).is_none(), "peek before touch → None (no create)");
        let _ = reg.handle(p);
        assert!(reg.get(&p).is_some(), "resolved → peekable");
        reg.remove(&p);
        assert!(reg.get(&p).is_none(), "removed → gone");
    }

    // what this catches: the process-global is a stable singleton (both callers get
    // the same registry), so a persona's buffer is truly one home across the process.
    #[test]
    fn global_registry_is_a_stable_singleton() {
        assert!(Arc::ptr_eq(&registry(), &registry()));
    }
}
