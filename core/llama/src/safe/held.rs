//! What a context holds, per allocator (UNIFIED-PAGING.md, slice 3b): each buffer type's bytes by what
//! they are, read from the context itself (`llama_context_memory_rows`). The buffer type is a handle, so
//! a reader places a row on the memory it lives in without matching a name.

use std::ffi::CStr;
use std::ptr::NonNull;

use super::Context;
use crate::sys;

/// One allocator a buffer comes from: the physical device it lives on, as the backend names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferType(pub(super) NonNull<sys::ggml_backend_buffer_type>);

// SAFETY: a ggml buffer type is a backend-owned singleton that lives for the process and is only
// read through its query functions, so its pointer may cross threads with the slot that names it.
unsafe impl Send for BufferType {}
unsafe impl Sync for BufferType {}

impl BufferType {
    /// The backend's own name for this buffer type ("CUDA0", "Metal", "CPU", ...).
    pub fn name(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe { CStr::from_ptr(sys::ggml_backend_buft_name(self.0.as_ptr())) }.to_str()
    }

    /// The device's own name ("CUDA0", "MTL0", "CPU", ...), when the buffer type has a device.
    pub fn device(&self) -> Option<Result<&str, std::str::Utf8Error>> {
        let device = unsafe { sys::ggml_backend_buft_get_device(self.0.as_ptr()) };
        if device.is_null() {
            return None;
        }
        let name = unsafe { sys::ggml_backend_dev_name(device) };
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_str())
    }

    /// Whether these bytes are host memory (RAM), not device memory.
    pub fn is_host(&self) -> bool {
        unsafe { sys::ggml_backend_buft_is_host(self.0.as_ptr()) }
    }
}

/// One buffer type's bytes in a context, by what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferHeld {
    pub buffer: BufferType,
    /// The model's weights.
    pub model: u64,
    /// The context's memory: KV, recurrent state, the expert cache.
    pub context: u64,
    /// The scheduler's compute workspace: serving's graphs and a reply's.
    pub compute: u64,
    /// The output buffer (logits, embeddings).
    pub output: u64,
}

impl BufferHeld {
    pub fn total(&self) -> u64 {
        self.model.saturating_add(self.context).saturating_add(self.compute).saturating_add(self.output)
    }
}

/// The context could not account for its buffers (an allocator threw while it was read).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("llama could not account for this context's buffers")]
pub struct Unaccounted;

/// A reading of what a context holds. Its reader keeps one and reads into it again, so a reading
/// allocates nothing past its high-water mark; native code writes its rows straight into it.
#[derive(Default)]
pub struct Held {
    rows: Vec<sys::llama_context_memory_row>,
}

// SAFETY: each row's buffer type is a process-lifetime backend singleton (see `BufferType`); the rows
// are plain counts besides it.
unsafe impl Send for Held {}
unsafe impl Sync for Held {}

impl Held {
    /// Each buffer type's bytes, as last read.
    pub fn buffers(&self) -> impl Iterator<Item = BufferHeld> + '_ {
        self.rows.iter().filter_map(|row| {
            let buffer = NonNull::new(row.buffer_type).map(BufferType)?; // never null: `Context::held` refuses a null row
            Some(BufferHeld { buffer, model: row.model, context: row.context, compute: row.compute, output: row.output })
        })
    }

    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.rows.capacity()
    }
}

impl Context<'_> {
    /// Read every buffer type this context holds bytes in into `held`.
    pub fn held(&self, held: &mut Held) -> Result<(), Unaccounted> {
        loop {
            held.rows.clear();
            let room = held.rows.capacity();
            let n_max = i32::try_from(room).map_err(|_| Unaccounted)?;
            // SAFETY: the context outlives this synchronous query, and `rows` has room for `n_max` rows.
            let n = unsafe { sys::llama_context_memory_rows(self.ptr.as_ptr(), held.rows.as_mut_ptr(), n_max) };
            let n = usize::try_from(n).map_err(|_| Unaccounted)?; // -1: the context could not account
            if n > room {
                held.rows.reserve(n); // the first reading, or a new buffer type: grow once, read again
                continue;
            }
            // SAFETY: native code wrote exactly `n <= room` rows.
            unsafe { held.rows.set_len(n) };
            if held.rows.iter().any(|row| row.buffer_type.is_null()) {
                held.rows.clear();
                return Err(Unaccounted);
            }
            return Ok(());
        }
    }
}
