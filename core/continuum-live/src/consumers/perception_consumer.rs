//! `PerceptionConsumer` — perception's seat at the resource-governor table.
//!
//! Live-call perception holds RAM: one bounded [`FrameRing`](crate::media::PerceptionBuffer)
//! per source per persona, each a handful of decoded frame blobs. That is real, reclaimable
//! residency, so — exactly like [`ServingConsumer`](super::serving_consumer) (VRAM),
//! [`BevyConsumer`](super::bevy_consumer) (render VRAM), and
//! [`VoiceConsumer`](super::live_session_consumer) (TTS) — perception registers as a peer
//! [`ResourceConsumer`] and LEASES rather than growing unaccounted
//! ([[resource-authority-is-a-system-concern]], #56). It reports its footprint and, when the
//! authority asks for bytes back under pressure, evicts oldest ring frames — always keeping
//! each source's HEAD so the persona is never blinded (room-as-now survives a reclaim).
//!
//! Perception does NOT hold VRAM: the vision-describe it triggers runs on the SERVED VL model,
//! whose VRAM is [`ServingConsumer`]'s footprint — perception is a *demander* of that lane, not
//! a holder of its bytes. The shared derivative cache (thumbnails/descriptions/signatures on
//! `SharedCompute`) is likewise not double-counted here; this consumer accounts only the frame
//! rings perception owns.

use std::sync::Arc;

use async_trait::async_trait;

use crate::media::perception_registry::PerceptionRegistry;
use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimRequest, ReclaimStatus, ResourceConsumer,
    ResourceKind,
};

/// Stable id matching this consumer's leases in the governor ledger.
pub const PERCEPTION_CONSUMER_ID: &str = "perception";

/// Accounts + reclaims the RAM held by every persona's perception frame rings, read live
/// from the process-global [`PerceptionRegistry`].
pub struct PerceptionConsumer {
    registry: Arc<PerceptionRegistry>,
}

impl PerceptionConsumer {
    pub fn new(registry: Arc<PerceptionRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ResourceConsumer for PerceptionConsumer {
    fn consumer_id(&self) -> &str {
        PERCEPTION_CONSUMER_ID
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        let bytes = self.registry.total_resident_bytes();
        // Idle (no live call / no frames) → report nothing, like the sibling consumers do
        // when not running: no zero-byte noise for the daemon to reconcile.
        if bytes == 0 {
            return Vec::new();
        }
        vec![ConsumerFootprint {
            kind: ResourceKind::Ram,
            bytes,
            detail: "live-call perception frame rings".to_string(),
        }]
    }

    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
        // Perception holds only RAM. An ask for any other kind is honestly refused (named,
        // never a silent freed=0) so the authority does not misread it.
        if request.kind != ResourceKind::Ram {
            return ReclaimOutcome::refused(format!(
                "perception holds no {:?}, only Ram (frame rings)",
                request.kind
            ));
        }

        let freed = self.registry.evict_at_least(request.target_bytes);
        if freed >= request.target_bytes {
            ReclaimOutcome::released(freed)
        } else {
            // We kept each source's head (room-as-now) — freeing more would blind a live
            // perceiver. Honest Partial: the authority keeps the lease and may re-ask.
            ReclaimOutcome {
                freed_bytes: freed,
                status: ReclaimStatus::Partial,
                detail: Some("kept each source's current frame (room-as-now)".to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::image_ops::DestSize;
    use crate::media::{MediaFrame, PerceptionBuffer};
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use uuid::Uuid;

    const AMBIENT: DestSize = DestSize {
        width: 32,
        height: 24,
    };

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, Rgba([(w % 256) as u8, 0, 0, 255]));
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// A registry with one persona whose ring holds three DISTINCT frames (seeded directly,
    /// no async warm). Returns the registry + the known resident-byte total.
    fn registry_with_three_frames() -> (Arc<PerceptionRegistry>, u64) {
        let registry = Arc::new(PerceptionRegistry::new());
        let buffer: Arc<PerceptionBuffer> = registry.handle(Uuid::new_v4());
        let frames = [png(40, 40), png(41, 41), png(42, 42)];
        let total: u64 = frames.iter().map(|b| b.len() as u64).sum();
        for bytes in &frames {
            buffer.seed_frame_for_test("s", MediaFrame::from_bytes(bytes.clone()));
        }
        (registry, total)
    }

    // what this catches: footprint reports the RAM the rings hold (sum of frame bytes) as
    // Ram, and is EMPTY when idle — the governor accounts perception like serving/bevy/voice.
    #[tokio::test]
    async fn footprint_reports_ring_ram_and_is_empty_when_idle() {
        let idle = PerceptionConsumer::new(Arc::new(PerceptionRegistry::new()));
        assert!(idle.footprint().is_empty(), "no frames → no footprint");

        let (registry, total) = registry_with_three_frames();
        let consumer = PerceptionConsumer::new(registry);
        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].kind, ResourceKind::Ram);
        assert_eq!(fp[0].bytes, total, "footprint == sum of ring frame bytes");
    }

    // what this catches: reclaim(Ram) evicts oldest frames toward the target but KEEPS each
    // source's head → a target it can't fully hit returns honest Partial with what it freed;
    // a non-Ram ask is refused (named, not silent).
    #[tokio::test]
    async fn reclaim_evicts_oldest_keeps_head_and_refuses_wrong_kind() {
        let (registry, total) = registry_with_three_frames();
        let consumer = PerceptionConsumer::new(registry.clone());

        // Wrong kind → refused, freed nothing.
        let refused = consumer
            .reclaim(ReclaimRequest {
                kind: ResourceKind::Vram,
                target_bytes: 1,
                deadline_ms: 10,
                reason: crate::resources::ReclaimReason::Pressure,
            })
            .await;
        assert_eq!(refused.status, ReclaimStatus::Refused);
        assert_eq!(refused.freed_bytes, 0);

        // Ask for MORE than is evictable (everything, including the head): we free the two
        // oldest but keep the head → Partial, freed < total, head's bytes remain.
        let outcome = consumer
            .reclaim(ReclaimRequest {
                kind: ResourceKind::Ram,
                target_bytes: total,
                deadline_ms: 10,
                reason: crate::resources::ReclaimReason::Pressure,
            })
            .await;
        assert_eq!(
            outcome.status,
            ReclaimStatus::Partial,
            "kept the head → partial"
        );
        assert!(
            outcome.freed_bytes > 0 && outcome.freed_bytes < total,
            "freed the old, kept the head"
        );
        assert_eq!(
            registry.total_resident_bytes(),
            total - outcome.freed_bytes,
            "residency dropped by exactly what was freed (honest)"
        );
        assert!(
            registry.total_resident_bytes() > 0,
            "the head frame survives — never blind"
        );
    }
}
