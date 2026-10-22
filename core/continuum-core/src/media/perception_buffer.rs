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
mod tests;
