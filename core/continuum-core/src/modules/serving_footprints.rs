//! The footprint adapters for the two lanes this node serves: the persona lane and
//! whatever currently provides VISION.
//!
//! Both answer the same question against the same source of truth — the model catalog's
//! [`ModelFootprint`] sizing (`weights + lanes × kv(window) + prefill compute reserve`)
//! evaluated at the LANE'S OWN live shape. Nothing here invents a size, and nothing here
//! caches one: the model id, the served window and the lane count are re-read from the
//! live [`ServingSnapshot`] on every call, so swapping the model or resizing the window
//! moves the number on the next tick with no invalidation step to forget.
//!
//! # Why "the vision provider" and not "the sidecar"
//!
//! Joel, 2026-08-19: *"or the feature really"*. Vision on this node is provided by ONE
//! of two things — the main persona lane when its own model sees (Qwen3.8-27B does), or a
//! separate describe sidecar when it does not. [`VisionFootprintSource`] is keyed on the
//! FEATURE: it reads whichever endpoint `ServingSnapshot` has verified, so a change of
//! provider needs no edit here.
//!
//! That framing also carries the correctness case. When the main lane is the vision
//! provider, its bytes are ALREADY charged by serving — attributing them again would
//! double-count one residency across two holders, which is the precise defect that had
//! VRAM and RAM disagreeing by 23 GB about one physical pool. So the main-lane case
//! reports a genuine `Measured(0)`: the vision FEATURE holds no bytes of its own.
//!
//! # Estimated today, Measured later, catalog-durable in the end
//!
//! Every reading here is [`Provenance::Estimated`] — honestly so. It is derived from
//! catalog metadata via a calculation that is already good (window-scaled, lane-scaled,
//! calibrated against a live `MTL0 compute buffer` measurement), but nothing has yet
//! compared it against the process's real RSS. When that comparison exists, the delta
//! belongs on the CATALOG ROW as the durable per-model calibration — the estimate
//! graduates to `Measured` and every later node inherits the learned value instead of
//! re-deriving it. The provenance field is what makes that graduation observable rather
//! than silent.

use std::sync::Arc;

use tokio::sync::watch;

use crate::inference::llama_server::ServingSnapshot;
use crate::inference::vision_sidecar::SIDECAR_CTX;
use crate::model_registry::live::ModelCatalog;
use crate::resources::footprint_source::{FootprintReading, FootprintSource, Provenance};
use crate::resources::lease::ResourceKind;

/// Which lane on the live snapshot an adapter answers for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    /// The persona serving lane — `active_model` at `served_context_window` × `lanes`.
    Persona,
    /// Whatever currently provides vision, main lane or sidecar (#106).
    Vision,
}

/// The live shape of a lane, pulled out of the snapshot so the sizing step is a pure
/// function of it and can be unit-tested without a watch channel or a catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneShape {
    pub model_id: String,
    pub window: u32,
    pub lanes: u32,
}

/// What a lane contributes, before the catalog is consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaneClaim {
    /// This lane holds bytes of its own; size `shape` against the catalog.
    Holds(LaneShape),
    /// This holder genuinely holds nothing right now — a real zero, not a missing
    /// answer. (Vision not ready, or provided by a lane another holder already charges.)
    HoldsNothing,
    /// The lane is LIVE but its shape cannot be determined, so its size is unknown.
    /// Never zero: a live lane holding gigabytes must not read as an empty one.
    Live(&'static str),
}

/// Read a lane's live shape off the snapshot. Pure, so every branch below — including
/// the double-count guard that is easy to get wrong and impossible to see go wrong — is
/// testable from a hand-built snapshot.
pub fn lane_claim(snap: &ServingSnapshot, vision: bool) -> LaneClaim {
    if !vision {
        if !snap.ready {
            return LaneClaim::HoldsNothing;
        }
        return match (&snap.active_model, snap.served_context_window) {
            (Some(id), w) if w > 0 => LaneClaim::Holds(LaneShape {
                model_id: id.clone(),
                window: w,
                lanes: snap.lanes,
            }),
            (Some(_), _) => LaneClaim::Live("ready lane with no served window"),
            (None, _) => LaneClaim::Live("ready lane with no active model"),
        };
    }

    if !snap.vision_ready {
        return LaneClaim::HoldsNothing;
    }

    // THE DOUBLE-COUNT GUARD. When the verified vision endpoint IS the persona lane,
    // serving already charges every one of those bytes. Charging them again would
    // inflate attributed residency by a whole model and shrink everyone's budget by
    // the same amount — a self-inflicted scarcity that looks exactly like real
    // pressure. The vision FEATURE holds nothing of its own here, and that is a
    // measured zero, not an unknown.
    if snap.vision_base_url.as_deref() == Some(snap.base_url.as_str()) {
        return LaneClaim::HoldsNothing;
    }

    match &snap.vision_model {
        // A separate describe sidecar: one lane, at the window `ensure_sidecar` spawns
        // it with. SIDECAR_CTX is read from the one constant the spawn uses, never
        // copied — a second copy is how a size silently stops tracking the thing.
        Some(id) => LaneClaim::Holds(LaneShape {
            model_id: id.clone(),
            window: SIDECAR_CTX,
            lanes: 1,
        }),
        None => LaneClaim::Live("vision verified but no model named"),
    }
}

/// One footprint adapter over the live catalog + serving board.
///
/// A LEAF, per the [`FootprintSource`] acyclicity rule: it reads a catalog snapshot and
/// a `watch` borrow — never the governor, never a budget. Both reads are non-blocking,
/// which is what lets this be called on the accounting tick.
pub struct CatalogFootprintSource {
    holder_id: &'static str,
    lane: Lane,
    catalog: Arc<ModelCatalog>,
    serving: watch::Receiver<ServingSnapshot>,
}

impl CatalogFootprintSource {
    /// The persona serving lane's own residency.
    pub fn persona(catalog: Arc<ModelCatalog>, serving: watch::Receiver<ServingSnapshot>) -> Self {
        Self {
            holder_id: "serving",
            lane: Lane::Persona,
            catalog,
            serving,
        }
    }

    /// Whatever currently provides vision (#106/#395).
    pub fn vision(catalog: Arc<ModelCatalog>, serving: watch::Receiver<ServingSnapshot>) -> Self {
        Self {
            holder_id: "vision",
            lane: Lane::Vision,
            catalog,
            serving,
        }
    }

    /// Size a shape against the catalog. `Unknown` — never 0 — when the row will not
    /// resolve, because the lane is live and the bytes are real whether or not we can
    /// name them.
    fn size(&self, shape: &LaneShape) -> FootprintReading {
        match self
            .catalog
            .snapshot()
            .get(&shape.model_id)
            .and_then(|live| crate::modules::serving_daemon::footprint_for(&live.model))
        {
            Some(fp) => FootprintReading::estimated(
                ResourceKind::Vram,
                fp.peak_resident_bytes(shape.window, shape.lanes),
            ),
            None => FootprintReading::unknown(ResourceKind::Vram),
        }
    }
}

impl FootprintSource for CatalogFootprintSource {
    fn holder_id(&self) -> &str {
        self.holder_id
    }

    fn read(&self) -> Vec<FootprintReading> {
        let claim = lane_claim(&self.serving.borrow(), self.lane == Lane::Vision);
        let reading = match &claim {
            LaneClaim::Holds(shape) => self.size(shape),
            LaneClaim::HoldsNothing => FootprintReading::measured(ResourceKind::Vram, 0),
            LaneClaim::Live(_) => FootprintReading::unknown(ResourceKind::Vram),
        };

        // The seam Joel asked to be able to interrogate: every term that produced the
        // number, plus its provenance, at the moment the decision is made. Without this
        // a bad grant can only be re-derived by guessing what the inputs were — which
        // is how a whole day went to a monitor that had been publishing a silent zero.
        if !matches!(reading.provenance, Provenance::Measured) || reading.bytes > 0 {
            let (model, window, lanes) = match &claim {
                LaneClaim::Holds(s) => (s.model_id.as_str(), s.window, s.lanes),
                _ => ("-", 0, 0),
            };
            crate::probe!(
                class = "resources.footprint",
                holder = self.holder_id,
                model = model,
                window = window,
                lanes = lanes,
                bytes = reading.bytes,
                provenance = format!("{:?}", reading.provenance).as_str(),
                "footprint reading",
            );
        }

        vec![reading]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> ServingSnapshot {
        ServingSnapshot::empty()
    }

    // what this catches: THE double-count. When the persona lane is itself the verified
    // vision endpoint (Qwen3.8-27B sees), the vision holder must contribute ZERO —
    // charging the same residency to two holders inflates attributed bytes by a whole
    // model and manufactures scarcity indistinguishable from the real thing.
    #[test]
    fn vision_provided_by_the_main_lane_holds_nothing_of_its_own() {
        let mut s = snap();
        s.ready = true;
        s.base_url = "http://127.0.0.1:58080".into();
        s.active_model = Some("qwen3.8-27b".into());
        s.served_context_window = 26_368;
        s.lanes = 2;
        s.vision_ready = true;
        s.vision_base_url = Some("http://127.0.0.1:58080".into());
        s.vision_model = Some("qwen3.8-27b".into());

        assert_eq!(lane_claim(&s, true), LaneClaim::HoldsNothing);
        // ...while the persona lane still charges the full residency once.
        assert_eq!(
            lane_claim(&s, false),
            LaneClaim::Holds(LaneShape {
                model_id: "qwen3.8-27b".into(),
                window: 26_368,
                lanes: 2,
            })
        );
    }

    // what this catches: a separate sidecar going unattributed. This is the ~9.4 GB that
    // read as UNOWNED on the live board, and unowned reads as immovable — so the planner
    // treated a releasable lane as permanent and refused a model that fits.
    #[test]
    fn a_separate_vision_sidecar_is_charged_at_the_window_the_spawn_uses() {
        let mut s = snap();
        s.ready = true;
        s.base_url = "http://127.0.0.1:58080".into();
        s.active_model = Some("devstral-24b".into());
        s.served_context_window = 16_384;
        s.lanes = 2;
        s.vision_ready = true;
        s.vision_base_url = Some("http://127.0.0.1:58091".into());
        s.vision_model = Some("qwen2.5-vl-7b".into());

        assert_eq!(
            lane_claim(&s, true),
            LaneClaim::Holds(LaneShape {
                model_id: "qwen2.5-vl-7b".into(),
                window: SIDECAR_CTX,
                lanes: 1,
            }),
            "sized at the sidecar's own spawn window, not the persona lane's"
        );
    }

    // what this catches: a live lane whose shape can't be read degrading to zero bytes.
    // A ready server we cannot size still holds gigabytes; reporting 0 tells every other
    // consumer those bytes are free and invites the concurrent OOM.
    #[test]
    fn a_live_lane_with_an_unreadable_shape_is_unknown_not_zero() {
        let mut s = snap();
        s.ready = true;
        s.active_model = Some("devstral-24b".into());
        s.served_context_window = 0; // /props not read yet
        assert!(matches!(lane_claim(&s, false), LaneClaim::Live(_)));

        let mut v = snap();
        v.vision_ready = true;
        v.base_url = "http://127.0.0.1:58080".into();
        v.vision_base_url = Some("http://127.0.0.1:58091".into());
        v.vision_model = None;
        assert!(matches!(lane_claim(&v, true), LaneClaim::Live(_)));
    }

    // what this catches: "not serving" being conflated with "cannot tell". Nothing
    // running is a real, usable zero — the distinction the Provenance ladder exists for.
    #[test]
    fn nothing_serving_is_a_real_zero() {
        assert_eq!(lane_claim(&snap(), false), LaneClaim::HoldsNothing);
        assert_eq!(lane_claim(&snap(), true), LaneClaim::HoldsNothing);
    }
}
