//! A shadow's life outside a reply (IN-PROCESS-LEARNING.md, L2): written fresh, measured before any
//! byte of it is allocated, and saved as a generation. Measurement is native code's own: what it
//! reports is what it would allocate, row by row, so the memory owner leases exactly that.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::num::NonZeroU32;
use std::path::Path;

use super::{shadow_params, Learner, LearnRefusal, MemoryRole, MemoryRow, Shadow};
use crate::safe::{LoraAdapter, Model};
use crate::sys;

/// One measured row as the boundary hands it (`learn_boundary.cpp`): `role` is null for an adapter's
/// weights.
#[repr(C)]
struct Row {
    device: *const c_char,
    buffer_type: *const c_char,
    role: *const c_char,
    bytes: usize,
    host: bool,
}

/// Where measured rows go: the one callback and the [`Measured`] it fills.
#[repr(C)]
struct Sink {
    row: unsafe extern "C" fn(row: *const Row, data: *mut c_void),
    data: *mut c_void,
}

/// A fresh shadow's geometry as the boundary takes it.
#[repr(C)]
struct Fresh {
    rank: i32,
    alpha: f32,
    targets: *const c_char,
    seed: u32,
    top_layers: i32,
}

extern "C" {
    fn continuum_lora_write_fresh(model: *const sys::llama_model, path: *const c_char, fresh: *const Fresh, sink: *const Sink) -> c_int;
    fn continuum_adapter_measure(model: *mut sys::llama_model, path: *const c_char, sink: *const Sink) -> bool;
    fn continuum_opt_measure_shadow_graph(ctx: *mut sys::llama_context, params: sys::llama_opt_params, reply_rows: u32, sink: *const Sink) -> bool;
    fn continuum_opt_shadow_state_write_path(ctx: *mut sys::llama_context, shadow: *const sys::llama_adapter_lora, path: *const c_char, binding: *const c_char) -> c_int;
}

/// What a shadow learning a reply would hold, by owner: the optimizer's own state (static for the
/// shadow's life, leased with her seat) and the shared workspace the reply needs while it learns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowMeasure {
    pub optimizer: Vec<MemoryRow>,
    pub workspace: Vec<MemoryRow>,
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

/// What one measurement reported: its typed rows, and any role this boundary cannot name.
#[derive(Default)]
struct Measured {
    rows: Vec<MemoryRow>,
    unknown: Vec<String>,
}

impl Measured {
    /// Measure through `measure`, handed the sink that fills this: the rows; `refused` when native
    /// code refused; or a refusal naming the roles nobody can lease against.
    fn through(step: &'static str, measure: impl FnOnce(*const Sink) -> bool, refused: impl FnOnce() -> LearnRefusal) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let mut measured = Measured::default();
        let sink = Sink { row: measured_row, data: std::ptr::from_mut(&mut measured).cast() };
        if !measure(&sink) {
            return Err(refused());
        }
        if measured.unknown.is_empty() {
            return Ok(measured.rows);
        }
        Err(LearnRefusal { step, message: format!("native code measured roles this crate cannot name: {:?}", measured.unknown) })
    }
}

unsafe extern "C" fn measured_row(row: *const Row, data: *mut c_void) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: the row and the Measured live for the synchronous call that handed them over.
        let (row, measured) = unsafe { (&*row, &mut *data.cast::<Measured>()) };
        let role = if row.role.is_null() { Some(MemoryRole::Weights) } else { MemoryRole::of(unsafe { CStr::from_ptr(row.role) }) };
        match role {
            Some(role) => measured.rows.push(MemoryRow { device: text(row.device), buffer_type: text(row.buffer_type), role, bytes: row.bytes as u64, host: row.host }),
            None => measured.unknown.push(text(row.role)),
        }
    }));
}

impl Model {
    /// What a fresh shadow of `geometry` would hold once loaded, writing and allocating nothing
    /// (native code's dry run of the same writer).
    pub fn measure_fresh_shadow(&self, geometry: &FreshGeometry<'_>) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let step = "measure_fresh_shadow";
        let fresh = FreshText::of(geometry, step)?;
        let empty = c_path(Path::new(""), step)?;
        Measured::through(step, |sink| unsafe { continuum_lora_write_fresh(self.as_ptr(), empty.as_ptr(), &fresh.native(), sink) } == 0, || LearnRefusal { step, message: "native code refused the fresh adapter (no target matched this model)".into() })
    }

    /// Write a fresh shadow at `path`: it leaves the model's output unchanged until its first
    /// optimizer step.
    pub fn write_fresh_shadow(&self, path: &Path, geometry: &FreshGeometry<'_>) -> Result<(), LearnRefusal> {
        let step = "write_fresh_shadow";
        let fresh = FreshText::of(geometry, step)?;
        let path = c_path(path, step)?;
        if unsafe { continuum_lora_write_fresh(self.as_ptr(), path.as_ptr(), &fresh.native(), std::ptr::null()) } != 0 {
            return Err(LearnRefusal { step, message: "native code refused the fresh adapter (no target matched this model, or the write failed)".into() });
        }
        Ok(())
    }

    /// What loading the adapter at `path` would hold, allocating nothing.
    pub fn measure_adapter(&self, path: &Path) -> Result<Vec<MemoryRow>, LearnRefusal> {
        let step = "measure_adapter";
        let path = c_path(path, step)?;
        Measured::through(step, |sink| unsafe { continuum_adapter_measure(self.as_ptr(), path.as_ptr(), sink) }, || LearnRefusal { step, message: "native code could not measure the adapter (it does not fit this model, or could not be read)".into() })
    }
}

/// A fresh geometry's text, kept alive while native code reads it.
struct FreshText {
    targets: CString,
    rank: i32,
    alpha: f32,
    seed: u32,
    top_layers: i32,
}

impl FreshText {
    fn of(geometry: &FreshGeometry<'_>, step: &'static str) -> Result<Self, LearnRefusal> {
        let targets = CString::new(geometry.targets.join(",")).map_err(|_| LearnRefusal { step, message: "a target with a NUL".into() })?;
        let (Ok(rank), Ok(top_layers)) = (i32::try_from(geometry.rank), i32::try_from(geometry.top_layers)) else {
            return Err(LearnRefusal { step, message: "a rank or layer count past what native code takes".into() });
        };
        Ok(Self { targets, rank, alpha: geometry.alpha, seed: geometry.seed, top_layers })
    }

    fn native(&self) -> Fresh {
        Fresh { rank: self.rank, alpha: self.alpha, targets: self.targets.as_ptr(), seed: self.seed, top_layers: self.top_layers }
    }
}

impl Learner<'_, '_> {
    /// What `shadow` learning a reply of up to `reply_rows` tokens would hold on this context, allocating
    /// nothing: its optimizer's own state, and the shared workspace that reply needs. One measure, sized by
    /// the reply, never by the context's window (a window-sized measure reported 5,513,766,976 bytes of
    /// compute on the 0.5B at 2,048 tokens where its lesson needed 451,808,352, 2026-11-24).
    pub fn measure(&mut self, shadow: &Shadow, reply_rows: NonZeroU32) -> Result<ShadowMeasure, LearnRefusal> {
        let params = shadow_params(shadow, std::ptr::from_mut(self.holder()).cast());
        let ctx = self.ctx.as_ptr();
        let rows = Measured::through("measure_shadow_graph", |sink| unsafe { continuum_opt_measure_shadow_graph(ctx, params, reply_rows.get(), sink) }, || self.refusal("measure_shadow_graph"))?;
        let (optimizer, workspace) = rows.into_iter().partition(|row| matches!(row.role, MemoryRole::OptimizerStatic | MemoryRole::OptimizerHost));
        Ok(ShadowMeasure { optimizer, workspace })
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
