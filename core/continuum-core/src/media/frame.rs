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

use sha2::{Digest, Sha256};

use super::image_ops::{scale_crop, CropRect, DestSize};
use crate::runtime::SharedCompute;

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
