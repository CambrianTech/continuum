//! One mind's files across a restart (UNIFIED-PAGING slice 2, custody by rename): her record, her KV file
//! and her token history, moved under her custody lock. A restore checks everything it can before her KV
//! file moves (her token history's exact length here; her record's owner, epoch, generation, layout and
//! storage bytes in the fork, read over her file mapped where it lies), then renames it into this core's
//! directory, so a refused restore leaves the last generation whole and a crash after the move leaves her
//! cold and named. A stop writes her history, then hands her blocks over: her record made durable, her
//! file synced, unmapped and renamed into the preserved directory. Only once every mind's files are
//! durable is the manifest naming them published (`Preserved::publish`).

use std::io;
use std::path::PathBuf;

use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_engine::residency::TierRefusal;
use continuum_persona_llama::{Blocks, HandOverRefusal, UnifiedTier};

use super::{Generation, Preserved, PreservedMind, Tokens};

/// Bytes of one token in her history file: a raw `i32`.
const TOKEN_BYTES: usize = std::mem::size_of::<i32>();

/// Why a preserved mind did not come back. Each leaves her files where the manifest names them,
/// except a refusal after her KV file moved (none today: the move is the last step).
#[derive(Debug, thiserror::Error)]
pub(in crate::inference::backends::persona_scheduler) enum RestoreRefusal {
    #[error("another core holds her custody: {0}")]
    Custody(io::Error),
    #[error("her token history {}: {source}", path.display())]
    Tokens { path: PathBuf, source: io::Error },
    #[error("her token history {} holds {found} bytes, the manifest's {count} tokens take {expected}", path.display())]
    TokenLength { path: PathBuf, found: u64, count: u64, expected: u64 },
    #[error(transparent)]
    Tier(#[from] TierRefusal),
}

/// Why a stop did not hand a mind over. Her entry from the last complete generation stands.
#[derive(Debug, thiserror::Error)]
pub(in crate::inference::backends::persona_scheduler) enum HandOverError {
    #[error("another core holds her custody: {0}")]
    Custody(io::Error),
    #[error("her token history was not written: {0}")]
    Tokens(io::Error),
    #[error(transparent)]
    Blocks(#[from] HandOverRefusal),
}

/// One mind as a stop hands her over: her identity, her blocks and her history, which travel together.
pub(in crate::inference::backends::persona_scheduler) struct Leaving<'a> {
    pub persona: uuid::Uuid,
    pub epoch: u64,
    pub basis: String,
    pub blocks: Blocks,
    pub held: &'a [i32],
}

impl Preserved {
    /// Hold `persona`'s custody for the rest of this process: an OS lock beside her files, taken without
    /// waiting, so a core that finds it held leaves her files alone. Held already is held.
    fn take_custody(&mut self, persona: uuid::Uuid) -> io::Result<()> {
        if self.custody.contains_key(&persona) {
            return Ok(());
        }
        let lock = continuum_host::durable::lock_path(&self.dir.join(format!("{persona}.custody")), false)?;
        self.custody.insert(persona, lock);
        Ok(())
    }

    /// Her blocks and token history back from the files `mind` names, under her custody.
    pub(in crate::inference::backends::persona_scheduler) fn restore(&mut self, tier: &mut UnifiedTier<'_, '_>, mind: &PreservedMind) -> Result<(Blocks, Vec<i32>), RestoreRefusal> {
        self.take_custody(mind.persona).map_err(RestoreRefusal::Custody)?;
        let held = match &mind.tokens {
            Tokens::File { name, count } => self.read_tokens(name, *count)?,
            Tokens::Migrating(held) => held.clone(),
        };
        let id = MindId { persona: PersonaId(mind.persona), epoch: Epoch(mind.epoch) };
        let kv = mind.kv.as_deref().map(|name| self.path(name));
        let blocks = tier.restore(id, &self.path(&mind.record), kv.as_deref(), mind.generation)?;
        Ok((blocks, held))
    }

    /// Her history from `name`: exactly `count` raw tokens, refused by the file's length before any byte is read,
    /// then read straight into her token buffer (one read, no staging copy, no per-token decode).
    fn read_tokens(&self, name: &str, count: u64) -> Result<Vec<i32>, RestoreRefusal> {
        use std::io::Read;
        let path = self.path(name);
        let refused = |source| RestoreRefusal::Tokens { path: path.clone(), source };
        let mut file = std::fs::File::open(&path).map_err(refused)?;
        let found = file.metadata().map_err(refused)?.len();
        let expected = count.saturating_mul(TOKEN_BYTES as u64);
        if found != expected {
            return Err(RestoreRefusal::TokenLength { path, found, count, expected });
        }
        let mut held = vec![0_i32; count as usize];
        file.read_exact(bytemuck::cast_slice_mut(&mut held)).map_err(refused)?;
        Ok(held)
    }

    /// Hand one mind over in `generation`: her history, then her blocks (record durable, KV file synced,
    /// unmapped and renamed), all under her custody. Returns her manifest entry, published with the stop's.
    pub(in crate::inference::backends::persona_scheduler) fn hand_over(&mut self, her: Leaving<'_>, generation: Generation) -> Result<PreservedMind, HandOverError> {
        let Leaving { persona, epoch, basis, blocks, held } = her;
        self.take_custody(persona).map_err(HandOverError::Custody)?;
        let tokens = generation.tokens(persona);
        // her history's own bytes, written as they lie in memory: no per-token encode, no staging copy
        continuum_host::durable::write_replace(&self.path(&tokens), bytemuck::cast_slice(held)).map_err(HandOverError::Tokens)?;
        let (record, kv) = (generation.record(persona), generation.kv(persona));
        let in_kv = blocks.hand_over(&self.path(&record), &self.path(&kv), generation.number())?;
        Ok(PreservedMind {
            persona,
            epoch,
            basis,
            generation: generation.number(),
            record,
            kv: in_kv.then_some(kv),
            tokens: Tokens::File { name: tokens, count: held.len() as u64 },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex, 2026-11-25: two contenders on each side of each rename): two cores both
    // moving one mind's files, a parallel copy restoring her while another runs her, or a handoff's new
    // core taking her before the old one handed her over. One holds her custody; the other is refused
    // until the holder ends, and then takes it.
    #[test]
    fn one_core_holds_a_minds_custody_and_another_is_refused_until_it_ends() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let her = uuid::Uuid::from_u128(9);
        let mut first = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        let mut second = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        first.take_custody(her).expect("test: the first core takes her"); // expect: the test's own precondition
        first.take_custody(her).expect("test: held already is held"); // expect: the test's own precondition
        let refused = second.take_custody(her).expect_err("test: the second core is refused"); // expect: the test's own precondition
        assert_eq!(refused.kind(), io::ErrorKind::WouldBlock, "{refused}");
        second.take_custody(uuid::Uuid::from_u128(10)).expect("test: another mind is free"); // expect: the test's own precondition
        drop(first); // the holding core ends (its process exits): the OS releases her lock
        second.take_custody(her).expect("test: then the second core takes her"); // expect: the test's own precondition
    }

    // what this catches: a torn or foreign token history taken as hers. Its length must be exactly the
    // manifest's count of raw tokens, checked before her KV file moves; a whole one reads back as written.
    #[test]
    fn a_token_history_of_another_length_is_refused_and_a_whole_one_reads_back() {
        let dir = tempfile::tempdir().expect("test: a temp dir"); // expect: the test's own precondition
        let preserved = Preserved::at(dir.path().to_path_buf()).expect("test: a store"); // expect: the test's own precondition
        let held: [i32; 3] = [151_644, -1, 7];
        std::fs::write(preserved.path("her.tok"), bytemuck::cast_slice(&held)).expect("test: write"); // expect: the test's own precondition
        assert_eq!(preserved.read_tokens("her.tok", 3).expect("test: whole"), held.to_vec()); // expect: the test's own precondition
        let torn = preserved.read_tokens("her.tok", 4).expect_err("test: torn is refused"); // expect: the test's own precondition
        assert!(matches!(torn, RestoreRefusal::TokenLength { found: 12, count: 4, expected: 16, .. }), "{torn}");
    }
}
