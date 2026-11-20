//! Her shadow's native birth: a fresh LoRA layer written for this model, which her genome then loads
//! and learns into ([`Model::write_fresh_shadow`]). The fork's one writer (`common_lora_write_fresh`).

use std::ffi::{c_char, CString};
use std::path::Path;

use super::*;

extern "C" {
    fn continuum_lora_write_fresh(
        model: *const sys::llama_model,
        path: *const c_char,
        rank: i32,
        alpha: f32,
        targets: *const *const c_char,
        n_targets: usize,
        seed: u32,
        top_layers: i32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
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
        let mut error = [0 as c_char; 512];
        let written = unsafe {
            continuum_lora_write_fresh(
                self.ptr.as_ptr(),
                c_path.as_ptr(),
                shape.rank,
                shape.alpha,
                pointers.as_ptr(),
                pointers.len(),
                shape.seed,
                shape.top_layers,
                error.as_mut_ptr(),
                error.len(),
            )
        };
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
