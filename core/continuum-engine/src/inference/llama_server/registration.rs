//! Metadata is a projection of the child owner's retained launch, not its custody.
use super::*;
use crate::inference::lane_registry::{self, LaneRecord};

impl LlamaServerProcess {
    pub(super) fn retain_registration(
        &self,
        generation: &crate::inference::slots::EngineGeneration,
        record: LaneRecord,
    ) {
        let Ok(mut guard) = self.child.lock() else {
            return;
        };
        if let Some(owned) = guard.as_mut().filter(|owned| {
            owned.generation.same_engine(generation)
                && owned.child.id() == Some(record.pid)
                && !owned.generation.has_exited()
        }) {
            owned.registration = Some(record);
        }
    }

    pub(super) fn registered_lane(&self) -> Option<LaneRecord> {
        let (_, port) = split_host_port(&self.root);
        let guard = self.child.lock().ok()?;
        if let Some(owned) = guard.as_ref() {
            if !self.is_live_lane
                || owned.generation.has_exited()
                || owned.verified_target.is_none()
            {
                return None;
            }
            let expected = owned.registration.as_ref()?;
            if owned.child.id() != Some(expected.pid) || expected.port != port {
                return None;
            }
            if let Some(record) = lane_registry::live_lane().filter(|record| {
                record.incarnation() == expected.incarnation() && expected.started_s != 0
            }) {
                return Some(record);
            }
            // Keep the owner lock across publication: replacement/retirement cannot race this
            // generation's repair. Never turn a saved record into authority after child exit.
            if let Err(error) = lane_registry::restore_owned(expected) {
                tracing::warn!(target: "continuum_core::inference::llama_server", %error,
                    "owned lane registration recovery refused");
                return None;
            }
        }
        // An adopted predecessor has no child custody here. Its existing registered identity
        // remains required; discovery from an arbitrary listener is deliberately not a repair.
        lane_registry::live_lane().filter(|record| record.port == port)
    }
}
