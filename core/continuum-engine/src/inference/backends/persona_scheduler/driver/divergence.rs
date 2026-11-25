//! Where her new prompt left what she holds, in words. A context is a FIFO (CONTINUUM-ENGINE.md, "the
//! governing law"): the head never changes, the middle only appends, only the tail may change. When a
//! prompt diverges inside what she holds, everything after that point is prefilled again (the M5,
//! 2026-11-25 22:00 to 22:10Z: 137k of 493k resident prompt tokens, 27% of the Core, rewritten 3 to 12k
//! tokens before the previous act's end). One probe a turn names what changed there, so the codec's
//! act-boundary rewrite (plan S1) is read from the live node, never guessed.
//! cold: once a turn, at her bind, never per step.

use continuum_persona_engine::persona::PersonaId;
use llama::Model;

/// The words read on each side of the divergence: enough to name the item that changed.
const AROUND: usize = 96;

/// Probe a divergence inside what she holds; a prompt that extends all of it (an append) is silent.
pub(super) fn probe(model: &Model, persona: PersonaId, held: &[i32], prompt: &[i32], shared: usize) {
    if shared >= held.len() {
        return;
    }
    let words = |tokens: Option<&[i32]>| tokens.unwrap_or(&[]).iter().map(|&token| model.token_to_piece(token)).collect::<String>(); // unwrap_or: a range past the end reads as nothing
    let before = words(held.get(shared.saturating_sub(AROUND)..shared));
    let held_after = words(held.get(shared..held.len().min(shared + AROUND)));
    let prompt_after = words(prompt.get(shared..prompt.len().min(shared + AROUND)));
    continuum_base::probe!(
        class = "persona.engine.diverged",
        persona = %persona.0,
        held_tokens = held.len() as u64,
        prompt_tokens = prompt.len() as u64,
        shared_tokens = shared as u64,
        rewritten_tokens = (held.len() - shared) as u64,
        before = ?before,
        held_after = ?held_after,
        prompt_after = ?prompt_after,
        "her prompt diverged inside what she holds: everything after this point is prefilled again"
    );
}
