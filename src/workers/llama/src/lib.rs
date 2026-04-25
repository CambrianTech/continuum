//! llama — our owned substrate for local LLM inference.
//!
//! Vendors llama.cpp as a git submodule, builds it via cmake, exposes a safe
//! Rust API. One binary, no external process, cross-platform via features.
//!
//! Features:
//!   - `metal`: Apple Silicon GPU (Mac) — REQUIRED on macOS
//!   - `cuda`: NVIDIA GPU (Linux + Windows/WSL)
//!   - default: CPU with BLAS

// ── Compile-time guard: no silent CPU-only Mac builds ─────────────────
// If you build this crate on macOS without `--features metal`, llama.cpp
// is compiled with GGML_METAL=OFF (per build.rs). That produces a library
// that SILENTLY FALLS BACK to CPU inference regardless of what
// `n_gpu_layers` is set to at runtime — no warning, no error, just a CPU
// model that runs ~20× slower than it should (12 tok/s instead of 60+
// tok/s on M-class Metal). This class of bug cost roughly a week of
// debugging (2026-04) because the runtime config path LOOKS correct:
// the Rust code passes `n_gpu_layers = -1` faithfully all the way through
// the FFI, but Metal simply doesn't exist in the compiled static library.
//
// Fail LOUD at compile time. Any Mac build path (cargo, Dockerfile, CI
// matrix, `npm start`) that reaches here without the feature flag now
// errors out with a clear message instead of shipping a broken binary.
// If you genuinely need CPU-only on macOS (rare — testing harness, x86
// cross-compile), delete this guard deliberately with a commit message
// justifying it. Don't silently pass a flag that removes it.
#[cfg(all(target_os = "macos", not(feature = "metal")))]
compile_error!(
    "\n\n\
     ===================================================================\n\
      llama crate built on macOS WITHOUT `--features metal`\n\
     ===================================================================\n\
     \n\
      This produces a CPU-ONLY build: llama.cpp compiled with\n\
      GGML_METAL=OFF. Token generation will run on CPU regardless of\n\
      `n_gpu_layers = -1` because Metal kernels are not in the binary.\n\
      Expect ~20x slowdown (12 tok/s instead of 60-100+ tok/s).\n\
     \n\
      FIX: add `--features metal` to your cargo build command.\n\
      Example:\n\
        cargo build --release -p continuum-core --features metal,accelerate\n\
     \n\
      If CPU-only on macOS is genuinely what you want (testing only),\n\
      delete this compile_error with a commit message justifying it.\n\
     ===================================================================\n"
);

pub mod sys {
    #![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

mod mtmd;
mod safe;
pub use mtmd::{MediaKind, MtmdContext};
pub use safe::*;
