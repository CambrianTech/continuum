//! A persona's native blocks and token history (the fork's CONTINUUM-ENGINE 4.14 seam,
//! `llama_persona_*`): her state moves out of and back into the engine as raw copies, never
//! serialized, tokenized or re-prefilled, and only under the owner her token history was reserved
//! for. The persona engine's tier adapters call this; nothing here chooses when.

use std::ptr::NonNull;

use super::Context;
use crate::sys;

/// Whose blocks these are: the persona and the epoch of her token history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonaOwner {
    pub persona: u64,
    pub epoch: u64,
}

/// A refusal from the fork, by its status name (`llama_persona_status_name`). Every refusal
/// leaves both sides as they were.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("persona move refused: {name}")]
pub struct PersonaRefusal {
    pub status: i32,
    pub name: String,
}

fn status(code: i32) -> Result<(), PersonaRefusal> {
    if code == 0 {
        return Ok(());
    }
    // SAFETY: the fork returns a static, NUL-terminated name for every status, unknown included.
    let name = unsafe { std::ffi::CStr::from_ptr(sys::llama_persona_status_name(code)) }
        .to_string_lossy()
        .into_owned();
    Err(PersonaRefusal { status: code, name })
}

/// One persona's blocks and token history, owned once (freed with her).
pub struct PersonaBlocks {
    ptr: NonNull<sys::llama_persona>,
}

// The blocks are plain host memory owned by this handle; the engine owner moves them between
// threads only with the context they belong to.
unsafe impl Send for PersonaBlocks {}

impl PersonaBlocks {
    /// Empty blocks prepared for `ctx`'s memory; `None` when that memory has no persona move.
    pub fn new(ctx: &mut Context<'_>) -> Option<Self> {
        // SAFETY: `ctx` is a live context; the fork returns an owned persona or NULL.
        NonNull::new(unsafe { sys::llama_persona_init(ctx.as_ptr()) }).map(|ptr| Self { ptr })
    }

    /// Own her token history, once, for `owner`. Sized by nothing: it starts empty and her owner
    /// grows it ahead of need ([`Self::grow`]). A second reserve is refused (`already_reserved`).
    pub fn reserve(&mut self, owner: PersonaOwner) -> Result<(), PersonaRefusal> {
        // SAFETY: `self.ptr` is live for this handle.
        status(unsafe { sys::llama_persona_reserve_tokens(self.ptr.as_ptr(), owner.persona, owner.epoch) })
    }

    /// Grow her history by `tokens` ids and `receipts` appends, keeping what she holds. Her owner
    /// calls it between acts, never inside one; an append on the hot path only writes.
    pub fn grow(&mut self, tokens: usize, receipts: usize) -> Result<(), PersonaRefusal> {
        // SAFETY: `self.ptr` is live for this handle.
        status(unsafe { sys::llama_persona_grow_tokens(self.ptr.as_ptr(), tokens, receipts) })
    }

    /// Move sequence `seq`'s state out of `ctx` into these blocks; her stream is left empty.
    pub fn save(&mut self, ctx: &mut Context<'_>, seq: i32) -> Result<(), PersonaRefusal> {
        // SAFETY: both pointers are live; the fork validates before writing and refuses whole.
        status(unsafe { sys::llama_persona_save(ctx.as_ptr(), seq, self.ptr.as_ptr()) })
    }

    /// Bind these blocks into `ctx` as sequence `seq`, in an empty stream the memory chooses (the
    /// caller never numbers streams); `no_empty_stream` when none is empty.
    pub fn bind(&mut self, ctx: &mut Context<'_>, seq: i32) -> Result<(), PersonaRefusal> {
        // SAFETY: both pointers are live; the fork validates before writing and refuses whole.
        status(unsafe { sys::llama_persona_bind(ctx.as_ptr(), seq, self.ptr.as_ptr()) })
    }

    /// The owner stamped by the last completed save; `None` before one.
    pub fn owner(&self) -> Option<PersonaOwner> {
        let (mut persona, mut epoch) = (0u64, 0u64);
        // SAFETY: `self.ptr` is live; the out pointers are valid locals.
        unsafe { sys::llama_persona_owner(self.ptr.as_ptr(), &mut persona, &mut epoch) }
            .then_some(PersonaOwner { persona, epoch })
    }

    /// Her token history as held; empty before a reserve.
    pub fn tokens(&self) -> &[i32] {
        let mut n = 0u64;
        // SAFETY: the fork returns her reserved buffer and its frontier (or NULL and 0); the
        // buffer lives as long as this handle and only an append through `&mut self` changes it.
        let ptr = unsafe { sys::llama_persona_token_ids(self.ptr.as_ptr(), &mut n) };
        match (NonNull::new(ptr as *mut i32), usize::try_from(n)) {
            (Some(ptr), Ok(len)) => unsafe { std::slice::from_raw_parts(ptr.as_ptr(), len) },
            _ => &[],
        }
    }
}

impl Drop for PersonaBlocks {
    fn drop(&mut self) {
        // SAFETY: owned since `new`; freed exactly once here.
        unsafe { sys::llama_persona_free(self.ptr.as_ptr()) }
    }
}
