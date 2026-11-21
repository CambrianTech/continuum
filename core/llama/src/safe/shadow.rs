//! Her shadow's native birth: a fresh LoRA layer written for this model, which her genome then loads
//! and learns into ([`Model::write_fresh_shadow`]). The fork's one writer (`common_lora_write_fresh`).

use std::ffi::{c_char, CString};
use std::path::Path;

use super::*;

extern "C" {
    fn continuum_lora_write_fresh(
        model: *const sys::llama_model,
        path: *const c_char,
        shape: *const NativeShadowShape,
        error: *mut NativeMessage,
    ) -> i32;
}

/// `continuum_shadow_shape` in `genome_boundary.cpp`.
#[repr(C)]
struct NativeShadowShape {
    rank: i32,
    alpha: f32,
    targets: *const *const c_char,
    n_targets: usize,
    seed: u32,
    top_layers: i32,
}

/// `continuum_message` in `genome_boundary.cpp`: where native code writes what it threw.
#[repr(C)]
struct NativeMessage {
    text: *mut c_char,
    capacity: usize,
}

/// The shape of a fresh shadow: its rank and scale, which weights it adapts (`attn_q`, `attn_v`, ...),
/// how many of the top blocks (0 = all), and the seed of its random A.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowShape {
    pub rank: i32,
    pub alpha: f32,
    pub targets: Vec<String>,
    pub top_layers: i32,
    pub seed: u32,
}

/// Why no shadow was written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShadowRefusal {
    #[error("the shadow's {0} contains a NUL byte")]
    Nul(&'static str),
    #[error("native code refused to write the shadow (no target weight matched, or the file could not be written)")]
    Refused,
    #[error("native code failed writing the shadow: {0}")]
    Native(String),
}

impl Model {
    /// Write a fresh shadow for this model at `path`: A uniform in +-1/sqrt(n_in), B zero, both F32, so
    /// the model's output is unchanged until her first optimizer step moves B.
    pub fn write_fresh_shadow(&self, path: &Path, shape: &ShadowShape) -> Result<(), ShadowRefusal> {
        let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| ShadowRefusal::Nul("path"))?;
        let names: Vec<CString> = shape
            .targets
            .iter()
            .map(|target| CString::new(target.as_bytes()).map_err(|_| ShadowRefusal::Nul("target")))
            .collect::<Result<_, _>>()?;
        let pointers: Vec<*const c_char> = names.iter().map(|name| name.as_ptr()).collect();
        let native = NativeShadowShape {
            rank: shape.rank,
            alpha: shape.alpha,
            targets: pointers.as_ptr(),
            n_targets: pointers.len(),
            seed: shape.seed,
            top_layers: shape.top_layers,
        };
        let mut error = [0 as c_char; 512];
        let mut message = NativeMessage { text: error.as_mut_ptr(), capacity: error.len() };
        // SAFETY: every pointer in `native` and `message` borrows a value that outlives the call.
        let written = unsafe { continuum_lora_write_fresh(self.ptr.as_ptr(), c_path.as_ptr(), &native, &mut message) };
        match written {
            1 => Ok(()),
            0 => Err(ShadowRefusal::Refused),
            _ => {
                let message = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }.to_string_lossy().into_owned();
                Err(ShadowRefusal::Native(message))
            }
        }
    }
}
