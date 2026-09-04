//! Per-persona media PROJECTION — the ONE place that decides which cached
//! [`MediaFrame`] derivative a persona actually receives, DERIVED from that
//! persona's live model capability, never a top-down clamp
//! ([[perception-feedback-must-not-blow-rag]]).
//!
//! # Derive, don't clamp
//!
//! Capability is the GATE, resolution is the KNOB — both threaded IN by reference,
//! nothing invented here:
//! - A **non-vision** model can never receive pixels; it gets the bridged text
//!   description regardless of what resolution was requested. The sensory bridge, so
//!   a lesser model still "sees" ([[built-to-teach-lesser-tuned-intelligences-win]]).
//! - A **vision** model gets exactly the resolution the situation asked for — `Full`
//!   is genuinely the full source pixels (no standing thumbnail cap, no imposed
//!   limit); `Scaled` is a role/budget/adapter-max size; `Describe`/`Handle` are the
//!   summary/placeholder rungs. We never PREVENT a capable model from seeing a big
//!   image — the system ADAPTS to its real capability
//!   ([[perception-feedback-must-not-blow-rag]], [[use-adapters-dont-dumb-it-down]]).
//!
//! # Zero-copy
//!
//! Every image/description variant holds the SAME `Arc` the [`SharedCompute`] cache
//! holds — the projection SELECTS a cached cell, it never recomputes or copies bytes
//! ([[media-is-compute-once-zero-copy-hardware-grade]]). N personas projecting the
//! same content-hash frame at the same resolution share one derivative.
//!
//! This is the consumption-side half of the substrate: `frame.rs` computes the
//! derivatives once; THIS picks the right one per persona.

use std::collections::HashSet;
use std::sync::Arc;

use super::frame::{FrameDescriber, MediaFrame};
use super::image_ops::DestSize;
use crate::model_registry::Capability;
use crate::runtime::SharedCompute;

/// How much of a frame a consumer wants — the resolution knob set by role / situation
/// / budget. Media-local (so `media/` never imports `persona/`); the render or RAG
/// caller maps its own `ResolutionPreference` onto this. Capability still gates:
/// asking for `Full` on a non-vision model yields a description, not pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaResolution {
    /// Full source pixels — a native-vision persona with room in its window sees the
    /// image exactly as-is. No scale, no clamp.
    Full,
    /// A bounded derivative at `dest` — the adapter's max accepted dims, or a
    /// role/budget-appropriate size (a standard thumbnail is just this).
    Scaled(DestSize),
    /// Force the text-description path even on a vision model — a summary rung for a
    /// tight budget or a passing mention.
    Describe,
    /// Just a handle (content hash) — nothing inline; the full-res artifact is
    /// reachable later by content address ([[handles-events-expansion-one-universal-primitive]]).
    Handle,
}

/// The projected form of a frame FOR a specific persona — what actually enters the
/// inference request. Each variant holds the shared cache `Arc` (zero-copy); the
/// derivative `Result`s ride inline so a failed transform is surfaced, never
/// silently replaced ([[fallbacks-are-illegal-fail-loud]]).
#[derive(Debug, Clone)]
pub enum ProjectedMedia {
    /// Native-vision FULL-resolution source pixels, shared zero-copy, with the
    /// source mime.
    Full { bytes: Arc<Vec<u8>>, mime: String },
    /// A scaled cell (re-encoded PNG) — the exact `Arc` the derivative cache holds.
    /// `Err` when the source could not be decoded/scaled.
    Scaled(Arc<Result<Vec<u8>, String>>),
    /// The bridged text description — the exact `Arc` the description cache holds.
    /// `Err` when the describer failed.
    Description(Arc<Result<String, String>>),
    /// A placeholder handle: the full-res artifact is addressable by `content_hash`.
    Handle { content_hash: String, mime: String },
}

impl ProjectedMedia {
    /// Whether this projection carries real pixels the model will natively consume
    /// (a successful `Full` or `Scaled`). A `Description`/`Handle`/failed cell is not
    /// a byte attachment.
    pub fn is_image_bytes(&self) -> bool {
        match self {
            ProjectedMedia::Full { .. } => true,
            ProjectedMedia::Scaled(cell) => cell.as_ref().is_ok(),
            _ => false,
        }
    }
}

/// Project a media frame for ONE persona: capability gates, resolution selects, the
/// cache serves. The persona's `caps` and the runtime's `compute` are borrowed —
/// this recomputes nothing and clamps nothing, it picks the already-computed cell
/// that matches the persona's real model + the requested resolution.
///
/// `mime` is the source encoding, used both for the description prompt and to label
/// a `Full`/`Handle` projection. `describer` produces (once, cached) the text for
/// the non-vision bridge and the `Describe` rung.
pub async fn project_image(
    frame: &MediaFrame,
    caps: &HashSet<Capability>,
    resolution: MediaResolution,
    mime: &str,
    compute: &SharedCompute,
    describer: &dyn FrameDescriber,
) -> ProjectedMedia {
    // Capability is the GATE: a non-vision model NEVER gets pixels — it gets the
    // bridged description whatever resolution was asked. This is the sensory bridge,
    // not a clamp: a capable model below still gets exactly its requested resolution.
    if !caps.contains(&Capability::Vision) {
        return ProjectedMedia::Description(frame.description(compute, describer, mime).await);
    }
    match resolution {
        MediaResolution::Full => ProjectedMedia::Full {
            bytes: frame.source(),
            mime: mime.to_string(),
        },
        MediaResolution::Scaled(dest) => {
            ProjectedMedia::Scaled(frame.scaled(compute, None, dest).await)
        }
        MediaResolution::Describe => {
            ProjectedMedia::Description(frame.description(compute, describer, mime).await)
        }
        MediaResolution::Handle => ProjectedMedia::Handle {
            content_hash: frame.content_hash().to_string(),
            mime: mime.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
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

    fn vision() -> HashSet<Capability> {
        let mut s = HashSet::new();
        s.insert(Capability::Vision);
        s
    }

    struct StubDescriber;
    #[async_trait]
    impl FrameDescriber for StubDescriber {
        async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
            Ok(format!("{mime} image, {} bytes", source.len()))
        }
    }

    // what this catches: THE derive-not-clamp gate — a non-vision model NEVER gets
    // pixels, even when Full is requested; it gets the bridged description. This is
    // the sensory bridge that makes a lesser model see.
    #[tokio::test]
    async fn a_non_vision_model_gets_a_description_even_asking_for_full() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(40, 40));
        let p = project_image(
            &frame,
            &HashSet::new(), // no Vision
            MediaResolution::Full,
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        match p {
            ProjectedMedia::Description(cell) => {
                assert!(cell.as_ref().is_ok(), "bridge description should resolve");
            }
            other => panic!("non-vision must project a Description, got {other:?}"),
        }
    }

    // what this catches: a vision model asking for Full gets the EXACT source bytes,
    // shared zero-copy (same Arc allocation as frame.source()) — no scale, no clamp.
    #[tokio::test]
    async fn a_vision_model_gets_full_source_pixels_zero_copy() {
        let compute = SharedCompute::new();
        let bytes = png(64, 48);
        let frame = MediaFrame::from_bytes(bytes.clone());
        let p = project_image(
            &frame,
            &vision(),
            MediaResolution::Full,
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        match p {
            ProjectedMedia::Full { bytes: got, mime } => {
                assert_eq!(&*got, &bytes, "full projection carries the source bytes");
                assert!(
                    Arc::ptr_eq(&got, &frame.source()),
                    "shared zero-copy, not a re-clone"
                );
                assert_eq!(mime, "image/png");
            }
            other => panic!("vision+Full must project Full pixels, got {other:?}"),
        }
    }

    // what this catches: the Scaled rung selects the cached scaled cell — the bytes
    // decode to the requested dims, and two projections at the same size share ONE
    // Arc (computed once). This is the role/budget/adapter-max resolution knob.
    #[tokio::test]
    async fn a_scaled_projection_selects_the_shared_cell() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(100, 80));
        let dest = DestSize {
            width: 20,
            height: 16,
        };

        let first = project_image(
            &frame,
            &vision(),
            MediaResolution::Scaled(dest),
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        let second = project_image(
            &frame,
            &vision(),
            MediaResolution::Scaled(dest),
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;

        let (a, b) = match (first, second) {
            (ProjectedMedia::Scaled(a), ProjectedMedia::Scaled(b)) => (a, b),
            _ => panic!("vision+Scaled must project Scaled cells"),
        };
        assert!(
            Arc::ptr_eq(&a, &b),
            "same size → SAME cached Arc (computed once)"
        );
        let bytes = a.as_ref().as_ref().expect("scale should succeed");
        assert_eq!(
            image::load_from_memory(bytes).unwrap().dimensions(),
            (20, 16)
        );
    }

    // what this catches: Describe forces the text path even on a vision model, and it
    // reuses the SAME description cell a non-vision projection would (one describe per
    // content hash across ALL personas, capable or not).
    #[tokio::test]
    async fn describe_forces_text_and_shares_the_one_description_cell() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(24, 24));

        let vision_describe = project_image(
            &frame,
            &vision(),
            MediaResolution::Describe,
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        let direct = frame
            .description(&compute, &StubDescriber, "image/png")
            .await;

        match vision_describe {
            ProjectedMedia::Description(cell) => {
                assert!(
                    Arc::ptr_eq(&cell, &direct),
                    "vision Describe shares the ONE description cell"
                );
            }
            other => panic!("Describe must project a Description, got {other:?}"),
        }
    }

    // what this catches: the Handle rung projects a content-addressed placeholder —
    // nothing inline, full res reachable by the hash. The "don't blow RAG" floor.
    #[tokio::test]
    async fn handle_projects_a_content_addressed_placeholder() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(16, 16));
        let p = project_image(
            &frame,
            &vision(),
            MediaResolution::Handle,
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        match p {
            ProjectedMedia::Handle { content_hash, mime } => {
                assert_eq!(content_hash, frame.content_hash());
                assert_eq!(mime, "image/png");
            }
            other => panic!("Handle must project a placeholder, got {other:?}"),
        }
    }

    // what this catches: is_image_bytes() distinguishes a real byte attachment (a
    // successful Full/Scaled) from a description/handle — the render seam uses this to
    // decide whether an item consumes the one byte-attachment slot.
    #[tokio::test]
    async fn is_image_bytes_only_true_for_successful_pixels() {
        let compute = SharedCompute::new();
        let frame = MediaFrame::from_bytes(png(20, 20));
        let full = project_image(
            &frame,
            &vision(),
            MediaResolution::Full,
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        let desc = project_image(
            &frame,
            &HashSet::new(),
            MediaResolution::Full,
            "image/png",
            &compute,
            &StubDescriber,
        )
        .await;
        assert!(full.is_image_bytes());
        assert!(!desc.is_image_bytes());
    }
}
