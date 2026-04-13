//! Safe Rust wrapper over llama.cpp FFI.

use std::ffi::{c_void, CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Once;

use crate::sys;

static BACKEND_INIT: Once = Once::new();

/// Initialize the llama backend. Idempotent.
pub fn backend_init() {
    BACKEND_INIT.call_once(|| {
        unsafe { sys::llama_backend_init(); }
    });
}

/// A loaded llama model. Thread-safe (contexts are single-threaded but model is shared).
pub struct Model {
    ptr: NonNull<sys::llama_model>,
}

unsafe impl Send for Model {}
unsafe impl Sync for Model {}

/// Model load parameters.
#[derive(Debug, Clone)]
pub struct ModelParams {
    /// Number of layers to offload to GPU (-1 = all)
    pub n_gpu_layers: i32,
    /// mmap the file (faster load, usually safe)
    pub use_mmap: bool,
}

impl Default for ModelParams {
    fn default() -> Self {
        Self { n_gpu_layers: -1, use_mmap: true }
    }
}

impl Model {
    /// Load a GGUF model from disk.
    pub fn load(path: impl AsRef<Path>, params: ModelParams) -> Result<Self, String> {
        backend_init();

        let path = path.as_ref();
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|e| format!("invalid path: {e}"))?;

        let mut ffi_params = unsafe { sys::llama_model_default_params() };
        ffi_params.n_gpu_layers = params.n_gpu_layers;
        ffi_params.use_mmap = params.use_mmap;

        let raw = unsafe { sys::llama_model_load_from_file(c_path.as_ptr(), ffi_params) };
        let ptr = NonNull::new(raw).ok_or_else(|| {
            format!("failed to load model from {}", path.display())
        })?;

        Ok(Self { ptr })
    }

    /// Vocabulary size.
    pub fn n_vocab(&self) -> i32 {
        unsafe { sys::llama_n_vocab(sys::llama_model_get_vocab(self.ptr.as_ptr())) }
    }

    /// Create an inference context.
    pub fn new_context(&self, params: ContextParams) -> Result<Context, String> {
        let mut ffi = unsafe { sys::llama_context_default_params() };
        ffi.n_ctx = params.n_ctx;
        ffi.n_batch = params.n_batch;

        let raw = unsafe { sys::llama_new_context_with_model(self.ptr.as_ptr(), ffi) };
        let ctx = NonNull::new(raw).ok_or_else(|| "failed to create context".to_string())?;
        Ok(Context { ptr: ctx, _model: std::marker::PhantomData })
    }

    /// Tokenize a string.
    pub fn tokenize(&self, text: &str, add_bos: bool, special: bool) -> Result<Vec<i32>, String> {
        let vocab = unsafe { sys::llama_model_get_vocab(self.ptr.as_ptr()) };
        let c_text = CString::new(text).map_err(|e| format!("invalid text: {e}"))?;
        let text_len = c_text.as_bytes().len() as i32;

        // First call with size 0 to get required size
        let required = unsafe {
            sys::llama_tokenize(
                vocab,
                c_text.as_ptr(),
                text_len,
                std::ptr::null_mut(),
                0,
                add_bos,
                special,
            )
        };
        let size = required.unsigned_abs() as usize;
        let mut tokens = vec![0i32; size];
        let n = unsafe {
            sys::llama_tokenize(
                vocab,
                c_text.as_ptr(),
                text_len,
                tokens.as_mut_ptr(),
                size as i32,
                add_bos,
                special,
            )
        };
        if n < 0 {
            return Err(format!("tokenize failed: {n}"));
        }
        tokens.truncate(n as usize);
        Ok(tokens)
    }

    /// Convert a token to its UTF-8 string representation.
    pub fn token_to_piece(&self, token: i32) -> String {
        let vocab = unsafe { sys::llama_model_get_vocab(self.ptr.as_ptr()) };
        let mut buf = vec![0u8; 128];
        let n = unsafe {
            sys::llama_token_to_piece(
                vocab,
                token,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as i32,
                0,
                false,
            )
        };
        if n < 0 { return String::new(); }
        buf.truncate(n as usize);
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Check if token is end-of-generation.
    pub fn is_eog_token(&self, token: i32) -> bool {
        let vocab = unsafe { sys::llama_model_get_vocab(self.ptr.as_ptr()) };
        unsafe { sys::llama_vocab_is_eog(vocab, token) }
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        unsafe { sys::llama_model_free(self.ptr.as_ptr()); }
    }
}

/// Context parameters.
#[derive(Debug, Clone)]
pub struct ContextParams {
    pub n_ctx: u32,
    pub n_batch: u32,
}

impl Default for ContextParams {
    fn default() -> Self {
        Self { n_ctx: 4096, n_batch: 512 }
    }
}

/// An inference context for a model. Single-threaded.
pub struct Context<'m> {
    ptr: NonNull<sys::llama_context>,
    _model: std::marker::PhantomData<&'m Model>,
}

impl<'m> Context<'m> {
    // TODO: batch creation, decode, get_logits, sampling, LoRA adapter_set/remove
}

impl<'m> Drop for Context<'m> {
    fn drop(&mut self) {
        unsafe { sys::llama_free(self.ptr.as_ptr()); }
    }
}
