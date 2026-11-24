//! A shadow's life outside a reply (IN-PROCESS-LEARNING.md, L2): written fresh, measured before any
//! byte of it is allocated, and saved as a generation. Measurement is native code's own: what it
//! reports is what it would allocate, row by row, so the memory owner leases exactly that.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::num::NonZeroU32;
use std::path::Path;

use super::{shadow_params, Learner, LearnRefusal, MemoryRole, MemoryRow, Shadow};
use crate::safe::{LoraAdapter, Model};
use crate::sys;

extern "C" {
    fn continuum_lora_write_fresh(
        model: *const sys::llama_model,
        path: *const c_char,
        rank: i32,
        alpha: f32,
        targets: *const c_char,
        seed: u32,
        top_layers: i32,
        measure: sys::llama_adapter_memory_callback,
        user_data: *mut c_void,
    ) -> c_int;
    fn continuum_opt_shadow_state_write_path(
        ctx: *mut sys::llama_context,
        shadow: *const sys::llama_adapter_lora,
        path: *const c_char,
        binding: *const c_char,
    ) -> c_int;
}


/// A fresh shadow's geometry: the targets it adapts and how.
#[derive(Debug, Clone, PartialEq)]
pub struct FreshGeometry<'a> {
    pub rank: u32,
    pub alpha: f32,
    pub targets: &'a [String],
    pub seed: u32,
    /// Adapt only the last `top_layers` blocks; 0 is every block.
    pub top_layers: u32,
}

fn c_path(path: &Path, step: &'static str) -> Result<CString, LearnRefusal> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| LearnRefusal { step, message: format!("a path with a NUL: {}", path.display()) })
}

fn text(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: a NUL-terminated name native code owns for the call.
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

unsafe extern "C" fn adapter_row(device: *const c_char, buffer_type: *const c_char, bytes: usize, host: bool, data: *mut c_void) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `data` is the Vec the synchronous call that passed it owns.
        let rows = unsafe { &mut *data.cast::<Vec<MemoryRow>>() };
        rows.push(MemoryRow { device: text(device), buffer_type: text(buffer_type), role: MemoryRole::Weights, bytes: bytes as u64, host });
    }));
}

unsafe extern "C" fn optimizer_row(
    _shadow: *const sys::llama_adapter_lora,
    device: *const c_char,
    buffer_type: *const c_char,
    kind: *const c_char,
    bytes: usize,
    host: bool,
    data: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: the Measured the synchronous call that passed it owns.
        let measured = unsafe { &mut *data.cast::<Measured>() };
        // SAFETY: `kind` is a NUL-terminated name native code owns for the call, or null.
        let role = (!kind.is_null()).then(|| unsafe { CStr::from_ptr(kind) }).and_then(MemoryRole::of);
        match role {
            Some(role) => measured.rows.push(MemoryRow { device: text(device), buffer_type: text(buffer_type), role, bytes: bytes as u64, host }),
            None => measured.unknown.push(text(kind)),
        }
    }));
}

/// What one optimizer measurement reported: its typed rows, and any role this boundary cannot name.
#[derive(Default)]
struct Measured {
    rows: Vec<MemoryRow>,
    unknown: Vec<String>,
}

impl Measured {
    /// The rows, or a refusal naming the roles nobody can lease against.
    fn typed(self, step: &'static str) -> Result<Vec<MemoryRow>, LearnRefusal> {
        if self.unknown.is_empty() {
            return Ok(self.rows);
        }
        Err(LearnRefusal { step, message: format!("native code measured roles this crate cannot name: {:?}", self.unknown) })
    }
}

impl Model {
    /// What a fresh shadow of `geometry` would hold once loaded, writing and allocating nothing
    /// (native code's dry run of the same writer).
    pub fn measure_fresh_shadow(&self, geometry: &FreshGeometry<'_>) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let mut rows: Vec<MemoryRow> = Vec::new();
        self.fresh_shadow(Path::new(""), geometry, Some(adapter_row), std::ptr::from_mut(&mut rows).cast(), "measure_fresh_shadow")?;
        Ok(rows)
    }

    /// Write a fresh shadow at `path`: it leaves the model's output unchanged until its first
    /// optimizer step.
    pub fn write_fresh_shadow(&self, path: &Path, geometry: &FreshGeometry<'_>) -> Result<(), LearnRefusal> {
        self.fresh_shadow(path, geometry, None, std::ptr::null_mut(), "write_fresh_shadow")
    }

    /// common's one writer: with `measure` it reports each tensor and writes nothing, without it it
    /// writes the file.
    fn fresh_shadow(
        &self,
        path: &Path,
        geometry: &FreshGeometry<'_>,
        measure: sys::llama_adapter_memory_callback,
        rows: *mut c_void,
        step: &'static str,
    ) -> Result<(), LearnRefusal> {
        let path = c_path(path, step)?;
        let targets = CString::new(geometry.targets.join(",")).map_err(|_| LearnRefusal { step, message: "a target with a NUL".into() })?;
        let (Ok(rank), Ok(top_layers)) = (i32::try_from(geometry.rank), i32::try_from(geometry.top_layers)) else {
            return Err(LearnRefusal { step, message: "a rank or layer count past what native code takes".into() });
        };
        let refused = unsafe {
            continuum_lora_write_fresh(self.as_ptr(), path.as_ptr(), rank, geometry.alpha, targets.as_ptr(), geometry.seed, top_layers, measure, rows)
        };
        if refused != 0 {
            return Err(LearnRefusal { step, message: "native code refused the fresh adapter (no target matched this model, or the write failed)".into() });
        }
        Ok(())
    }

    /// What loading the adapter at `path` would hold, allocating nothing.
    pub fn measure_adapter(&self, path: &Path) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let step = "measure_adapter";
        let path = c_path(path, step)?;
        let mut rows: Vec<MemoryRow> = Vec::new();
        let measured = unsafe { sys::llama_adapter_lora_measure(self.as_ptr(), path.as_ptr(), Some(adapter_row), std::ptr::from_mut(&mut rows).cast()) };
        if !measured {
            return Err(LearnRefusal { step, message: "native code could not measure the adapter (it does not fit this model, or could not be read)".into() });
        }
        Ok(rows)
    }
}

impl Learner<'_, '_> {
    /// What `shadow`'s optimizer would hold on this context, allocating nothing.
    pub fn measure_optimizer(&mut self, shadow: &Shadow) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let params = shadow_params(shadow, std::ptr::from_mut(self.holder()).cast());
        let mut measured = Measured::default();
        let ok = unsafe { sys::llama_opt_measure_shadow(self.ctx.as_ptr(), params, Some(optimizer_row), std::ptr::from_mut(&mut measured).cast()) };
        if !ok {
            return Err(self.refusal("measure_shadow"));
        }
        measured.typed("measure_shadow")
    }

    /// What a reply of up to `reply_rows` tokens would hold in the shared workspace, allocating
    /// nothing.
    pub fn measure_workspace(&mut self, shadow: &Shadow, reply_rows: NonZeroU32) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let params = shadow_params(shadow, std::ptr::from_mut(self.holder()).cast());
        let mut measured = Measured::default();
        let ok = unsafe {
            sys::llama_opt_measure_shadow_graph(self.ctx.as_ptr(), params, reply_rows.get(), Some(optimizer_row), std::ptr::from_mut(&mut measured).cast())
        };
        if !ok {
            return Err(self.refusal("measure_shadow_graph"));
        }
        measured.typed("measure_shadow_graph")
    }

    /// Save `adapter` as a generation in `into`: `adapter.gguf` (its weights) and `optimizer.gguf`
    /// (its optimizer state, bound to `binding`). Returns the optimizer steps it had committed.
    pub fn save_generation(&mut self, adapter: &LoraAdapter, into: &Path, binding: &str) -> Result<u64, LearnRefusal> {
        let step = "save_generation";
        std::fs::create_dir_all(into).map_err(|e| LearnRefusal { step, message: format!("{}: {e}", into.display()) })?;
        let weights = c_path(&into.join("adapter.gguf"), step)?;
        if unsafe { sys::llama_adapter_lora_save(adapter.ptr.as_ptr(), weights.as_ptr()) } != 0 {
            return Err(LearnRefusal { step, message: "native code could not save the adapter's weights".into() });
        }
        let optimizer = c_path(&into.join("optimizer.gguf"), step)?;
        let binding = CString::new(binding).map_err(|_| LearnRefusal { step, message: "a binding with a NUL".into() })?;
        if unsafe { continuum_opt_shadow_state_write_path(self.ctx.as_ptr(), adapter.ptr.as_ptr(), optimizer.as_ptr(), binding.as_ptr()) } != 0 {
            return Err(self.refusal("shadow_state_write"));
        }
        let steps = unsafe { sys::llama_opt_shadow_steps(self.ctx.as_ptr(), adapter.ptr.as_ptr()) };
        u64::try_from(steps).map_err(|_| LearnRefusal { step, message: format!("native code counted {steps} steps") })
    }
}
