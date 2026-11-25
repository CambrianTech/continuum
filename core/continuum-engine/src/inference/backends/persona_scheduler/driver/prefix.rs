//! Bringing the active mind to her served prefix before a learn step (card 67af8fb2). Native learning
//! begins only when her state ends exactly at the prefix it learns from. Attention cells are cut back
//! natively; recurrent state cannot be, so on a recurrent or hybrid model (the M5's Qwen3.6-35B is
//! mostly Gated Delta Net layers) she is rewound to her deepest checkpoint at or below the prefix and
//! the rest of it is prefilled again under the genes she was served with. A checkpoint at her view's end
//! usually sits exactly there, so the common case prefills nothing.

use continuum_persona_llama::Step;
use llama::Pos;

use super::{Driver, TurnError};

impl Driver<'_, '_> {
    pub(super) fn reposition_to_served_prefix(&mut self, served: usize) -> Result<(), TurnError> {
        if !self.model.has_recurrent_state() {
            return Ok(());
        }
        let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
        let history = &her.parts().tokens;
        if history.held.len() == served {
            return Ok(());
        }
        // never under genes she was not served with: a mind resumed from disk kept only their identity
        let genes = her.parts().genome.served_genes().ok_or(TurnError::GenesNotHeld)?;
        let prefix = history.held.get(..served).ok_or(TurnError::TooLong)?.to_vec();
        let mut pos = match history.deepest_at_or_below(served) {
            Some(end) => {
                self.rewind_to_checkpoint(end)?;
                self.record(|her| her.tokens.cut(end))?;
                end
            }
            // nothing kept below it: her sequence is emptied by its owner, confirmed, then she starts at 0
            // (Codex on #6260: a cut history alone left her native state where it was)
            None => {
                self.engine.clear_active()?;
                self.record(|her| her.tokens.cut(0))?;
                0
            }
        };
        for chunk in prefix[pos..].chunks(self.batch.get()) {
            let at = Pos::try_from(pos).map_err(|_| TurnError::TooLong)?;
            self.engine.submit(Step { tokens: chunk.to_vec(), from: at, logits_at_end: false, genes: std::sync::Arc::clone(&genes) })?;
            self.engine.settle()?;
            self.record(|her| her.tokens.held.extend_from_slice(chunk))?;
            pos += chunk.len();
        }
        Ok(())
    }
}
