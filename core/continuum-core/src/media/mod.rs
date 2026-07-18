//! `media` — the efficient, compute-once, share-many media substrate.
//!
//! A media artifact (image/frame/audio) is a **CBAR VideoFrame**: one source
//! keyed by its content hash, whose expensive DERIVATIVES are computed at most
//! once and shared zero-copy across every consumer and persona:
//! - scaled sizes (a mipmap chain of standard thumbnails) + arbitrary scale/crop,
//! - the CV aids (YOLO / semseg / OCR) that let a non-vision model perceive,
//! - the text description (`cognition::vision_describe`, already content-addressed),
//! - the vision-encode embedding / KV ("preserve one KV cache"),
//! - for audio: the transcription + features.
//!
//! The compute-once/share-many cache is [`runtime::SharedCompute`](crate::runtime::SharedCompute)
//! — the runtime's CBAR_VideoFrame analog (`get_or_compute(scope, key, fut) ->
//! Arc<T>`). Scope = the content hash, so two personas looking at the same frame
//! share ALL of its derivatives. We build ON that cache, never a parallel one
//! ([[media-is-compute-once-zero-copy-hardware-grade]],
//! [[embeddings-are-per-content-computed-once-shared]]).
//!
//! A per-persona projection then only SELECTS the right cached derivative for a
//! persona's model + adapter (threaded by reference), never recomputes and never
//! clamps ([[perception-feedback-must-not-blow-rag]]).
//!
//! This module starts with [`image_ops`] — the canvas-style scale/crop primitive
//! every scaled cell / mip level is built from — and [`frame`], the content-hash
//! `MediaFrame` that caches derivatives on `SharedCompute` (compute-once, shared).

pub mod frame;
pub mod image_ops;
pub mod projection;

pub use frame::{FrameDescriber, MediaFrame};
pub use projection::{project_image, MediaResolution, ProjectedMedia};
