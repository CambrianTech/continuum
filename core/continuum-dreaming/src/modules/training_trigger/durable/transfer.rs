//! Carrying a bucket's outstanding examples to another base (`genome/training-trigger/transfer`,
//! card 70c24ee8). A lesson is her lived text, learnable on whatever base she runs; a gene and its
//! generations are bound to the base they were learned on. So what moves is only what is not
//! learned yet, and nothing of the source changes but where its rest is learned (Codex on
//! 9913596d): its id, fitted messages, source base and review stay as written, and the positions
//! it learned stay its own.
//!
//! Each pending source row gets one carried row in the bucket keyed by the target base: its id a
//! digest of (source id, target base), so a repeat is the same row; its examples the source's from
//! its learned position on; its batch otherwise the source's. The source row leaves pending, naming
//! the row its rest went to (`carried_to`), and reclaim leaves it alone. Both writes are ONE atomic
//! batch under both buckets' gates, refused while a seat holds either bucket, so no position is
//! learned, or credited, on both sides. The engine renders a carried lesson with the target's own
//! tokenizer as it renders any (per request), and what the target learns names its origin
//! (`carried_from`).

use super::*;
use crate::orm::store::commit;

/// The rows a pending page holds at most.
const PAGE: usize = 256;

/// What a transfer would carry, or carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Carried {
    /// Pending source rows with examples not yet learned.
    pub(crate) rows: usize,
    /// Their examples not yet learned: what the target learns.
    pub(crate) examples: usize,
}

/// What an apply carried.
#[derive(Debug, Clone)]
pub(crate) struct Transferred {
    pub(crate) to: BucketKey,
    pub(crate) carried: Carried,
}

/// Why a transfer read or carried nothing.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TransferRefused {
    #[error("the trainer's store is unavailable: {0}")]
    Unavailable(String),
    #[error("the trainer's store failed: {0}")]
    Store(String),
    #[error("the carried rows could not be written: {0}")]
    Orm(#[from] crate::orm::store::OrmStoreError),
    #[error("the training submission sequence is exhausted")]
    SequenceExhausted,
    #[error(transparent)]
    Base(#[from] crate::model_registry::base_identity::BaseRefused),
    #[error("the source bucket is already on base '{0}': nothing to carry")]
    SameBase(String),
    #[error("a seat on the node engine holds {0}: transfer once it is let go")]
    Seated(&'static str),
    #[error("the source and target buckets pin different training policies: carrying would train one bucket under two")]
    PolicyDiffers,
    #[error("the source bucket has nothing pending that is not learned: nothing to carry")]
    NothingOutstanding,
}

impl TransferRefused {
    /// The wire's `errorKind`.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "PersistenceUnavailable",
            Self::Store(_) | Self::Orm(_) => "PersistenceFailed",
            Self::SequenceExhausted => "SequenceExhausted",
            Self::Base(_) => "BaseRefused",
            Self::SameBase(_) => "SameBase",
            Self::Seated(_) => "Seated",
            Self::PolicyDiffers => "InconsistentBucket",
            Self::NothingOutstanding => "NothingOutstanding",
        }
    }
}

impl DurableStore {
    /// Every pending row of `key`, whole, oldest first.
    async fn pending_rows(&self, key: &BucketKey) -> Result<Vec<Submission>, TransferRefused> {
        let mut rows = Vec::new();
        let mut after = 0u64;
        loop {
            let page = self.pending_page(key, after, PAGE).await.map_err(TransferRefused::Store)?;
            let full = page.len() == PAGE;
            if let Some(last) = page.last() {
                after = last.sequence;
            }
            rows.extend(page);
            if !full {
                return Ok(rows);
            }
        }
    }
}

/// The examples of `row` not learned yet.
fn outstanding(row: &Submission) -> usize {
    row.batch.examples.len().saturating_sub(row.learned as usize)
}

/// The id of `source`'s carried row on `base`: the same for every transfer of it there.
fn carried_id(source: Uuid, base: &str) -> Uuid {
    Uuid::new_v5(&source, format!("carry:{base}").as_bytes())
}

impl TrainingTriggerState {
    /// The catalog id `to_base` names, and the bucket `from`'s rest would be learned in there.
    fn target_of(&self, from: &BucketKey, to_base: &str) -> Result<BucketKey, TransferRefused> {
        let base = crate::model_registry::base_identity::canonical_base(&self.models_now(), to_base)?;
        if crate::model_registry::artifacts::names_the_same_base(&base, &from.base_model) {
            return Err(TransferRefused::SameBase(base));
        }
        Ok(BucketKey { base_model: base, ..from.clone() })
    }

    /// What carrying `from` to `to_base` would carry; reads and changes nothing.
    pub(crate) async fn transfer_preview(&self, from: &BucketKey, to_base: &str) -> Result<(BucketKey, Carried), TransferRefused> {
        let to = self.target_of(from, to_base)?;
        let store = self.durable.require().map_err(TransferRefused::Unavailable)?;
        let rows = store.pending_rows(from).await?;
        let carried = rows.iter().map(outstanding).filter(|n| *n > 0).fold(Carried::default(), |sum, n| Carried { rows: sum.rows + 1, examples: sum.examples + n });
        Ok((to, carried))
    }

    /// Carry `from`'s outstanding examples to the bucket on `to_base`, atomically, as an owned
    /// operation holding both buckets' gates (module doc).
    pub(crate) async fn transfer_apply(self: &Arc<Self>, from: &BucketKey, to_base: &str) -> Result<Transferred, TransferRefused> {
        self.require_ready().map_err(TransferRefused::Unavailable)?;
        let to = self.target_of(from, to_base)?;
        let from = from.clone(); // owned by the operation it runs as
        // gates in key order, as any two-bucket holder takes them
        let (first, second) = if from < to { (from.clone(), to.clone()) } else { (to.clone(), from.clone()) };
        let first_lease = self.submit_gates.acquire(&first).await;
        let second_lease = self.submit_gates.acquire(&second).await;
        self.run_leased(first_lease, first, super::super::OwnedKind::Transfer, move |state, _| async move {
            let _second = second_lease;
            state.transfer_holding_gates(from, to).await
        })
        .await
        .map_err(TransferRefused::Unavailable)?
    }

    /// [`Self::transfer_apply`], both gates held: refuse a seat on either side, bring the source's
    /// learned positions current, and carry its rest in one batch.
    async fn transfer_holding_gates(self: Arc<Self>, from: BucketKey, to: BucketKey) -> Result<Transferred, TransferRefused> {
        if self.seated_on_engine(&from) {
            return Err(TransferRefused::Seated("the source bucket"));
        }
        if self.seated_on_engine(&to) {
            return Err(TransferRefused::Seated("the target bucket"));
        }
        let store = self.durable.require().map_err(TransferRefused::Unavailable)?;
        // a crash between a commit and its projection would leave a learned position unrecorded:
        // repaired first, so only what is truly not learned is carried
        if let Some(generation) = store.checkpoint_of(&from).await.map_err(TransferRefused::Store)? {
            store.project(&generation).await.map_err(TransferRefused::Store)?;
        }
        if !store.policies_agree(&from, &to, TransferRefused::Store).await? {
            return Err(TransferRefused::PolicyDiffers);
        }
        let mut operations = Vec::new();
        let mut carried = Carried::default();
        let mut newest = None;
        for mut source in store.pending_rows(&from).await? {
            let rest = outstanding(&source);
            if rest == 0 {
                continue;
            }
            let id = entity_id(&source.base).map_err(TransferRefused::Store)?;
            let into = carried_id(id, &to.base_model);
            let mut base = BaseEntity::for_new_record();
            base.id = into.to_string();
            let sequence = self.next_sequence.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1)).map_err(|_| TransferRefused::SequenceExhausted)?;
            let mut batch = source.batch.clone();
            batch.examples = source.batch.examples.iter().skip(source.learned as usize).cloned().collect();
            let row = Submission {
                base,
                sequence,
                persona_id: to.persona_id,
                trait_kind: to.trait_kind.clone(),
                base_model: to.base_model.clone(),
                batch,
                is_pending: true,
                dispatch_id: None,
                retirement_id: None,
                learned: 0,
                carried_to: None,
                carried_from: Some(id),
                carried_offset: source.learned,
            };
            source.is_pending = false;
            source.carried_to = Some(into);
            operations.push(store.submissions.create_op(into, &row)?);
            operations.push(store.submissions.update_op(id, &source)?);
            carried = Carried { rows: carried.rows + 1, examples: carried.examples + rest };
            newest = Some(sequence);
        }
        let Some(newest) = newest else { return Err(TransferRefused::NothingOutstanding) };
        commit(&store.adapter, operations).await?;
        self.note_arrival(newest);
        continuum_base::probe!(class = "training.trigger.transferred", persona = %from.persona_id, trait_kind = %from.trait_kind,
            from = %from.base_model, to = %to.base_model, rows = carried.rows, examples = carried.examples,
            "a bucket's examples not learned yet were carried to the base she runs; what she learned stays on its own");
        Ok(Transferred { to, carried })
    }
}

#[cfg(test)]
mod tests;
