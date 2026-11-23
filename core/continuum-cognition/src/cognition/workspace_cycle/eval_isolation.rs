use super::*;

/// RAII guard for a memory-isolated measurement window over a cycle's
/// hippocampus — see [`WorkspaceCycle::isolate_for_eval`]. Holds the persona's
/// `AdmissionState`, the frame to rewind to, and the real persistence sink to
/// restore. All `Option` so a pure-cognition cycle (no hands) yields a benign
/// no-op guard. Not `Clone` — the restore must happen exactly once, on drop.
pub struct EvalIsolation {
    pub(super) admission: Option<Arc<crate::cognition::admission_state::AdmissionState>>,
    pub(super) checkpoint: Option<crate::cognition::admission_state::AdmissionCheckpoint>,
    pub(super) real_sink: Option<Arc<dyn crate::cognition::admission_persistence::AdmissionPersistenceSink>>,
    /// The shared decoding handle the guard forced to greedy on creation — restored
    /// to relaxed (`None`) on drop. Carried even for a no-hands (pure-cognition)
    /// cycle, because a reproducible metric needs deterministic generation whether
    /// or not she acts.
    pub(super) decoding: Option<DecodingHandle>,
}

impl EvalIsolation {
    /// Rewind the persona's in-memory admission frame to the checkpoint taken
    /// when the guard was created — call BETWEEN A/B arms so the base and
    /// candidate arms start from identical memory (the only difference the lift
    /// measures is the genome, never accumulated engrams). No-op for a
    /// pure-cognition cycle.
    pub fn rewind(&self) {
        if let (Some(admission), Some(checkpoint)) = (&self.admission, &self.checkpoint) {
            admission.restore(checkpoint);
        }
    }
}

impl Drop for EvalIsolation {
    fn drop(&mut self) {
        // Restore relaxed decoding (her lived warmth) first — independent of hands,
        // so even a pure-cognition eval leaves her sampling normally afterward.
        if let Some(decoding) = &self.decoding {
            decoding.store(Arc::new(None));
        }
        let Some(admission) = &self.admission else {
            return;
        };
        // Rewind the memory frame, THEN restore the real sink — order matters:
        // restoring the sink first could let a racing observe land a write the
        // rewind was meant to erase. With the sink still muted, the rewind is
        // the last word on what disk will ever see from this window.
        if let Some(checkpoint) = &self.checkpoint {
            admission.restore(checkpoint);
        }
        if let Some(sink) = self.real_sink.take() {
            admission.swap_persistence(sink);
        }
    }
}
