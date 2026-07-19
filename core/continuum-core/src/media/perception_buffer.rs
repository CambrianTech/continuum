//! `PerceptionBuffer` — the non-blocking, room-as-NOW hold for live-call perception.
//!
//! One LATEST frame per participant (coalesced — a newer frame REPLACES the old; never a
//! backlog, [[perceive-the-room-as-it-is-now]]). Ingest FIRES the ambient cell warms async
//! and returns immediately; reads return ONLY the cells resolved so far via the
//! `MediaFrame::*_if_ready` twins — the persona NEVER waits, a pending cell is simply absent
//! this tick and bridges in the next ([[command-async-shape-prefer-stream-never-block]]).
//!
//! Keyed by the **airc participant identity** (the room roster), NOT a parallel call-scoped
//! id: the call IS an airc room, LiveKit is only the media plane
//! ([[all-rooms-are-airc-rooms-no-mirrors]], [[livekit-media-plane-rides-airc-not-parallel]],
//! [[placement-first-four-repos-then-adapters-boy-scout]]).
//!
//! This is the buffer under the (next) `MediaPerceptionSource: RagSource`, which delivers
//! these percepts under the flexbox RAG budget so perception never dominates context
//! ([[perception-feedback-must-not-blow-rag]]).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::frame::{FrameDescriber, MediaFrame};
use super::image_ops::DestSize;
use crate::runtime::SharedCompute;

/// The airc participant identity (peer id) a frame belongs to. Never a parallel
/// call/session id — the room roster is the truth.
pub type ParticipantId = String;

/// The MINIMUM spacing between expensive cell-warms (scale + LLM describe) per
/// participant — the perception plane's sample rate, DECOUPLED from the media frame
/// rate. A live call runs 30fps for smooth human video + avatars (the display plane);
/// a persona does NOT need a fresh vision-encode/description 30×/sec. At ~2 Hz the
/// ambient "look" stays current while the LLM-describe cost stays bounded regardless
/// of frame rate ([[multimodal-live-mode-is-a-latency-obsession-cbar-doctrine]],
/// [[avatar-render-is-a-resolution-field-attention-priced]]). Birth default; the
/// governor makes this an attention-priced resolution field later (#173) — a focused
/// persona looks more often, an idle one less.
pub const AMBIENT_WARM_MIN_INTERVAL_MS: u64 = 500;

/// Per-participant warm-cadence gate. Coalescing the latest frame is unconditional
/// and O(1); only the EXPENSIVE warm (scale + describe) passes through this gate, so
/// a 30fps ingest coalesces 30 frames/sec into one latest but triggers at most
/// ~2 describes/sec, and never stacks a second warm on an unfinished one.
#[derive(Default)]
struct WarmGate {
    /// Monotonic ms of the last warm we launched for this participant.
    last_warm_ms: u64,
    /// False until the first warm — so a participant's first frame warms immediately.
    warmed_once: bool,
    /// Held `true` for the duration of an in-flight warm task; a concurrent observe
    /// that sees it set skips (no stacking a slow describe under a fast stream).
    in_flight: Arc<AtomicBool>,
}

/// What a persona perceives of ONE participant RIGHT NOW — only the cells that have
/// RESOLVED. Each field holds the cache's `Arc` (zero-copy); `None` = not-ready-this-tick
/// (present the next); `Some(Err(..))` = the cell resolved to a surfaced failure, never a
/// silent placeholder ([[fallbacks-are-illegal-fail-loud]]).
#[derive(Clone, Debug)]
pub struct Percept {
    pub participant: ParticipantId,
    pub content_hash: String,
    /// The ambient thumbnail cell (PNG bytes), if resolved.
    pub thumbnail: Option<Arc<Result<Vec<u8>, String>>>,
    /// The description cell (the sensory-bridge text), if resolved.
    pub description: Option<Arc<Result<String, String>>>,
}

impl Percept {
    /// Whether this participant has ANY resolved cell to perceive yet (else it's a known
    /// presence with nothing rendered this tick).
    pub fn has_any(&self) -> bool {
        self.thumbnail.is_some() || self.description.is_some()
    }
}

/// Non-blocking, room-as-now perception hold. One latest frame per airc participant; cells
/// warm async on the shared cache; reads take only what's ready.
pub struct PerceptionBuffer {
    latest: Mutex<HashMap<ParticipantId, MediaFrame>>,
    /// The ambient forced-look size (~480w default) — the cheap thumbnail cell every tick
    /// warms + reads. Full-res / bigger is the drill-in tool, not the ambient path.
    ambient: DestSize,
    /// Per-participant warm-cadence gates — the seam that keeps the expensive cells off
    /// the media frame rate. See [`WarmGate`] and [`AMBIENT_WARM_MIN_INTERVAL_MS`].
    warm_gates: Mutex<HashMap<ParticipantId, WarmGate>>,
    /// Minimum ms between warms per participant (the perception sample rate). Field, not
    /// hardcoded at the call site, so the governor can retune it per persona later (#173).
    min_warm_interval_ms: u64,
}

impl PerceptionBuffer {
    pub fn new(ambient: DestSize) -> Self {
        Self {
            latest: Mutex::new(HashMap::new()),
            ambient,
            warm_gates: Mutex::new(HashMap::new()),
            min_warm_interval_ms: AMBIENT_WARM_MIN_INTERVAL_MS,
        }
    }

    /// The warm-cadence DECISION (pure + testable): should this observe launch a cell
    /// warm for `participant` at `now_ms`? Returns the in-flight flag to hold for the
    /// warm's duration when YES; `None` when the warm is GATED — either one is already
    /// in flight (never stack), or we warmed within `min_warm_interval_ms` (the frame
    /// rate outruns the perception sample rate). The 30fps-safety guarantee lives here,
    /// separated from the spawn so it can be asserted without timing or a runtime.
    fn should_warm(&self, participant: &str, now_ms: u64) -> Option<Arc<AtomicBool>> {
        let mut gates = self.warm_gates.lock().unwrap_or_else(|e| e.into_inner());
        let g = gates.entry(participant.to_string()).or_default();
        // Never stack a warm on an unfinished one — protects against a describe slower
        // than the interval under a fast stream.
        if g.in_flight.load(Ordering::Acquire) {
            return None;
        }
        let due = !g.warmed_once
            || now_ms.saturating_sub(g.last_warm_ms) >= self.min_warm_interval_ms;
        if !due {
            return None;
        }
        g.warmed_once = true;
        g.last_warm_ms = now_ms;
        g.in_flight.store(true, Ordering::Release);
        Some(g.in_flight.clone())
    }

    /// Ingest a frame for `participant`: COALESCE to the latest (room-as-now — a newer frame
    /// replaces the old) UNCONDITIONALLY (an O(1) mutex insert, safe to call at 30fps — the
    /// media/display plane), then FIRE the ambient cell warms (thumbnail + description) on a
    /// spawned task ONLY when the cadence gate opens ([`should_warm`](Self::should_warm)) and
    /// return IMMEDIATELY. Non-blocking — the ingest path never awaits the cells. `now_ms` is
    /// the caller's monotonic clock (substrate passes time in; no `Instant::now` in the media
    /// core). Must be called within a tokio runtime (the live LiveKit ingest path always is).
    pub fn observe(
        &self,
        participant: ParticipantId,
        frame: MediaFrame,
        compute: Arc<SharedCompute>,
        describer: Arc<dyn FrameDescriber>,
        mime: &str,
        now_ms: u64,
    ) {
        // Coalesce ALWAYS: the latest frame per participant wins; the old one is dropped.
        // Cheap enough for the 30fps media plane — perception samples it, doesn't mirror it.
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(participant.clone(), frame.clone());

        // Gate the EXPENSIVE warm to the perception sample rate. A gated tick simply
        // returns — its frame is already the coalesced latest and will warm on the next
        // open tick (room-as-now). compute-once per content-hash still applies on top:
        // re-warming identical bytes is a cache hit.
        let Some(in_flight) = self.should_warm(&participant, now_ms) else {
            return;
        };

        let ambient = self.ambient;
        let mime = mime.to_string();
        tokio::spawn(async move {
            let _ = frame.scaled(&compute, None, ambient).await; // warm ~480w thumbnail
            let _ = frame.description(&compute, describer.as_ref(), &mime).await; // warm describe
            // Release the gate so the next due tick can warm the then-latest frame.
            in_flight.store(false, Ordering::Release);
        });
    }

    /// The perception of the room AS IT IS NOW — one `Percept` per participant, each carrying
    /// only the cells RESOLVED so far (non-blocking reads via the `_if_ready` twins). NEVER
    /// awaits; a still-warming cell is simply `None` this tick.
    pub fn current_percepts(&self, compute: &SharedCompute) -> Vec<Percept> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(pid, frame)| Percept {
                participant: pid.clone(),
                content_hash: frame.content_hash().to_string(),
                thumbnail: frame.scaled_if_ready(compute, None, self.ambient),
                description: frame.description_if_ready(compute),
            })
            .collect()
    }

    /// Drop a participant that left the call.
    pub fn remove(&self, participant: &str) {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(participant);
    }

    /// Number of participants currently held (for probes/tests).
    pub fn len(&self) -> usize {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    struct StubDescriber;
    #[async_trait]
    impl FrameDescriber for StubDescriber {
        async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
            Ok(format!("{mime} image, {} bytes", source.len()))
        }
    }

    const AMBIENT: DestSize = DestSize { width: 32, height: 24 };

    // what this catches: COALESCE — a newer frame for the same participant REPLACES the old
    // (room-as-now, no backlog). Two observes of the same participant leave ONE percept,
    // carrying the LATEST content hash.
    #[tokio::test]
    async fn observe_coalesces_to_the_latest_frame_per_participant() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);

        let old = MediaFrame::from_bytes(png(40, 40));
        let new = MediaFrame::from_bytes(png(60, 40)); // different bytes → different hash
        buffer.observe("alice".into(), old.clone(), compute.clone(), describer.clone(), "image/png", 0);
        buffer.observe("alice".into(), new.clone(), compute.clone(), describer.clone(), "image/png", 1);
        buffer.observe("bob".into(), MediaFrame::from_bytes(png(20, 20)), compute.clone(), describer.clone(), "image/png", 2);

        assert_eq!(buffer.len(), 2, "alice coalesced, bob separate → 2 participants");
        let percepts = buffer.current_percepts(&compute);
        let alice = percepts.iter().find(|p| p.participant == "alice").unwrap();
        assert_eq!(alice.content_hash, new.content_hash(), "alice holds the LATEST frame");
    }

    // what this catches: NON-BLOCKING read semantics — before a cell is warmed the percept
    // carries None (perception takes what's ready, doesn't wait); once the same content-hash
    // cell is resolved on the shared compute, current_percepts surfaces it as Some. Proves the
    // buffer reads via the _if_ready twins, not a blocking recompute.
    #[tokio::test]
    async fn current_percepts_surface_cells_only_once_resolved() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);
        let frame = MediaFrame::from_bytes(png(50, 40));

        // Store WITHOUT warming: read the percept before any cell resolves → all None.
        buffer.latest.lock().unwrap().insert("alice".into(), frame.clone());
        let before = &buffer.current_percepts(&compute)[0];
        assert!(before.thumbnail.is_none() && before.description.is_none(), "cold → nothing rendered");
        assert!(!before.has_any());

        // Resolve the cells on the SHARED compute (deterministic; in prod the observe spawn
        // does this async). Now the non-blocking read surfaces them.
        frame.scaled(&compute, None, AMBIENT).await;
        frame.description(&compute, &StubDescriber, "image/png").await;
        let after = &buffer.current_percepts(&compute)[0];
        assert!(after.thumbnail.is_some(), "thumbnail now ready");
        assert!(after.description.is_some(), "description now ready");
        assert_eq!(
            image::load_from_memory(after.thumbnail.as_ref().unwrap().as_ref().as_ref().unwrap())
                .unwrap()
                .width(),
            AMBIENT.width,
            "the ready thumbnail is the ambient size"
        );
    }

    // what this catches: remove drops a participant who left the call.
    #[tokio::test]
    async fn remove_drops_a_participant() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);
        buffer.observe("alice".into(), MediaFrame::from_bytes(png(10, 10)), compute, describer, "image/png", 0);
        assert_eq!(buffer.len(), 1);
        buffer.remove("alice");
        assert!(buffer.is_empty());
    }

    // what this catches: the 30fps SAFETY guarantee, as a pure decision (no timing/spawn).
    // The first frame warms; further frames WITHIN the interval are gated even though they
    // are distinct content (the frame rate outruns the perception sample rate); once the
    // interval elapses the next warm is due again. This is what keeps a 30fps stream from
    // triggering 30 LLM describes/sec.
    #[test]
    fn warm_cadence_gates_within_the_interval_and_reopens_after() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "alice";

        // First ever frame → warms immediately; holds the in-flight flag.
        let flag = buffer.should_warm(p, 0).expect("first frame warms");
        // Simulate the warm completing so the in-flight guard is not what's under test.
        flag.store(false, Ordering::Release);

        // A distinct frame 10ms later (30fps ≈ 33ms apart) → GATED by cadence.
        assert!(
            buffer.should_warm(p, 10).is_none(),
            "a frame within the interval is gated — perception samples, not mirrors, 30fps"
        );
        assert!(
            buffer.should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS - 1).is_none(),
            "still within the interval → still gated"
        );

        // Once the interval elapses, the next warm is due again.
        let flag2 = buffer
            .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS)
            .expect("interval elapsed → warm is due");
        flag2.store(false, Ordering::Release);
    }

    // what this catches: never STACK a warm — while one is in flight, later observes are
    // gated even if the interval has elapsed, so a describe slower than the interval under
    // a fast stream cannot pile up concurrent inferences.
    #[test]
    fn warm_gate_never_stacks_an_in_flight_warm() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "bob";

        // Launch a warm and DON'T release it (simulates a describe still running).
        let _held = buffer.should_warm(p, 0).expect("first warms");

        // Even far past the interval, a second warm is refused while one is in flight.
        assert!(
            buffer.should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS * 100).is_none(),
            "in-flight guard blocks stacking regardless of elapsed time"
        );

        // Once it completes, the next due tick warms again.
        _held.store(false, Ordering::Release);
        assert!(
            buffer.should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS * 100).is_some(),
            "warm reopens after the in-flight one finishes"
        );
    }
}
