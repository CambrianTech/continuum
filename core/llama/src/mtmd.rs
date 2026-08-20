//! Multimodal projector — safe wrapper around llama.cpp's `libmtmd`.
//!
//! `libmtmd` is the upstream library that handles vision/audio projection
//! for multimodal models (Qwen2-VL, LLaVA, MiniCPM-V, Llama-3.2-Vision,
//! etc.). It loads a mmproj GGUF (the vision encoder + cross-modal
//! projection weights), encodes raw image / audio bytes into model-
//! native tokens, and evaluates them through a normal llama_context so
//! subsequent text generation can attend over the encoded media.
//!
//! Marked experimental upstream — the C API may change. We pin against
//! the vendored llama.cpp version and re-test on bumps.
//!
//! Typical use (matching mtmd-cli.cpp):
//!
//! ```ignore
//! let model = Model::load("qwen2-vl-7b.gguf", ModelParams::default())?;
//! let mut lctx = model.new_context(ContextParams::default())?;
//! let mtmd = MtmdContext::from_file("mmproj-qwen2-vl.gguf", &model)?;
//! let n_past = mtmd.eval_image(&mut lctx, "<__media__>What's in this picture?", &png_bytes, 0, 512, 0)?;
//! // ... continue with normal sampler.sample(&lctx, ...) loop, starting from n_past
//! ```
//!
//! The `<__media__>` marker (or whatever `mtmd_default_marker()` returns)
//! tells the tokenizer where in the text to splice the image tokens.

use crate::sys;
use crate::{Context, Model};
use std::ffi::CString;
use std::path::Path;
use std::ptr::NonNull;

/// Which modality the caller is asking the mtmd projector to process.
/// Used for capability checks + error-message specificity. The underlying
/// bitmap helper auto-detects image vs audio from magic bytes either way,
/// but the caller's intent is what tells us which capability to enforce
/// and which projector mismatch to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Audio,
}

/// Multimodal projector context. Loaded once per (mmproj, model) pair and
/// reused across many image evaluations.
pub struct MtmdContext {
    ptr: NonNull<sys::mtmd_context>,
}

unsafe impl Send for MtmdContext {}
unsafe impl Sync for MtmdContext {}

impl MtmdContext {
    /// Load a multimodal projector from a mmproj GGUF and bind it to the
    /// given text model. The model must already be loaded — the projector
    /// produces tokens compatible with this specific model's embedding
    /// space, so a Qwen2-VL mmproj only works with a Qwen2-VL text model.
    pub fn from_file(mmproj_path: impl AsRef<Path>, model: &Model) -> Result<Self, String> {
        let path = mmproj_path.as_ref();
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("invalid mmproj path: {e}"))?;
        let params = unsafe { sys::mtmd_context_params_default() };
        let raw = unsafe { sys::mtmd_init_from_file(c_path.as_ptr(), model.as_ptr(), params) };
        let ptr = NonNull::new(raw).ok_or_else(|| {
            format!(
                "mtmd_init_from_file failed for {} — wrong mmproj/model pair, missing file, or unsupported architecture",
                path.display()
            )
        })?;
        Ok(Self { ptr })
    }

    /// `true` if the projector accepts image input. Some mmproj files are
    /// audio-only (e.g., Qwen2-Audio); the policy needs this to skip
    /// routing image media to a model that won't use it.
    pub fn supports_vision(&self) -> bool {
        unsafe { sys::mtmd_support_vision(self.ptr.as_ptr()) }
    }

    /// `true` if the projector accepts audio input.
    pub fn supports_audio(&self) -> bool {
        unsafe { sys::mtmd_support_audio(self.ptr.as_ptr()) }
    }

    /// The default media marker string the tokenizer recognizes (e.g.
    /// `<__media__>`). Caller must include this exact substring inside the
    /// text passed to `eval_image` — that's where the image tokens get
    /// spliced into the prompt.
    pub fn default_marker() -> &'static str {
        unsafe {
            let p = sys::mtmd_default_marker();
            std::ffi::CStr::from_ptr(p)
                .to_str()
                .unwrap_or("<__media__>")
        }
    }

    /// Tokenize `text` (which must contain the media marker, see
    /// `default_marker()`) together with `image_bytes`, then evaluate the
    /// resulting interleaved chunks through `lctx` starting at `n_past`.
    ///
    /// Returns the new `n_past` after evaluation — the caller continues
    /// the normal sampler-loop from this position. `seq_id` selects which
    /// sequence in the shared context receives the tokens.
    ///
    /// `logits_last` controls whether logits for the very last token are
    /// computed (true if the next step is sampling, false if more eval
    /// calls follow).
    ///
    /// Thin wrapper for the single-image case. For audio-only callers see
    /// `eval_audio`; for the eventual mixed-media case, `eval_media` is
    /// the underlying workhorse (currently single-bitmap; multi-marker
    /// support is a follow-up once a real caller needs it).
    pub fn eval_image(
        &self,
        lctx: &mut Context,
        text: &str,
        image_bytes: &[u8],
        n_past: i32,
        n_batch: i32,
        seq_id: i32,
        logits_last: bool,
    ) -> Result<i32, String> {
        self.eval_media(lctx, text, image_bytes, n_past, n_batch, seq_id, logits_last, MediaKind::Image)
    }

    /// Audio analogue of `eval_image`. The underlying mtmd helper
    /// (`mtmd_helper_bitmap_init_from_buf`) auto-detects audio vs image
    /// from magic bytes and routes through the same bitmap+chunks+eval
    /// pipeline. Different entry points exist (a) so the error messages
    /// and capability checks stay specific to the modality the caller
    /// asked for and (b) because adapter routing reads the request's
    /// ContentPart variant explicitly — silently letting an "image"
    /// caller succeed on audio bytes (or vice versa) would mask a
    /// classification bug upstream.
    ///
    /// Supported audio container formats are whatever miniaudio
    /// understands (wav, mp3, flac per upstream mtmd-helper.h).
    pub fn eval_audio(
        &self,
        lctx: &mut Context,
        text: &str,
        audio_bytes: &[u8],
        n_past: i32,
        n_batch: i32,
        seq_id: i32,
        logits_last: bool,
    ) -> Result<i32, String> {
        self.eval_media(lctx, text, audio_bytes, n_past, n_batch, seq_id, logits_last, MediaKind::Audio)
    }

    /// Internal workhorse — single-bitmap eval (image OR audio, whichever
    /// the bytes turn out to be). The `kind` argument shapes only the
    /// error messages so failures point at the actual modality the
    /// caller asked for; the underlying mtmd code path is identical.
    fn eval_media(
        &self,
        lctx: &mut Context,
        text: &str,
        media_bytes: &[u8],
        n_past: i32,
        n_batch: i32,
        seq_id: i32,
        logits_last: bool,
        kind: MediaKind,
    ) -> Result<i32, String> {
        // Step 1: load bitmap from raw bytes — the helper auto-detects
        // image vs audio from magic bytes (per mtmd-helper.h: stb_image
        // formats for images, miniaudio formats wav/mp3/flac for audio).
        // Upstream 2026-07: gained `placeholder` (false = real media bytes) and returns a
        // wrapper { bitmap, video_ctx }. video_ctx is only populated for VIDEO inputs —
        // unsupported here (images/audio only), where it is null; the bitmap is ours to free.
        let wrapper = unsafe {
            sys::mtmd_helper_bitmap_init_from_buf(
                self.ptr.as_ptr(),
                media_bytes.as_ptr(),
                media_bytes.len(),
                false,
            )
        };
        let bitmap = NonNull::new(wrapper.bitmap).ok_or_else(|| {
            format!(
                "mtmd_helper_bitmap_init_from_buf failed — bytes not a valid {} format",
                match kind {
                    MediaKind::Image => "image (JPEG/PNG/BMP/etc)",
                    MediaKind::Audio => "audio (WAV/MP3/FLAC)",
                }
            )
        })?;

        // RAII: free bitmap + chunks even if we early-return on error.
        struct BitmapGuard(NonNull<sys::mtmd_bitmap>);
        impl Drop for BitmapGuard {
            fn drop(&mut self) {
                unsafe { sys::mtmd_bitmap_free(self.0.as_ptr()) }
            }
        }
        let _bitmap_guard = BitmapGuard(bitmap);

        // Step 2: allocate the chunks output container.
        let chunks = unsafe { sys::mtmd_input_chunks_init() };
        let chunks = NonNull::new(chunks)
            .ok_or_else(|| "mtmd_input_chunks_init returned null".to_string())?;

        struct ChunksGuard(NonNull<sys::mtmd_input_chunks>);
        impl Drop for ChunksGuard {
            fn drop(&mut self) {
                unsafe { sys::mtmd_input_chunks_free(self.0.as_ptr()) }
            }
        }
        let _chunks_guard = ChunksGuard(chunks);

        // Step 3: tokenize text + image into mixed chunks.
        let c_text = CString::new(text).map_err(|e| format!("invalid text (NUL byte?): {e}"))?;
        let input_text = sys::mtmd_input_text {
            text: c_text.as_ptr(),
            // Upstream 2026-07: explicit byte length alongside the pointer (excl. NUL).
            text_len: c_text.as_bytes().len(),
            add_special: true,
            parse_special: true,
        };
        let mut bitmap_ptrs: [*const sys::mtmd_bitmap; 1] = [bitmap.as_ptr() as *const _];
        let tok_rc = unsafe {
            sys::mtmd_tokenize(
                self.ptr.as_ptr(),
                chunks.as_ptr(),
                &input_text,
                bitmap_ptrs.as_mut_ptr(),
                bitmap_ptrs.len(),
            )
        };
        if tok_rc != 0 {
            return Err(format!(
                "mtmd_tokenize returned {tok_rc} — likely text is missing the media marker (`{}`) or model+mmproj mismatch",
                Self::default_marker()
            ));
        }

        // Diagnostic: print chunk structure to stderr so we can compare
        // against brew's verbose output (which shows add_text / image
        // insertions in eval order). Silenced via env to avoid test noise.
        if std::env::var_os("MTMD_DEBUG_CHUNKS").is_some() {
            let n_chunks = unsafe { sys::mtmd_input_chunks_size(chunks.as_ptr()) };
            eprintln!("[mtmd-dbg] mtmd_tokenize produced {} chunks", n_chunks);
            for i in 0..n_chunks {
                let chunk = unsafe { sys::mtmd_input_chunks_get(chunks.as_ptr(), i) };
                let ctype = unsafe { sys::mtmd_input_chunk_get_type(chunk) };
                let n_pos = unsafe { sys::mtmd_input_chunk_get_n_pos(chunk) };
                eprintln!("[mtmd-dbg]   chunk[{}] type={} n_pos={}", i, ctype, n_pos);
                if ctype == sys::mtmd_input_chunk_type_MTMD_INPUT_CHUNK_TYPE_TEXT {
                    let mut n_tokens: usize = 0;
                    let toks_ptr = unsafe {
                        sys::mtmd_input_chunk_get_tokens_text(chunk, &mut n_tokens)
                    };
                    if !toks_ptr.is_null() && n_tokens > 0 {
                        let toks = unsafe { std::slice::from_raw_parts(toks_ptr, n_tokens) };
                        eprintln!("[mtmd-dbg]     tokens ({} total): {:?}", n_tokens, toks);
                    }
                }
            }
        }

        // Step 4: evaluate the chunks through llama_context, advancing n_past.
        let mut new_n_past: sys::llama_pos = n_past;
        let eval_rc = unsafe {
            sys::mtmd_helper_eval_chunks(
                self.ptr.as_ptr(),
                lctx.as_ptr(),
                chunks.as_ptr(),
                n_past,
                seq_id,
                n_batch,
                logits_last,
                &mut new_n_past,
            )
        };
        if eval_rc != 0 {
            return Err(format!(
                "mtmd_helper_eval_chunks returned {eval_rc} — KV exhausted, decode error, or n_batch too small for image tokens"
            ));
        }

        Ok(new_n_past)
    }
}

impl Drop for MtmdContext {
    fn drop(&mut self) {
        unsafe { sys::mtmd_free(self.ptr.as_ptr()) }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the default media marker string drifting from
    /// the upstream value the tokenizer expects. If this changes silently
    /// in a llama.cpp bump, prompts built around the OLD marker would
    /// fail at `mtmd_tokenize` with no image tokens spliced in.
    ///
    /// Validated 2026-04-21: hardcoded the helper's return to a wrong
    /// string via test fixture; not strictly mutation-validated since
    /// the upstream constant is opaque, but the assertion documents what
    /// shape the value MUST have.
    #[test]
    fn default_marker_is_well_formed() {
        let m = MtmdContext::default_marker();
        assert!(!m.is_empty(), "default marker must be a non-empty string");
        assert!(
            m.starts_with('<') && m.ends_with('>'),
            "default marker should be a tag-like token, got {m:?}"
        );
    }
}
