//! `perception_ingest` — fan ONE decoded call frame out into every viewer's perception.
//!
//! The WRITE side of live-call perception (#192): a video frame arrives from the
//! LiveKit bridge for one speaker, and each AI persona in the call must SEE it —
//! i.e. it must land in each persona's [`PerceptionBuffer`](super::PerceptionBuffer),
//! keyed by the speaker's airc identity. This module is the pure fan-out that does
//! that, decoupled from the transport so it is testable without a live call.
//!
//! # Compute-once/share-many holds across the fan-out
//!
//! The frame's expensive derivatives (scale, describe, luma signature) are keyed by
//! CONTENT HASH on the ONE [`shared_compute::global`] cache, so even though
//! [`observe`](super::PerceptionBuffer::observe) is called once per viewer, the
//! describe/scale run ONCE for that frame and every viewer shares the result — the
//! multi-persona vision moat ([[vision-replication-is-the-multipersona-moat-vs-cloud]],
//! [[media-is-compute-once-zero-copy-hardware-grade]]). The per-viewer cost is a
//! bounded ring push (an `Arc` handle) plus a gated warm that dedups on the cache.
//!
//! # A persona never watches its own outbound avatar
//!
//! The speaker is identified by its LiveKit identity string, which for a persona IS
//! `persona_id.to_string()` (the avatar publish path sets `identity = persona_id`).
//! So the fan-out SKIPS the viewer whose id equals the speaker — a persona doesn't
//! perceive the frames it is itself publishing. Every other viewer (and every human
//! speaker, whose identity matches no persona id) is seen by all.
//!
//! # Not a manager
//!
//! This holds no call state and tracks no rosters — the live-call lifecycle already
//! owns "who is in the call" ([[all-rooms-are-airc-rooms-no-mirrors]]). The caller
//! passes the viewer roster in; this is a stateless projection, not a parallel
//! coordinator (CONCURRENCY-STYLE-GUIDE forbidden move #6). It must be invoked from
//! within a tokio runtime — `observe` spawns the warm — which the live drain task is.

use std::sync::{Arc, OnceLock};

use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

use super::frame::{FrameDescriber, MediaFrame};

/// The stateless fan-out. Holds ONLY the shared describer (one per call — the
/// sensory-bridge inference that N viewers share), never any call/roster state.
#[derive(Clone)]
pub struct FrameIngest {
    /// The one describer the whole call shares — `observe` hands it to each viewer's
    /// warm, but compute-once on the content hash means it runs at most once per frame.
    describer: Arc<dyn FrameDescriber>,
}

impl FrameIngest {
    pub fn new(describer: Arc<dyn FrameDescriber>) -> Self {
        Self { describer }
    }

    /// Ingest one decoded frame from `speaker` into every `viewer` persona's
    /// perception buffer — EXCEPT the speaker's own (a persona doesn't watch its own
    /// avatar). `jpeg` is the already-encoded frame bytes (the bridge encodes I420 →
    /// JPEG upstream), `mime` its encoding (`image/jpeg`), `now_ms` the caller's
    /// monotonic clock (the substrate passes time in). Non-blocking: each `observe`
    /// coalesces the frame and fires an async warm, returning immediately.
    ///
    /// Builds the [`MediaFrame`] ONCE and clones the cheap `Arc`-backed handle per
    /// viewer, so the content hash — and therefore every shared derivative — is
    /// identical across viewers (the compute-once seam).
    pub fn fan_out(&self, speaker: &str, viewers: &[Uuid], jpeg: Vec<u8>, mime: &str, now_ms: u64) {
        if viewers.is_empty() {
            return;
        }
        let frame = MediaFrame::from_bytes(jpeg);
        let compute = crate::runtime::shared_compute::global();
        for &viewer in viewers {
            // A persona never observes its OWN outbound avatar frame. The persona's
            // LiveKit identity is its uuid string, so equality on that is the self-skip.
            if viewer.to_string() == speaker {
                continue;
            }
            let buffer = crate::media::perception_registry().handle(viewer);
            buffer.observe(
                speaker.to_string(),
                frame.clone(),
                compute.clone(),
                self.describer.clone(),
                mime,
                now_ms,
            );
        }
    }
}

/// One decoded call frame awaiting fan-out — the unit of work crossing the bridge
/// reader THREAD → tokio DRAIN boundary. The bridge (a blocking `std::thread` with no
/// reactor) can't call [`FrameIngest::fan_out`] directly (`observe` spawns), so it
/// hands the already-encoded JPEG across this channel to the drain, which runs on the
/// runtime. `call_id` is the airc session/room id string; `speaker_id` the airc
/// identity of whoever the frame is of.
pub struct IngestFrame {
    pub call_id: String,
    pub speaker_id: String,
    /// The encoded frame bytes (the bridge encodes I420 → JPEG upstream at ~1 fps).
    pub jpeg: Vec<u8>,
    /// The encoding of `jpeg` (`image/jpeg`).
    pub mime: String,
}

/// The process-global send endpoint of the ingest channel — installed ONCE by the
/// live module when it spawns the drain. Same `OnceLock` shape as
/// [`perception_registry`](super::perception_registry) / `shared_compute::global`:
/// it is a channel handle, not a manager (CONCURRENCY-STYLE-GUIDE: the mpsc "here is
/// work" primitive). `None` until the drain is up — a frame before then is simply not
/// perceived, and the human display plane (the LiveKit track) is unaffected regardless.
static INGEST_SINK: OnceLock<UnboundedSender<IngestFrame>> = OnceLock::new();

/// Create the ingest channel and install its sender globally, returning the receiver
/// for the drain to own. The live module calls this once at init and spawns a task on
/// the returned receiver. A second call returns `None` (only one drain owns the stream).
pub fn install_channel() -> Option<UnboundedReceiver<IngestFrame>> {
    let (tx, rx) = unbounded_channel();
    if INGEST_SINK.set(tx).is_ok() {
        Some(rx)
    } else {
        None
    }
}

/// Post a decoded frame to the drain — NON-BLOCKING, callable from the bridge reader
/// thread (which has no tokio runtime): an unbounded `send` neither blocks nor awaits.
/// Drops silently if no drain is installed yet (boot-only) or the drain has shut down.
/// This is the ONLY thing the hot bridge thread does with the frame beyond its own
/// logging — it never scales, describes, or touches the display track.
pub fn try_enqueue(frame: IngestFrame) {
    if let Some(tx) = INGEST_SINK.get() {
        let _ = tx.send(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::perception_registry;
    use async_trait::async_trait;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgba([200, 40, 40, 255])
            } else {
                Rgba([40, 40, 200, 255])
            }
        });
        let mut out = Cursor::new(Vec::new());
        // JPEG has no alpha — go through rgb8, exactly like the bridge encoder.
        DynamicImage::ImageRgba8(img)
            .to_rgb8()
            .write_to(&mut out, ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    struct StubDescriber;
    #[async_trait]
    impl FrameDescriber for StubDescriber {
        async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
            Ok(format!("{mime} frame, {} bytes", source.len()))
        }
    }

    fn ingest() -> FrameIngest {
        FrameIngest::new(Arc::new(StubDescriber))
    }

    // what this catches: THE fan-out — a human speaker's frame lands in EVERY viewer
    // persona's buffer, each keyed by the speaker's identity, with the SAME content
    // hash (compute-once seam: one MediaFrame cloned across viewers). The ring push is
    // synchronous, so residency is observable immediately after fan_out returns.
    #[tokio::test]
    async fn fan_out_lands_a_speaker_frame_in_every_viewer_buffer() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let human = "human-speaker-not-a-persona";

        ingest().fan_out(human, &[a, b], jpeg(80, 60), "image/jpeg", 0);

        let compute = crate::runtime::shared_compute::global();
        let mut hashes = Vec::new();
        for viewer in [a, b] {
            let buf = perception_registry().get(&viewer).expect("viewer got a buffer");
            let percepts = buf.current_percepts(&compute);
            assert_eq!(percepts.len(), 1, "each viewer sees the one speaker");
            assert_eq!(percepts[0].participant, human, "keyed by the speaker identity");
            hashes.push(percepts[0].content_hash.clone());
            perception_registry().remove(&viewer);
        }
        assert_eq!(hashes[0], hashes[1], "same frame content hash across viewers (compute-once)");
    }

    // what this catches: a persona NEVER observes its own outbound avatar frame — when
    // the speaker id equals a viewer's id, that viewer is skipped while the others still
    // see it. Prevents a persona perceiving the video it is itself publishing.
    #[tokio::test]
    async fn a_persona_does_not_observe_its_own_frame() {
        let speaker = Uuid::new_v4();
        let other = Uuid::new_v4();

        // The speaker is one of the viewers (it's a persona in the call).
        ingest().fan_out(&speaker.to_string(), &[speaker, other], jpeg(50, 40), "image/jpeg", 0);

        assert!(
            perception_registry().get(&speaker).is_none(),
            "the speaker persona did not observe (and never resolved) its own buffer"
        );
        let compute = crate::runtime::shared_compute::global();
        let other_buf = perception_registry().get(&other).expect("the other viewer saw it");
        assert_eq!(other_buf.current_percepts(&compute).len(), 1, "the other viewer sees the speaker");
        assert_eq!(
            other_buf.current_percepts(&compute)[0].participant,
            speaker.to_string(),
            "keyed by the speaker persona id"
        );
        perception_registry().remove(&other);
    }

    // what this catches: an empty viewer roster is a no-op (no buffers created, no
    // panic) — a frame arriving before any persona has joined simply goes nowhere.
    #[tokio::test]
    async fn fan_out_to_no_viewers_is_a_noop() {
        ingest().fan_out("someone", &[], jpeg(20, 20), "image/jpeg", 0);
        // Nothing to assert beyond "did not panic / created nothing" — a fresh id has no buffer.
        assert!(perception_registry().get(&Uuid::new_v4()).is_none());
    }
}
