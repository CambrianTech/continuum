//! A prefill's rate, as a number (Cormac, the 5090's 4c3e window, 2026-11-26: a cold 162,435-token
//! prefill was unfinished 854 s after admission and nothing said how fast it went). One row each time a
//! prefill crosses a mark, and one when it lands: fresh tokens and milliseconds so far, so a slow prefill
//! can be set beside the learning jobs (`learning.job`) that shared the engine thread with it.

use std::time::Duration;

use continuum_persona_engine::persona::PersonaId;

use super::Phase;

/// How many fresh tokens between progress rows: a 162k prefill says ten rows, a short one none.
const MARK: usize = 16_384;

/// After a step of `mind`'s turn that began at `from` (`None`: it decoded) and left her at `phase`, with
/// `shared` tokens of her prompt reused and `spent` prefilling so far.
pub(super) fn observe(mind: PersonaId, from: Option<usize>, phase: &Phase, prompt: usize, shared: usize, spent: Duration) {
    let Some(from) = from else { return };
    let ms = spent.as_millis() as u64;
    match phase {
        Phase::Prefilling { pos } if from / MARK != pos / MARK => {
            let fresh = pos.saturating_sub(shared) as u64;
            continuum_base::probe!(class = "core.prefill.progress", persona = %mind.0, fresh, ms, prompt = prompt as u64,
                "a prefill crossed a mark: its fresh tokens and time so far");
        }
        Phase::Decoding { .. } => {
            let fresh = prompt.saturating_sub(shared) as u64;
            continuum_base::probe!(class = "core.prefill.done", persona = %mind.0, fresh, ms, prompt = prompt as u64,
                "a prefill landed: its fresh tokens and time");
        }
        Phase::Prefilling { .. } | Phase::Waiting => {}
    }
}
