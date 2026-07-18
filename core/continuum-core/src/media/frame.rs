//! `MediaFrame` — a media artifact as a CBAR VideoFrame: one source keyed by its
//! content hash, whose expensive derivatives are computed ONCE and shared
//! zero-copy across every consumer and persona.
//!
//! The compute-once/share-many cache is [`SharedCompute`](crate::runtime::SharedCompute)
//! (the runtime's CBAR_VideoFrame analog). The frame's **scope is its content
//! hash**, so two personas looking at the SAME bytes share every derivative — the
//! 13-way-video-chat scaling property ([[media-is-compute-once-zero-copy-hardware-grade]]).
//!
//! This slice lands the frame identity + the first derivative cell (scale/crop,
//! the fast lane). The deep cells (YOLO / semseg / description / vision-KV, and
//! audio transcription) attach the same way — `get_or_compute` on the content-hash
//! scope, async so they never stall the conversation ([[two-tier-resolution-mesh]]).

use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::image_ops::{scale_crop, CropRect, DestSize};
use crate::runtime::SharedCompute;

/// Produces the text DESCRIPTION of a media frame — the sensory bridge that lets a
/// non-vision model "see" ([[perception-feedback-must-not-blow-rag]]). Kept as a
/// trait so `media/` never depends on `cognition/` (the live implementor is
/// `cognition::vision_describe`, which routes `ai/generate`); tests inject a stub.
///
/// The frame caches ONE description per content hash via [`MediaFrame::description`],
/// so the same image described for 13 personas costs one describe call, not 13
/// ([[media-is-compute-once-zero-copy-hardware-grade]]).
#[async_trait]
pub trait FrameDescriber: Send + Sync {
    /// Describe these encoded image bytes as text. `mime` is the source encoding
    /// (e.g. `image/png`). Fails loud — a describe error is cached as `Err` (the
    /// bytes are fixed, so the failure is deterministic) and surfaced, never
    /// silently swapped for a placeholder ([[fallbacks-are-illegal-fail-loud]]).
    async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String>;
}

/// A media artifact addressed by the sha256 of its bytes. Cheap to clone (the
/// source is `Arc`-shared); derivatives live in [`SharedCompute`], not on the
/// frame, so many `MediaFrame` handles to the same content share one cache.
#[derive(Clone)]
pub struct MediaFrame {
    /// sha256-hex of the source bytes — the content address AND the SharedCompute
    /// scope under which every derivative of this frame is cached.
    content_hash: String,
    /// The encoded source bytes (a PNG/JPEG today). `Arc` so decoding/transform
    /// futures borrow it without copying the blob.
    source: Arc<Vec<u8>>,
}

impl MediaFrame {
    /// Wrap encoded image bytes, hashing them once to get the content address.
    pub fn from_bytes(source: Vec<u8>) -> Self {
        let content_hash = sha256_hex(&source);
        Self {
            content_hash,
            source: Arc::new(source),
        }
    }

    /// The content address (sha256-hex) — the SharedCompute scope for this frame's
    /// derivatives, and the handle by which the same content is deduped everywhere.
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    /// The source bytes, shared by `Arc` (zero-copy). This is the FULL-resolution
    /// derivative — a native-vision persona with room in its window gets these
    /// exact bytes, no scale, no clamp ([[perception-feedback-must-not-blow-rag]]).
    pub fn source(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.source)
    }

    /// The text DESCRIPTION of this frame — the sensory-bridge derivative, computed
    /// at most ONCE per content hash and shared as `Arc<Result<..>>`. Every persona
    /// (and every turn) that needs the description of THIS content gets the same
    /// cached string; the describer runs once ([[media-is-compute-once-zero-copy-hardware-grade]]).
    ///
    /// This is the deep-lane cell named in the module header: it may cost a real
    /// vision inference, so it is async and cached — call it ahead of the turn (or
    /// let it resolve async and bridge in) so it never stalls the conversation
    /// ([[media-is-compute-once-zero-copy-hardware-grade]] two-tier resolution).
    /// `describer` is passed by reference; the compute future borrows it and is
    /// awaited inline (SharedCompute does not spawn), so no `'static` clone is needed.
    pub async fn description(
        &self,
        compute: &SharedCompute,
        describer: &dyn FrameDescriber,
        mime: &str,
    ) -> Arc<Result<String, String>> {
        let source = Arc::clone(&self.source);
        let mime = mime.to_string();
        compute
            .get_or_compute(&self.content_hash, DESCRIBE_KEY, async move {
                describer.describe(&source, &mime).await
            })
            .await
    }

    /// Eagerly warm a set of scaled cells CONCURRENTLY — "if we know we'll need it,
    /// do it ahead of time, ONCE, not on the hot path." Call it when a frame is
    /// about to be shown (a room opening media, a bench about to score a UI) so the
    /// later reads are cache hits (the fast lane). Idempotent — an already-cached
    /// cell is a free hit, so re-warming never recomputes.
    pub async fn prefetch(&self, compute: &SharedCompute, sizes: &[DestSize]) {
        futures::future::join_all(sizes.iter().map(|&dest| self.scaled(compute, None, dest))).await;
    }

    /// A scaled/cropped derivative, computed at most ONCE per `(content, crop,
    /// dest)` via `compute` and shared as `Arc<Result<..>>`. Two calls with the
    /// same spec return the SAME `Arc` (zero-copy, one transform); different specs
    /// are distinct cells. A standard-size call is a thumbnail; a chain of these is
    /// a mip set to prefetch eagerly.
    ///
    /// The transform is deterministic, so caching its `Err` (bad bytes / spec) is
    /// correct — it will never spuriously differ. `compute` is passed by reference
    /// so the frame shares the ONE cache the caller owns (per-persona callers pass
    /// the runtime's shared instance; a bad frame never gets its own silo).
    pub async fn scaled(
        &self,
        compute: &SharedCompute,
        crop: Option<CropRect>,
        dest: DestSize,
    ) -> Arc<Result<Vec<u8>, String>> {
        let key = derivative_key(crop, dest);
        let source = Arc::clone(&self.source);
        compute
            .get_or_compute(&self.content_hash, &key, async move {
                scale_crop(&source, crop, dest)
            })
            .await
    }

    /// NON-BLOCKING read of an already-warmed scaled cell — the read-side twin of
    /// [`scaled`] (which WARMS async). Returns `Some(Arc<Result<..>>)` ONLY if the
    /// `(crop, dest)` derivative has ALREADY resolved on `compute`; `None` if it hasn't
    /// been computed yet. This is how live perception stays alive: a persona takes what's
    /// ready NOW and the rest bridges in on a later tick — it NEVER awaits a cell
    /// ([[command-async-shape-prefer-stream-never-block]]). Warm with `scaled`/`prefetch`
    /// (fire-and-forget in a spawned task); read here.
    pub fn scaled_if_ready(
        &self,
        compute: &SharedCompute,
        crop: Option<CropRect>,
        dest: DestSize,
    ) -> Option<Arc<Result<Vec<u8>, String>>> {
        compute.get::<Result<Vec<u8>, String>>(&self.content_hash, &derivative_key(crop, dest))
    }

    /// NON-BLOCKING read of the already-warmed description cell — the read-side twin of
    /// [`description`]. `Some` only if the describe cell has resolved; `None` otherwise.
    /// Same "take what's ready, never wait" contract as [`scaled_if_ready`].
    pub fn description_if_ready(
        &self,
        compute: &SharedCompute,
    ) -> Option<Arc<Result<String, String>>> {
        compute.get::<Result<String, String>>(&self.content_hash, DESCRIBE_KEY)
    }
}

/// sha256-hex of `bytes` — the content address (same hashing spill/vision use).
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The SharedCompute key for the description cell within a frame's scope. One
/// description per content hash — the describer/mime don't fork the key because the
/// bytes fully determine the derivative (the compute-once contract).
const DESCRIBE_KEY: &str = "describe";

/// The SharedCompute key for one scaled derivative within a frame's scope —
/// stable and unique per `(crop, dest)` so identical requests share a cell.
fn derivative_key(crop: Option<CropRect>, dest: DestSize) -> String {
    match crop {
        Some(c) => format!(
            "scale:{}x{}:crop:{},{},{},{}",
            dest.width, dest.height, c.x, c.y, c.width, c.height
        ),
        None => format!("scale:{}x{}:full", dest.width, dest.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
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

    // what this catches: content-addressing — the same bytes hash identically
    // (dedup key), different bytes differ. This is what makes two personas' frames
    // of the same content share ONE derivative cache.
    #[test]
    fn content_hash_is_stable_and_content_addressed() {
        let a = MediaFrame::from_bytes(png(10, 10));
        let b = MediaFrame::from_bytes(png(10, 10));
        let c = MediaFrame::from_bytes(png(12, 12));
        assert_eq!(a.content_hash(), b.content_hash(), "same bytes → same address");
        assert_ne!(a.content_hash(), c.content_hash(), "different bytes → different address");
        assert_eq!(a.content_hash().len(), 64, "sha256-hex");
    }

    // what this catches: THE compute-once-share property — a scaled cell is
    // computed once and every caller gets the SAME Arc (zero-copy), and a different
    // spec is a distinct cell. This is the 13-viewers-share-one-transform guarantee.
    #[tokio::test]
    async fn a_scaled_cell_computes_once_and_is_shared_zero_copy() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(100, 80));
        let dest = DestSize { width: 20, height: 16 };

        let first = frame.scaled(&compute, None, dest).await;
        let second = frame.scaled(&compute, None, dest).await;
        assert!(
            Arc::ptr_eq(&first, &second),
            "same spec must return the SAME Arc — computed once, shared zero-copy"
        );

        let bytes = first.as_ref().as_ref().expect("scale should succeed");
        let img = image::load_from_memory(bytes).unwrap();
        assert_eq!(img.dimensions(), (20, 16));

        // A different destination is a distinct cell (its own cached transform).
        let other = frame.scaled(&compute, None, DestSize { width: 10, height: 8 }).await;
        assert!(!Arc::ptr_eq(&first, &other), "different spec → different cell");
    }

    // what this catches: prefetch WARMS cells ahead of time — after it, the
    // requested sizes are already in the cache (proven by key_count), so the later
    // hot-path reads are free hits. "Do it ahead of time, once, not on the hot path."
    #[tokio::test]
    async fn prefetch_warms_cells_ahead_of_time() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(64, 64));
        let sizes = [
            DestSize { width: 16, height: 16 },
            DestSize { width: 32, height: 32 },
        ];

        assert_eq!(compute.key_count(frame.content_hash()), 0, "cold");
        frame.prefetch(&compute, &sizes).await;
        assert_eq!(
            compute.key_count(frame.content_hash()),
            sizes.len(),
            "prefetch warmed every requested size"
        );

        // A later read is a cache hit — same Arc, no recompute.
        let hit = frame.scaled(&compute, None, sizes[0]).await;
        let again = frame.scaled(&compute, None, sizes[0]).await;
        assert!(Arc::ptr_eq(&hit, &again));
    }

    /// A describer that counts how many times it actually ran — proves the
    /// description cell computes once and shares. Returns a fixed string keyed on
    /// byte length so different content yields different descriptions.
    struct CountingDescriber {
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }
    #[async_trait]
    impl FrameDescriber for CountingDescriber {
        async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(format!("a {mime} image of {} bytes", source.len()))
        }
    }

    // what this catches: THE description compute-once property — the describer (a
    // potentially expensive vision call) runs at most ONCE per content hash; every
    // later request returns the SAME cached Arc. This is the "13 personas → one
    // describe" guarantee for the non-vision sensory bridge.
    #[tokio::test]
    async fn a_description_is_computed_once_and_shared() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let compute = SharedCompute::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let describer = CountingDescriber {
            calls: Arc::clone(&calls),
        };
        let frame = MediaFrame::from_bytes(png(30, 30));

        let first = frame.description(&compute, &describer, "image/png").await;
        let second = frame.description(&compute, &describer, "image/png").await;

        assert!(first.as_ref().is_ok(), "describe should succeed");
        assert!(
            Arc::ptr_eq(&first, &second),
            "same content → SAME cached description Arc (computed once)"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1, "describer ran at most once");
    }

    // what this catches: a describe FAILURE is cached as Err and surfaced — the
    // bytes are fixed so the failure is deterministic, and we never silently swap a
    // placeholder ([[fallbacks-are-illegal-fail-loud]]).
    #[tokio::test]
    async fn a_description_failure_is_cached_and_surfaced() {
        struct FailingDescriber;
        #[async_trait]
        impl FrameDescriber for FailingDescriber {
            async fn describe(&self, _: &[u8], _: &str) -> Result<String, String> {
                Err("vision model unavailable".into())
            }
        }
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(8, 8));
        let d = frame.description(&compute, &FailingDescriber, "image/png").await;
        assert_eq!(d.as_ref().as_ref().unwrap_err(), "vision model unavailable");
    }

    // what this catches: THE non-blocking-read contract — a cell reads None BEFORE it's
    // warmed (perception takes what's ready, never waits), and after warming the _if_ready
    // read returns the SAME cached Arc (zero-copy, no recompute). How live perception stays
    // alive: fire the warm, read what's ready NOW, the rest bridges in later.
    #[tokio::test]
    async fn ready_reads_are_none_until_warmed_then_share_the_cell() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(50, 40));
        let dest = DestSize { width: 20, height: 16 };
        let describer = CountingDescriber {
            calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };

        // Cold: nothing warmed → non-blocking reads see nothing (absent this tick).
        assert!(frame.scaled_if_ready(&compute, None, dest).is_none(), "scaled cold → None");
        assert!(frame.description_if_ready(&compute).is_none(), "describe cold → None");

        // Warm the cells (the async fire; in the PerceptionBuffer this is a spawned task).
        let warmed_scaled = frame.scaled(&compute, None, dest).await;
        let warmed_desc = frame.description(&compute, &describer, "image/png").await;

        // Non-blocking reads now return the SAME cached Arc — ready, zero-copy.
        let read_scaled = frame.scaled_if_ready(&compute, None, dest).expect("scaled ready");
        let read_desc = frame.description_if_ready(&compute).expect("describe ready");
        assert!(Arc::ptr_eq(&read_scaled, &warmed_scaled), "ready read shares the warmed cell");
        assert!(Arc::ptr_eq(&read_desc, &warmed_desc), "ready read shares the warmed cell");
        // A DIFFERENT spec is still cold — reads only what was actually warmed.
        assert!(frame
            .scaled_if_ready(&compute, None, DestSize { width: 8, height: 8 })
            .is_none());
    }

    // what this catches: two SEPARATE frame handles to the SAME content share the
    // cache (scope = content hash) — the cross-persona dedup that makes the video
    // wall affordable.
    #[tokio::test]
    async fn separate_handles_to_same_content_share_the_cell() {
        let compute = SharedCompute::new();
        let bytes = png(40, 40);
        let persona_a_frame = MediaFrame::from_bytes(bytes.clone());
        let persona_b_frame = MediaFrame::from_bytes(bytes);
        let dest = DestSize { width: 8, height: 8 };

        let a = persona_a_frame.scaled(&compute, None, dest).await;
        let b = persona_b_frame.scaled(&compute, None, dest).await;
        assert!(
            Arc::ptr_eq(&a, &b),
            "same content + same spec across handles must hit ONE shared cell"
        );
    }
}
