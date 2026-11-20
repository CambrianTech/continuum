use super::{reuse, Basis, Footprint, Held};

/// The tokens her blocks hold, and her checkpoint at her last view's end while what she holds still
/// runs through it.
#[derive(Default)]
pub(super) struct History {
    pub(super) held: Vec<i32>,
    /// View end, latest learned prefix, and earliest still-valid learned prefix.
    kept: [Option<KeptCheckpoint>; 3],
}

/// Her checkpoint and what makes it valid: she held exactly `held[..end]` under `basis` when it was
/// taken, and nothing she holds below `end` has changed since (a cut below it drops it).
struct KeptCheckpoint {
    blocks: llama::PersonaCheckpoint,
    end: usize,
    basis: Basis,
}

/// Her checkpoint holds her recurrent rows in host memory.
impl Footprint for KeptCheckpoint {
    fn held(&self) -> Held {
        Held::host(self.blocks.bytes() as u64)
    }
}

/// Her token history and her checkpoint. Her blocks in the context are reported by the tier that
/// binds them; [`Bound`] is that child's view.
impl Footprint for History {
    fn held(&self) -> Held {
        self.kept
            .iter()
            .fold(self.held.held(), |bytes, kept| bytes + kept.held())
    }
}

impl History {
    /// Keep only her first `len` tokens; a cut below her checkpoint's end drops the checkpoint.
    pub(super) fn cut(&mut self, len: usize) {
        self.held.truncate(len);
        for slot in &mut self.kept {
            if slot.as_ref().is_some_and(|kept| kept.end > len) {
                *slot = None;
            }
        }
    }

    /// Where `prompt` under `basis` would start for her, whose blocks were computed under `under`
    /// (`reuse::choose`), deciding nothing and moving nothing: the bind uses it to resume her, the
    /// frame to size her turn's prefill.
    pub(super) fn reuse(&self, under: &Basis, prompt: &[i32], basis: &Basis) -> reuse::Reuse {
        let kept = self.kept.iter().flatten().map(|kept| reuse::Kept {
            end: kept.end,
            basis: &kept.basis,
        });
        reuse::choose(
            reuse::History {
                basis: under,
                tokens: &self.held,
            },
            kept,
            reuse::History {
                basis,
                tokens: prompt,
            },
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum Boundary {
    View,
    Learned,
}

pub(super) struct Boundaries {
    pub(super) view: Option<usize>,
    pub(super) learned: Option<usize>,
}
impl Boundaries {
    fn entries(&self) -> impl Iterator<Item = (Boundary, usize)> {
        [
            (Boundary::View, self.view),
            (Boundary::Learned, self.learned),
        ]
        .into_iter()
        .filter_map(|(kind, end)| Some((kind, end?)))
    }
    pub(super) fn next_end(&self, pos: usize, prompt_end: usize) -> usize {
        self.entries()
            .map(|(_, end)| end)
            .filter(|end| *end > pos)
            .min()
            .unwrap_or(prompt_end) // unwrap_or: the prompt ends the final frame
    }
    pub(super) fn at(&self, end: usize) -> Option<Boundary> {
        self.entries()
            .find_map(|(kind, pos)| (pos == end).then_some(kind))
    }
}
impl From<Vec<i32>> for History {
    fn from(held: Vec<i32>) -> Self {
        Self {
            held,
            ..Self::default()
        }
    }
}

impl History {
    pub(super) fn deepest(&self) -> Option<usize> {
        self.kept.iter().flatten().map(|s| s.end).max()
    }
    pub(super) fn take_at(&mut self, end: usize) -> Option<Snapshot> {
        let (slot, entry) = self
            .kept
            .iter_mut()
            .enumerate()
            .find(|(_, entry)| entry.as_ref().is_some_and(|s| s.end == end))?;
        Some(Snapshot {
            slot,
            checkpoint: entry.take()?,
        })
    }
    pub(super) fn put_back(&mut self, snapshot: Snapshot) {
        self.kept[snapshot.slot] = Some(snapshot.checkpoint);
    }
    fn slot(&mut self, kind: Boundary) -> &mut Option<KeptCheckpoint> {
        let [view, learned, _] = &mut self.kept;
        match kind {
            Boundary::View => view,
            Boundary::Learned => learned,
        }
    }
    pub(super) fn capture_buffer(
        &mut self,
        kind: Boundary,
        end: usize,
    ) -> Option<llama::PersonaCheckpoint> {
        if let Boundary::Learned = kind {
            let [_, latest, earliest] = &mut self.kept;
            if latest.as_ref().is_some_and(|prior| {
                prior.end < end && earliest.as_ref().is_none_or(|s| prior.end < s.end)
            }) {
                std::mem::swap(latest, earliest);
            }
        }
        self.slot(kind)
            .take()
            .map(|s| s.blocks)
            .or_else(llama::PersonaCheckpoint::new)
    }
    pub(super) fn save(
        &mut self,
        kind: Boundary,
        blocks: llama::PersonaCheckpoint,
        end: usize,
        basis: &Basis,
    ) {
        *self.slot(kind) = Some(KeptCheckpoint {
            blocks,
            end,
            basis: basis.clone(),
        });
    }
}
pub(super) struct Snapshot {
    slot: usize,
    checkpoint: KeptCheckpoint,
}
impl Snapshot {
    pub(super) fn blocks(&self) -> &llama::PersonaCheckpoint {
        &self.checkpoint.blocks
    }
}
