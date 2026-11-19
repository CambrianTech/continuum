//! A persona's native blocks and token history (the fork's CONTINUUM-ENGINE 4.14 seam,
//! `llama_persona_*`): her state moves out of and back into the engine as raw copies, never
//! serialized, tokenized or re-prefilled, and only under the owner she was made for (the native save
//! stamps it, the native load and bind refuse anyone else). The persona engine's tier adapters call this; nothing here chooses when.

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

/// Why a bind was refused: the blocks are not the expected owner's, or the fork refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindRefusal {
    #[error("these blocks are stamped {held:?}, not the expected {expected:?}")]
    WrongOwner { expected: PersonaOwner, held: Option<PersonaOwner> },
    #[error(transparent)]
    Native(#[from] PersonaRefusal),
}

/// The handle's stamp against the transfer's owner: equal persona AND epoch, or refused. A handle
/// never saved (no stamp) has nothing to bind.
fn owner_matches(held: Option<PersonaOwner>, expected: PersonaOwner) -> Result<(), BindRefusal> {
    match held {
        Some(held) if held == expected => Ok(()),
        held => Err(BindRefusal::WrongOwner { expected, held }),
    }
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
    /// One persona for `ctx`'s memory, made for `owner`: her blocks empty, her token history empty and
    /// sized by nothing. `None` when that memory has no persona move.
    pub fn new(ctx: &mut Context<'_>, owner: PersonaOwner) -> Option<Self> {
        // SAFETY: `ctx` is a live context; the fork returns an owned persona or NULL.
        NonNull::new(unsafe { sys::llama_persona_init(ctx.as_ptr(), owner.persona, owner.epoch) }).map(|ptr| Self { ptr })
    }

    /// Grow her history by `tokens` ids and `receipts` appends, keeping what she holds. Her owner
    /// calls it ahead of need, between acts, never inside one; an append on the hot path only writes.
    pub fn grow(&mut self, tokens: usize, receipts: usize) -> Result<(), PersonaRefusal> {
        // SAFETY: `self.ptr` is live for this handle.
        status(unsafe { sys::llama_persona_grow_tokens(self.ptr.as_ptr(), tokens, receipts) })
    }

    /// Give back the room she does not hold as she cools; nothing she holds is dropped.
    pub fn shrink(&mut self) -> Result<(), PersonaRefusal> {
        // SAFETY: `self.ptr` is live for this handle.
        status(unsafe { sys::llama_persona_shrink_tokens(self.ptr.as_ptr()) })
    }

    /// Move sequence `seq`'s state out of `ctx` into these blocks; her stream is left empty.
    pub fn save(&mut self, ctx: &mut Context<'_>, seq: i32) -> Result<(), PersonaRefusal> {
        // SAFETY: both pointers are live; the fork validates before writing and refuses whole.
        status(unsafe { sys::llama_persona_save(ctx.as_ptr(), seq, self.ptr.as_ptr()) })
    }

    /// Bind these blocks into `ctx` as sequence `seq`, in an empty stream the memory chooses (the
    /// caller never numbers streams); `no_empty_stream` when none is empty. The caller names the
    /// owner the transfer is FOR: blocks stamped for another persona, or for another epoch of her (a
    /// stale generation), are refused before the fork is asked, so a wrong but intact handle never
    /// binds and the destination is untouched (Codex on #6132; the fork checks only that a handle's
    /// blocks match the handle itself).
    pub fn bind(&mut self, ctx: &mut Context<'_>, seq: i32, expected: PersonaOwner) -> Result<(), BindRefusal> {
        owner_matches(self.owner(), expected)?;
        // SAFETY: both pointers are live; the fork validates before writing and refuses whole.
        status(unsafe { sys::llama_persona_bind(ctx.as_ptr(), seq, self.ptr.as_ptr()) }).map_err(BindRefusal::Native)
    }

    /// The owner stamped by the last completed save; `None` before one.
    pub fn owner(&self) -> Option<PersonaOwner> {
        let (mut persona, mut epoch) = (0u64, 0u64);
        // SAFETY: `self.ptr` is live; the out pointers are valid locals.
        unsafe { sys::llama_persona_owner(self.ptr.as_ptr(), &mut persona, &mut epoch) }
            .then_some(PersonaOwner { persona, epoch })
    }

    /// Her token history as held; empty until her first append.
    pub fn tokens(&self) -> &[i32] {
        let mut n = 0u64;
        // SAFETY: the fork returns her history's buffer and its frontier (or NULL and 0); the
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

/// A bound mind's state at one of her positions, taken WITHOUT moving her (the fork's
/// `llama_persona_checkpoint_*`, CONTINUUM-ENGINE 4.15): her recurrent rows are copied out; her
/// attention KV stays in place. [`Self::rewind`] writes the rows back and cuts what she holds past
/// [`Self::pos`], so her next decode continues from the boundary instead of re-prefilling from zero.
/// Its buffers are reused take after take; one checkpoint rewinds any number of times.
pub struct PersonaCheckpoint {
    ptr: NonNull<sys::llama_persona_checkpoint>,
}

// Plain host memory owned by this handle; the engine owner moves it between threads only with the
// context it belongs to.
unsafe impl Send for PersonaCheckpoint {}

impl PersonaCheckpoint {
    /// An empty checkpoint (nothing taken). `None` when the allocation failed.
    pub fn new() -> Option<Self> {
        // SAFETY: the fork returns an owned checkpoint or NULL.
        NonNull::new(unsafe { sys::llama_persona_checkpoint_init() }).map(|ptr| Self { ptr })
    }

    /// Take sequence `seq`'s state at her current position. A refusal leaves no checkpoint (a later
    /// rewind refuses), never a stale one.
    pub fn take(&mut self, ctx: &mut Context<'_>, seq: i32) -> Result<(), PersonaRefusal> {
        // SAFETY: both pointers are live; the fork reserves before copying and refuses whole.
        status(unsafe { sys::llama_persona_checkpoint_take(ctx.as_ptr(), seq, self.ptr.as_ptr()) })
    }

    /// Put sequence `seq` back at [`Self::pos`]: what she holds past it is cut. A refusal (nothing
    /// taken, or a checkpoint ahead of what she holds) leaves her exactly as she was.
    pub fn rewind(&self, ctx: &mut Context<'_>, seq: i32) -> Result<(), PersonaRefusal> {
        // SAFETY: both pointers are live; the fork checks before writing and refuses whole.
        status(unsafe { sys::llama_persona_checkpoint_rewind(ctx.as_ptr(), seq, self.ptr.as_ptr()) })
    }

    /// Her last position the checkpoint holds through; `None` when nothing is taken.
    pub fn pos(&self) -> Option<u32> {
        // SAFETY: `self.ptr` is live for this handle.
        u32::try_from(unsafe { sys::llama_persona_checkpoint_pos(self.ptr.as_ptr()) }).ok()
    }

    /// The bytes it holds (her recurrent rows): what the footprint registry is told.
    pub fn bytes(&self) -> usize {
        // SAFETY: `self.ptr` is live for this handle.
        unsafe { sys::llama_persona_checkpoint_bytes(self.ptr.as_ptr()) }
    }
}

impl Drop for PersonaCheckpoint {
    fn drop(&mut self) {
        // SAFETY: owned since `new`; freed exactly once here.
        unsafe { sys::llama_persona_checkpoint_free(self.ptr.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HER: PersonaOwner = PersonaOwner { persona: 7, epoch: 3 };

    // what this catches (Codex on #6132): two intact handles at one reusable sequence. The bind of
    // the wrong one, or of a stale epoch of the right one, is refused before the fork is asked.
    #[test]
    fn only_the_expected_owner_and_epoch_bind() {
        assert_eq!(owner_matches(Some(HER), HER), Ok(()));
        let other = PersonaOwner { persona: 8, epoch: 3 };
        assert_eq!(owner_matches(Some(other), HER), Err(BindRefusal::WrongOwner { expected: HER, held: Some(other) }));
        let stale = PersonaOwner { persona: 7, epoch: 2 };
        assert_eq!(owner_matches(Some(stale), HER), Err(BindRefusal::WrongOwner { expected: HER, held: Some(stale) }));
        assert_eq!(owner_matches(None, HER), Err(BindRefusal::WrongOwner { expected: HER, held: None }));
    }
}
