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

use std::collections::{HashMap, VecDeque};
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

/// The MAXIMUM staleness — the slow BASELINE floor a participant is re-looked-at even
/// with a dead-static scene and no change signal. "Slow cadence, not NO cadence": an
/// idle observer still refreshes a still participant every few seconds so room-as-now
/// stays true, just coarse. The warm cadence is a BAND — a relevant change (motion /
/// scene-change / YOLO, #192) escalates toward the MIN ceiling; its absence decays to
/// this floor, never to infinity. Attention priority (speaker/actor) shrinks both
/// bounds later (#173).
pub const AMBIENT_WARM_BASELINE_INTERVAL_MS: u64 = 5_000;

/// Mean-luma-delta threshold (0..=255) above which two consecutive frames of a source
/// count as a CHANGE. ~8/255 ≈ 3% average pixel shift — past camera noise, catches real
/// motion / scene / slide changes. The trigger that escalates the warm band; below it the
/// scene is "static" and decays to the baseline floor.
pub const CHANGE_THRESHOLD: u8 = 8;

/// Depth of each source's sliding-window frame ring — how many recent frames are held
/// per participant. The HEAD is the most-current frame ("latest"); the window (front K)
/// is what the sliding-window read + change comparison see. Bounded so a 30fps source
/// costs a fixed handful of `Arc` frame handles, never an unbounded backlog. A field on
/// the buffer (defaulted from here) so the governor can deepen it for a source under
/// scrutiny (#173).
pub const AMBIENT_RING_CAPACITY: usize = 8;

/// A bounded, per-source sliding window of recent frames — the shared "pre-warmed image
/// cache" for ONE participant. Newest at the FRONT (the head = "latest"); the oldest is
/// evicted past capacity. Frames are `Arc`-backed handles and their derivatives dedup by
/// content-hash on the shared cache, so a ring of K frames is cheap and never duplicates
/// a derivative across viewers. The window is what change-detection (compare consecutive
/// heads) and "what did I miss" read; the head is the room-as-now view.
struct FrameRing {
    /// front = newest, back = oldest.
    frames: VecDeque<MediaFrame>,
    capacity: usize,
}

impl FrameRing {
    fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            frames: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push the newest frame to the front. COALESCE: a re-send of the SAME content as the
    /// current head (identical hash) is a no-op — the head already IS this frame. Evict the
    /// oldest past capacity (room-as-now, bounded).
    fn push(&mut self, frame: MediaFrame) {
        if self.frames.front().map(|f| f.content_hash()) == Some(frame.content_hash()) {
            return;
        }
        self.frames.push_front(frame);
        while self.frames.len() > self.capacity {
            self.frames.pop_back();
        }
    }

    /// The most-current frame (room-as-now "latest").
    fn head(&self) -> Option<&MediaFrame> {
        self.frames.front()
    }

    /// The last `k` frames, newest first — the sliding-window read.
    fn window(&self, k: usize) -> impl Iterator<Item = &MediaFrame> {
        self.frames.iter().take(k)
    }

    /// RAM held by this ring: the sum of frame source bytes (Arc-shared, but this ring is
    /// the owner that can drop them). The honest footprint perception can reclaim.
    fn resident_bytes(&self) -> u64 {
        self.frames.iter().map(|f| f.source().len() as u64).sum()
    }

    /// Evict oldest frames (from the back) to free ~`want_bytes`, but ALWAYS keep the head
    /// (the room-as-now frame — never blind the persona). Returns bytes actually freed.
    fn evict_oldest(&mut self, want_bytes: u64) -> u64 {
        let mut freed = 0u64;
        while self.frames.len() > 1 && freed < want_bytes {
            if let Some(f) = self.frames.pop_back() {
                freed += f.source().len() as u64;
            }
        }
        freed
    }
}

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

/// An à la carte look request — WHICH sources a persona wants to see right now. The PULL
/// side of perception (the persona's explicit ask), distinct from the ambient PUSH (the
/// warm band). "Into their vision à la carte" — the persona composes its own view.
#[derive(Clone, Debug)]
pub enum LookScope {
    /// One participant's most-current frame.
    Source(ParticipantId),
    /// Every participant's most-current frame — the contact-sheet / gallery.
    Everyone,
}

/// The image fidelity of a look. Higher-res is just a different SIZE on the same content
/// hash, so it's compute-once/shared exactly like the thumbnail (a higher-res look another
/// persona already asked for is a cache hit here). `Full` is the raw frame bytes.
#[derive(Clone, Copy, Debug)]
pub enum LookFidelity {
    /// The ambient thumbnail (~480w) — the cheap default.
    Thumbnail,
    /// A specific resolution (higher OR lower than ambient).
    Res(DestSize),
    /// The raw source frame, unscaled.
    Full,
}

/// One satisfied look — the image bytes for a source at the requested fidelity, shared
/// (`Arc`, zero-copy for the scaled path). `Err` surfaces a real failure, never a
/// fabricated image ([[fallbacks-are-illegal-fail-loud]]).
#[derive(Clone)]
pub struct LookImage {
    pub participant: ParticipantId,
    pub content_hash: String,
    pub fidelity: LookFidelity,
    pub image: Arc<Result<Vec<u8>, String>>,
}

/// Non-blocking, room-as-now perception hold. One latest frame per airc participant; cells
/// warm async on the shared cache; reads take only what's ready.
pub struct PerceptionBuffer {
    /// One bounded sliding-window ring per participant — the shared pre-warmed store.
    /// The head is the room-as-now frame; the window is the recent history.
    rings: Mutex<HashMap<ParticipantId, FrameRing>>,
    /// The ambient forced-look size (~480w default) — the cheap thumbnail cell every tick
    /// warms + reads. Full-res / bigger is the drill-in tool, not the ambient path.
    ambient: DestSize,
    /// Depth of each participant's ring (defaulted from [`AMBIENT_RING_CAPACITY`]) — a field
    /// so the governor can deepen a source's window under scrutiny (#173).
    ring_capacity: usize,
    /// Per-participant warm-cadence gates — the seam that keeps the expensive cells off
    /// the media frame rate. See [`WarmGate`] and [`AMBIENT_WARM_MIN_INTERVAL_MS`].
    warm_gates: Mutex<HashMap<ParticipantId, WarmGate>>,
    /// The warm cadence BAND, per participant — fields (not call-site constants) so the
    /// governor retunes them per persona as an attention-priced resolution field (#173):
    /// `min` = the ceiling (fastest, under change), `baseline` = the floor / max-staleness
    /// (slowest, static scene). A relevant change escalates toward min; its absence decays
    /// to baseline, never to infinity.
    min_warm_interval_ms: u64,
    baseline_warm_interval_ms: u64,
}

impl PerceptionBuffer {
    pub fn new(ambient: DestSize) -> Self {
        Self {
            rings: Mutex::new(HashMap::new()),
            ambient,
            ring_capacity: AMBIENT_RING_CAPACITY,
            warm_gates: Mutex::new(HashMap::new()),
            min_warm_interval_ms: AMBIENT_WARM_MIN_INTERVAL_MS,
            baseline_warm_interval_ms: AMBIENT_WARM_BASELINE_INTERVAL_MS,
        }
    }

    /// Is this source's scene RECENTLY CHANGING? — the real change signal that drives the
    /// warm band (replacing the old `changed = true` placeholder). Compares the two most
    /// recent frames whose luma SIGNATURE has already warmed (the change monitor reads only
    /// ready cells, never awaits): mean-abs delta over [`CHANGE_THRESHOLD`] = changed. Fewer
    /// than two ready signatures (cold start, or signatures still warming) → `true`: assume
    /// changed and ride the MIN ceiling until we can actually tell — safe, never starves.
    /// Cheap: a bounded ring scan + a ~192-byte diff, fine to call per ingest.
    fn scene_recently_changed(&self, source: &str, compute: &SharedCompute) -> bool {
        let rings = self.rings.lock().unwrap_or_else(|e| e.into_inner());
        let Some(ring) = rings.get(source) else {
            return true;
        };
        // Newest-first: collect the two most recent READY, OK signatures.
        let mut ready = ring.window(usize::MAX).filter_map(|f| {
            f.signature_if_ready(compute).and_then(|sig| match &*sig {
                Ok(bytes) => Some(bytes.clone()),
                Err(_) => None,
            })
        });
        match (ready.next(), ready.next()) {
            (Some(newest), Some(prev)) => {
                super::image_ops::luma_mean_abs_delta(&newest, &prev) > CHANGE_THRESHOLD
            }
            // Can't tell yet → assume changed (ride the ceiling until a signature warms).
            _ => true,
        }
    }

    /// The warm-cadence DECISION (pure + testable): should this observe launch a cell
    /// warm for `participant` at `now_ms`? Returns the in-flight flag to hold for the
    /// warm's duration when YES; `None` when GATED. The cadence is a BAND, not a switch
    /// ("slow cadence, not NO cadence"):
    /// - Never while one is already in flight (never stack a slow describe under a fast
    ///   stream).
    /// - `changed` (a relevant motion/scene-change/YOLO signal, #192) ESCALATES: warm as
    ///   soon as the MIN ceiling allows.
    /// - Absent change, still warm on the slow BASELINE floor — an idle observer refreshes
    ///   a static participant every few seconds, never zero.
    ///
    /// The 30fps-safety guarantee lives here, separated from the spawn so both branches
    /// (changed→fast, static→slow-floor) are asserted without timing or a runtime.
    fn should_warm(
        &self,
        participant: &str,
        now_ms: u64,
        changed: bool,
    ) -> Option<Arc<AtomicBool>> {
        let mut gates = self.warm_gates.lock().unwrap_or_else(|e| e.into_inner());
        let g = gates.entry(participant.to_string()).or_default();
        if g.in_flight.load(Ordering::Acquire) {
            return None;
        }
        let elapsed = now_ms.saturating_sub(g.last_warm_ms);
        // First look always; a relevant change escalates to the MIN ceiling; otherwise
        // decay to the slow BASELINE floor (never off).
        let due = !g.warmed_once
            || (changed && elapsed >= self.min_warm_interval_ms)
            || elapsed >= self.baseline_warm_interval_ms;
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
        // Push onto the participant's ring ALWAYS — cheap (an Arc handle + a bounded pop),
        // safe at the 30fps media plane. The head becomes the room-as-now frame; the ring
        // retains the recent window (coalescing an identical re-send). Perception samples
        // this store; it doesn't mirror the frame rate.
        self.rings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(participant.clone())
            .or_insert_with(|| FrameRing::with_capacity(self.ring_capacity))
            .push(frame.clone());

        // Gate the EXPENSIVE warm. A gated tick simply returns — its frame is already the
        // coalesced latest and will warm on the next open tick (room-as-now). compute-once
        // per content-hash still applies on top: re-warming identical bytes is a cache hit.
        //
        // The change monitor (cheap luma-signature diff of the two most recent warmed
        // frames) drives the band: a static scene decays to the BASELINE floor, a real
        // change escalates to the MIN ceiling. Cold start / signatures still warming →
        // `true` (ride the ceiling until we can tell).
        let changed = self.scene_recently_changed(&participant, &compute);
        let Some(in_flight) = self.should_warm(&participant, now_ms, changed) else {
            return;
        };

        let ambient = self.ambient;
        let mime = mime.to_string();
        tokio::spawn(async move {
            // Warm the cheap signature FIRST — it's the change monitor's fingerprint that
            // gates future describes; then the thumbnail; then the expensive describe.
            let _ = frame.signature(&compute).await;
            let _ = frame.scaled(&compute, None, ambient).await; // warm ~480w thumbnail
            let _ = frame.description(&compute, describer.as_ref(), &mime).await; // warm describe
                                                                                  // Release the gate so the next due tick can warm the then-latest frame.
            in_flight.store(false, Ordering::Release);
        });
    }

    /// Non-blocking projection of one frame into a `Percept` — reads ONLY the cells resolved
    /// so far on the shared cache (the `_if_ready` twins), never awaits, never recomputes.
    /// The single place a frame becomes a percept, shared by the room-as-now read and the
    /// windowed read (compression: one projection).
    fn percept_of(
        &self,
        participant: &str,
        frame: &MediaFrame,
        compute: &SharedCompute,
    ) -> Percept {
        Percept {
            participant: participant.to_string(),
            content_hash: frame.content_hash().to_string(),
            thumbnail: frame.scaled_if_ready(compute, None, self.ambient),
            description: frame.description_if_ready(compute),
        }
    }

    /// The perception of the room AS IT IS NOW — one `Percept` per participant from each
    /// ring's HEAD, carrying only cells RESOLVED so far. NEVER awaits; a still-warming cell
    /// is simply `None` this tick. This is the GROUPED "everyone at a glance" read (the
    /// human-widget gallery): every source's most-current warm look, shared, zero-copy.
    pub fn current_percepts(&self, compute: &SharedCompute) -> Vec<Percept> {
        self.rings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|(pid, ring)| ring.head().map(|f| self.percept_of(pid, f, compute)))
            .collect()
    }

    /// The SLIDING-WINDOW read for ONE participant — the last `k` frames as percepts, newest
    /// first (only resolved cells). This is "what changed / what did I miss" over a source's
    /// recent history, off the same shared warm store. Empty if the participant is unknown.
    pub fn window_percepts(
        &self,
        participant: &str,
        k: usize,
        compute: &SharedCompute,
    ) -> Vec<Percept> {
        self.rings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(participant)
            .map(|ring| {
                ring.window(k)
                    .map(|f| self.percept_of(participant, f, compute))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// à la carte PULL: satisfy a persona's EXPLICIT request for image(s) — of one source or
    /// the whole group, at a chosen fidelity — ASAP. A warm cell returns instant; a cold one
    /// is warmed NOW (compute-once/shared on the content hash, so a higher-res look another
    /// persona already pulled is a cache hit). This is the ONE place perception AWAITS: a
    /// deliberate ask, not the ambient hot path — and it implicitly JUMPS the warm cadence
    /// band (it never consults the ambient gate, it computes-or-hits directly). Unknown source
    /// → empty (never a fabricated look).
    pub async fn look(
        &self,
        scope: LookScope,
        fidelity: LookFidelity,
        compute: &SharedCompute,
    ) -> Vec<LookImage> {
        // Snapshot the target heads UNDER the lock (cheap Arc-backed clones), then await
        // compute OUTSIDE it — never hold a Mutex across an await.
        let targets: Vec<(ParticipantId, MediaFrame)> = {
            let rings = self.rings.lock().unwrap_or_else(|e| e.into_inner());
            match scope {
                LookScope::Source(id) => rings
                    .get(&id)
                    .and_then(|r| r.head().cloned())
                    .map(|f| (id, f))
                    .into_iter()
                    .collect(),
                LookScope::Everyone => rings
                    .iter()
                    .filter_map(|(id, r)| r.head().cloned().map(|f| (id.clone(), f)))
                    .collect(),
            }
        };

        let mut out = Vec::with_capacity(targets.len());
        for (participant, frame) in targets {
            let image = match fidelity {
                LookFidelity::Thumbnail => frame.scaled(compute, None, self.ambient).await,
                LookFidelity::Res(dest) => frame.scaled(compute, None, dest).await,
                // Deliberate full-res pull: hand back the raw bytes (a copy — the rare,
                // explicit expensive path; the scaled paths stay zero-copy/shared).
                LookFidelity::Full => Arc::new(Ok(frame.source().to_vec())),
            };
            out.push(LookImage {
                participant,
                content_hash: frame.content_hash().to_string(),
                fidelity,
                image,
            });
        }
        out
    }

    /// Total RAM held by this buffer's rings (frame source bytes) — the honest footprint
    /// perception owns and can reclaim. Derivatives (thumbnails/descriptions/signatures)
    /// live in the SHARED compute cache, not here, so they are not double-counted.
    pub fn resident_bytes(&self) -> u64 {
        self.rings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|r| r.resident_bytes())
            .sum()
    }

    /// Evict oldest ring frames across sources to free ~`want_bytes` (keeping each source's
    /// head — room-as-now). Returns bytes ACTUALLY freed (honest, never over-reports).
    pub fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let mut rings = self.rings.lock().unwrap_or_else(|e| e.into_inner());
        let mut freed = 0u64;
        for ring in rings.values_mut() {
            if freed >= want_bytes {
                break;
            }
            freed += ring.evict_oldest(want_bytes - freed);
        }
        freed
    }

    /// Test-only: push a frame directly onto a source's ring without spawning warms — lets
    /// sibling-module tests (e.g. `modules::perception_consumer`) seed known residency.
    #[cfg(test)]
    pub(crate) fn seed_frame_for_test(&self, source: &str, frame: MediaFrame) {
        self.rings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(source.to_string())
            .or_insert_with(|| FrameRing::with_capacity(self.ring_capacity))
            .push(frame);
    }

    /// Drop a participant that left the call.
    pub fn remove(&self, participant: &str) {
        self.rings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(participant);
    }

    /// Number of participants currently held (for probes/tests).
    pub fn len(&self) -> usize {
        self.rings.lock().unwrap_or_else(|e| e.into_inner()).len()
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

    const AMBIENT: DestSize = DestSize {
        width: 32,
        height: 24,
    };

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
        buffer.observe(
            "alice".into(),
            old.clone(),
            compute.clone(),
            describer.clone(),
            "image/png",
            0,
        );
        buffer.observe(
            "alice".into(),
            new.clone(),
            compute.clone(),
            describer.clone(),
            "image/png",
            1,
        );
        buffer.observe(
            "bob".into(),
            MediaFrame::from_bytes(png(20, 20)),
            compute.clone(),
            describer.clone(),
            "image/png",
            2,
        );

        assert_eq!(
            buffer.len(),
            2,
            "alice coalesced, bob separate → 2 participants"
        );
        let percepts = buffer.current_percepts(&compute);
        let alice = percepts.iter().find(|p| p.participant == "alice").unwrap();
        assert_eq!(
            alice.content_hash,
            new.content_hash(),
            "alice holds the LATEST frame"
        );
    }

    // what this catches: NON-BLOCKING read semantics — before a cell is warmed the percept
    // carries None (perception takes what's ready, doesn't wait); once the same content-hash
    // cell is resolved on the shared compute, current_percepts surfaces it as Some. Proves the
    // buffer reads via the _if_ready twins, not a blocking recompute.
    #[tokio::test]
    async fn current_percepts_surface_cells_only_once_resolved() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let frame = MediaFrame::from_bytes(png(50, 40));

        // Push onto the ring WITHOUT warming (bypass observe's spawn): read the percept from
        // the head before any cell resolves → all None.
        buffer
            .rings
            .lock()
            .unwrap()
            .entry("alice".into())
            .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY))
            .push(frame.clone());
        let before = &buffer.current_percepts(&compute)[0];
        assert!(
            before.thumbnail.is_none() && before.description.is_none(),
            "cold → nothing rendered"
        );
        assert!(!before.has_any());

        // Resolve the cells on the SHARED compute (deterministic; in prod the observe spawn
        // does this async). Now the non-blocking read surfaces them.
        frame.scaled(&compute, None, AMBIENT).await;
        frame
            .description(&compute, &StubDescriber, "image/png")
            .await;
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

    // what this catches: the ring is a bounded sliding window — newest-first, an identical
    // re-send of the head coalesces (no dup), and past capacity the oldest is evicted. This is
    // the "pre-warmed sliding-window cache" the grouped + windowed reads sit on.
    #[test]
    fn frame_ring_windows_newest_first_coalesces_head_and_bounds_capacity() {
        let mut ring = FrameRing::with_capacity(3);
        assert!(ring.head().is_none(), "empty ring has no head");

        let a = MediaFrame::from_bytes(png(11, 11));
        let b = MediaFrame::from_bytes(png(12, 12));
        let c = MediaFrame::from_bytes(png(13, 13));
        let d = MediaFrame::from_bytes(png(14, 14));

        ring.push(a.clone());
        ring.push(a.clone()); // identical head → coalesced no-op
        assert_eq!(
            ring.window(9).count(),
            1,
            "re-send of the same head coalesces"
        );

        ring.push(b.clone());
        ring.push(c.clone());
        ring.push(d.clone()); // capacity 3 → 'a' evicted

        let hashes: Vec<_> = ring
            .window(9)
            .map(|f| f.content_hash().to_string())
            .collect();
        assert_eq!(
            hashes,
            vec![
                d.content_hash().to_string(),
                c.content_hash().to_string(),
                b.content_hash().to_string()
            ],
            "newest-first, capacity-bounded, oldest ('a') evicted"
        );
        assert_eq!(
            ring.head().unwrap().content_hash(),
            d.content_hash(),
            "head = most current"
        );
        assert_eq!(ring.window(2).count(), 2, "window respects k");
    }

    // what this catches: the two serving shapes off the ONE shared store — GROUPED (all ring
    // heads = the human-widget gallery, everyone at a glance) and WINDOWED (one source's recent
    // history, newest-first). An unknown participant windows to empty.
    #[tokio::test]
    async fn grouped_read_is_all_heads_windowed_read_is_one_sources_history() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());

        // alice gets a short history (3 distinct frames), bob one — pushed WITHOUT warming so
        // the read exercises structure (heads/window), not cell resolution.
        {
            let mut rings = buffer.rings.lock().unwrap();
            let alice = rings
                .entry("alice".into())
                .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY));
            for (w, h) in [(11, 11), (12, 12), (13, 13)] {
                alice.push(MediaFrame::from_bytes(png(w, h)));
            }
            rings
                .entry("bob".into())
                .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY))
                .push(MediaFrame::from_bytes(png(20, 20)));
        }

        // GROUPED: one head per source — the gallery of everyone at a glance.
        let grouped = buffer.current_percepts(&compute);
        assert_eq!(grouped.len(), 2, "gallery = one head per participant");

        // WINDOWED: the last k of ONE source, newest-first.
        let win = buffer.window_percepts("alice", 2, &compute);
        assert_eq!(win.len(), 2, "windowed read = last k of one source");
        assert!(
            win.iter().all(|p| p.participant == "alice"),
            "windowed read is source-scoped"
        );

        // Unknown participant → empty (never a fabricated look).
        assert!(buffer.window_percepts("nobody", 5, &compute).is_empty());
    }

    // what this catches: the à la carte PULL — a persona requests images of a SOURCE or the
    // GROUP, at THUMBNAIL / higher-RES / FULL, satisfied ASAP. Higher-res is a distinct cache
    // key on the same content-hash; a repeat pull is compute-once/shared (same Arc); FULL is
    // the raw bytes; an unknown source pulls empty.
    #[tokio::test]
    async fn look_serves_source_or_group_at_requested_fidelity_asap_and_shared() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);

        // Two sources present (via the ingest path — the head is what a look reads).
        buffer.observe(
            "alice".into(),
            MediaFrame::from_bytes(png(120, 90)),
            compute.clone(),
            describer.clone(),
            "image/png",
            0,
        );
        buffer.observe(
            "bob".into(),
            MediaFrame::from_bytes(png(64, 64)),
            compute.clone(),
            describer.clone(),
            "image/png",
            0,
        );

        // SOURCE + THUMBNAIL: one image, at the ambient size, satisfied ASAP (awaited).
        let a = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Thumbnail,
                &compute,
            )
            .await;
        assert_eq!(a.len(), 1, "source scope → just that source");
        assert_eq!(a[0].participant, "alice");
        let bytes = a[0].image.as_ref().as_ref().expect("thumbnail resolved");
        assert_eq!(
            image::load_from_memory(bytes).unwrap().width(),
            AMBIENT.width,
            "ambient thumbnail size"
        );

        // PREFER-WARM / compute-once: a second identical pull returns the SAME shared Arc.
        let a2 = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Thumbnail,
                &compute,
            )
            .await;
        assert!(
            Arc::ptr_eq(&a[0].image, &a2[0].image),
            "repeat pull is compute-once/shared, not recomputed"
        );

        // Higher-RES: a distinct size → a distinct derivative (bigger than the thumbnail).
        let hi = DestSize {
            width: 96,
            height: 72,
        };
        let r = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Res(hi),
                &compute,
            )
            .await;
        assert_eq!(
            image::load_from_memory(r[0].image.as_ref().as_ref().unwrap())
                .unwrap()
                .width(),
            hi.width,
            "higher-res honored"
        );
        assert!(
            !Arc::ptr_eq(&a[0].image, &r[0].image),
            "different fidelity → different cell"
        );

        // FULL: the raw source bytes (original 120×90).
        let f = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Full,
                &compute,
            )
            .await;
        assert_eq!(
            image::load_from_memory(f[0].image.as_ref().as_ref().unwrap())
                .unwrap()
                .width(),
            120,
            "full = raw frame"
        );

        // GROUP (Everyone) + THUMBNAIL: the contact-sheet — one image per source.
        let group = buffer
            .look(LookScope::Everyone, LookFidelity::Thumbnail, &compute)
            .await;
        assert_eq!(
            group.len(),
            2,
            "group scope → every source's current frame (the gallery)"
        );

        // Unknown source → empty (never a fabricated look).
        assert!(buffer
            .look(
                LookScope::Source("nobody".into()),
                LookFidelity::Thumbnail,
                &compute
            )
            .await
            .is_empty());
    }

    // what this catches: the change MONITOR — `scene_recently_changed` diffs the two most
    // recent WARMED luma signatures. Same scene (same pattern, different frame) → NOT changed
    // (decays to the floor); a visually different frame → changed (escalates). Unknown source
    // or unwarmed signatures → true (assume changed, ride the ceiling until we can tell).
    #[tokio::test]
    async fn scene_recently_changed_reads_warmed_signatures() {
        fn solid_png(w: u32, h: u32) -> Vec<u8> {
            let img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
            let mut out = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(img)
                .write_to(&mut out, ImageFormat::Png)
                .unwrap();
            out.into_inner()
        }

        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let src = "alice";

        // Unknown source → assume changed.
        assert!(buffer.scene_recently_changed(src, &compute));

        // Two visually-IDENTICAL frames (same red|blue pattern, different dims → different
        // content hash, so both are kept in the ring). Warm their signatures.
        let f1 = MediaFrame::from_bytes(png(80, 60));
        let f2 = MediaFrame::from_bytes(png(120, 90));
        f1.signature(&compute).await;
        f2.signature(&compute).await;
        {
            let mut rings = buffer.rings.lock().unwrap();
            let r = rings
                .entry(src.into())
                .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY));
            r.push(f1.clone());
            r.push(f2.clone());
        }
        assert!(
            !buffer.scene_recently_changed(src, &compute),
            "same scene across frames → not changed → decays to the slow floor"
        );

        // A visually DIFFERENT frame on top → changed.
        let solid = MediaFrame::from_bytes(solid_png(50, 50));
        solid.signature(&compute).await;
        buffer
            .rings
            .lock()
            .unwrap()
            .get_mut(src)
            .unwrap()
            .push(solid.clone());
        assert!(
            buffer.scene_recently_changed(src, &compute),
            "a different scene → changed → escalate to the ceiling"
        );
    }

    // what this catches: remove drops a participant who left the call.
    #[tokio::test]
    async fn remove_drops_a_participant() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);
        buffer.observe(
            "alice".into(),
            MediaFrame::from_bytes(png(10, 10)),
            compute,
            describer,
            "image/png",
            0,
        );
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
        let flag = buffer.should_warm(p, 0, true).expect("first frame warms");
        // Simulate the warm completing so the in-flight guard is not what's under test.
        flag.store(false, Ordering::Release);

        // A CHANGED frame 10ms later (30fps ≈ 33ms apart) → GATED by the MIN ceiling.
        assert!(
            buffer.should_warm(p, 10, true).is_none(),
            "a changed frame within the min ceiling is gated — perception samples, not mirrors, 30fps"
        );
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS - 1, true)
                .is_none(),
            "still within the min ceiling → still gated"
        );

        // Once the min ceiling elapses, a changed frame warms again.
        let flag2 = buffer
            .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS, true)
            .expect("min ceiling elapsed + changed → warm is due");
        flag2.store(false, Ordering::Release);
    }

    // what this catches: the BAND — "slow cadence, not NO cadence". Absent a change signal a
    // participant is NOT starved: it decays to the slow BASELINE floor (still warms every few
    // seconds), while a relevant change escalates it to the fast MIN ceiling.
    #[test]
    fn warm_cadence_is_a_band_change_escalates_static_decays_to_floor() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "carol";

        // First look happens regardless.
        buffer
            .should_warm(p, 0, false)
            .expect("first look")
            .store(false, Ordering::Release);

        // STATIC (changed=false): gated until the SLOW baseline floor — but NOT forever.
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS, false)
                .is_none(),
            "static past the min ceiling is still gated — no change, don't spend a describe"
        );
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS - 1, false)
                .is_none(),
            "static within the baseline floor → still gated"
        );
        buffer
            .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS, false)
            .expect("static past the baseline floor → STILL warms (slow cadence, not none)")
            .store(false, Ordering::Release);

        // CHANGE escalates: a relevant change warms as soon as the MIN ceiling allows,
        // long before the baseline floor.
        let t = AMBIENT_WARM_BASELINE_INTERVAL_MS + AMBIENT_WARM_MIN_INTERVAL_MS;
        buffer
            .should_warm(p, t, true)
            .expect("change past the min ceiling → escalates, no need to wait for the floor")
            .store(false, Ordering::Release);
    }

    // what this catches: never STACK a warm — while one is in flight, later observes are
    // gated even if the interval has elapsed, so a describe slower than the interval under
    // a fast stream cannot pile up concurrent inferences.
    #[test]
    fn warm_gate_never_stacks_an_in_flight_warm() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "bob";

        // Launch a warm and DON'T release it (simulates a describe still running).
        let _held = buffer.should_warm(p, 0, true).expect("first warms");

        // Even far past the interval, a second warm is refused while one is in flight.
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS * 100, true)
                .is_none(),
            "in-flight guard blocks stacking regardless of elapsed time"
        );

        // Once it completes, the next due tick warms again.
        _held.store(false, Ordering::Release);
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS * 100, true)
                .is_some(),
            "warm reopens after the in-flight one finishes"
        );
    }
}
