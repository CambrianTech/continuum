//! llama — our owned substrate for local LLM inference.
//!
//! Vendors llama.cpp as a git submodule, builds it via cmake, exposes a safe
//! Rust API. One binary, no external process, cross-platform via features.
//!
//! Features:
//!   - `metal`: Apple Silicon GPU (Mac)
//!   - `cuda`: NVIDIA GPU (Linux + Windows/WSL)
//!   - default: CPU with BLAS

pub mod sys {
    #![allow(non_camel_case_types, non_snake_case, non_upper_case_globals, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

mod mtmd;
mod safe;
pub use mtmd::MtmdContext;
pub use safe::*;
