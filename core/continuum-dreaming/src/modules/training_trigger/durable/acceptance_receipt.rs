use super::*;



#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_submissions")]
#[entity(index(name = "idx_training_submission_bucket", fields = ["personaId", "traitKind", "baseModel", "isPending", "sequence"]))]
#[entity(index(name = "idx_training_submission_pending", fields = ["isPending", "sequence"]))]
pub(in crate::modules::training_trigger) struct Submission {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    /// Destination-local accepted order, restored from the indexed maximum.
    #[entity(indexed)]
    pub(super) sequence: u64,
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    #[entity(json)]
    pub(super) batch: PendingBatch,
    pub(super) is_pending: bool,
    pub(super) dispatch_id: Option<Uuid>,
    /// Withdrawn before any dispatch by `genome/training-trigger/retire`: the
    /// [`Retirement`] receipt that names why. Such a row is never pending again:
    /// the producer's retry of the same submission id replays as accepted and
    /// appends nothing (see [`TrainingTriggerState::accept`]). Absent on every
    /// row written before the verb existed.
    #[serde(default)]
    pub(super) retirement_id: Option<Uuid>,
    /// How many of `batch.examples`, in order, were learned in place: each counted only when the
    /// engine committed its update with certain effects (card 4114a2c6). The row leaves pending when
    /// every example is. Absent on rows written before in-place learning: none learned.
    #[serde(default)]
    pub(super) learned: u32,
}

/// The journaled withdrawal of bucket-pending submissions: what was pulled back
/// before it could train, and why. One row per `retire` call; the rows it covers
/// point back at it through `Submission::retirement_id`. Written BEFORE the rows
/// flip, so a crash between the two leaves a receipt that names rows a rerun
/// finishes (a rerun of the same selection reports them `already_retired`).
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_retirements")]
#[entity(index(name = "idx_training_retirement_card", fields = ["cardId"]))]
pub(in crate::modules::training_trigger) struct Retirement {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    /// The card whose settled turns were withdrawn, when the selection was by card.
    #[entity(indexed)]
    pub(super) card_id: Option<Uuid>,
    /// The bucket the rows left.
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    /// Every submission this receipt withdrew, in the order they were flipped.
    pub(super) submission_ids: Vec<Uuid>,
    /// Examples those submissions carried: what the next job will NOT train on.
    pub(super) examples: u32,
    pub(super) reason: String,
    /// The caller's peer id, when the command carried one.
    pub(super) retired_by: Option<Uuid>,
    pub(super) retired_at_ms: u64,
}


#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_dispatches")]
#[entity(index(name = "idx_training_dispatch_bucket", fields = ["personaId", "traitKind", "baseModel", "isActive"]))]
#[entity(index(name = "idx_training_dispatch_active", fields = ["isActive", "id"]))]
pub(in crate::modules::training_trigger) struct DispatchIntent {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    pub(super) submission_ids: Vec<Uuid>,
    /// The batch path's dispatch phase, kept opaque: only the migration reads these rows now.
    #[entity(json)]
    pub(super) phase: serde_json::Value,
    pub(super) is_active: bool,
    /// The attempt this intent is in (card 55d4b9e3): minted durably when it enters
    /// `Dispatching`, BEFORE job-create runs, so recovery can ask the selection owner about
    /// exactly this attempt instead of inferring from a board that only learns of a job after
    /// `create_job` returns. `None` = written before attempts existed: its outcome is unknown,
    /// and a stranded one is surfaced as unknown, never auto-retried.
    pub(super) attempt: Option<Uuid>,
}


pub(in crate::modules::training_trigger) struct DurableStore {
    pub(super) submissions: OrmStore<Submission>,
    pub(super) dispatches: OrmStore<DispatchIntent>,
    pub(super) retirements: OrmStore<Retirement>,
    pub(super) policy_corrections: OrmStore<policy::PolicyCorrection>,
    pub(super) learn_checkpoints: OrmStore<super::learn_checkpoint::LearnCheckpoint>,
    pub(super) adapter: Arc<dyn StorageAdapter>,
}

impl DurableStore {
    pub(super) async fn new(adapter: Arc<dyn StorageAdapter>) -> Result<Self, String> {
        Ok(Self {
            submissions: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            dispatches: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            retirements: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            policy_corrections: OrmStore::new(adapter.clone()).await.map_err(|e| e.to_string())?,
            learn_checkpoints: OrmStore::new(adapter.clone()).await.map_err(|e| e.to_string())?,
            adapter,
        })
    }

    pub(super) async fn next_sequence(&self) -> Result<u64, String> {
        let result = self
            .adapter
            .query(
                QueryBuilder::new(Submission::COLLECTION)
                    .sort_desc("sequence")
                    .limit(1)
                    .select(vec!["sequence".into()])
                    .build(),
            )
            .await;
        if !result.success {
            return Err(result
                .error
                .unwrap_or_else(|| "training sequence query refused".into())); // The query explicitly failed; missing diagnostics still return an error.
        }
        let rows = result.data.ok_or("training sequence query omitted rows")?;
        let last = rows
            .first()
            .map(|row| {
                row.data
                    .get("sequence")
                    .and_then(|s| s.as_u64())
                    .ok_or("training submission has invalid sequence")
            })
            .transpose()?
            .unwrap_or(0); // A successful empty table starts at sequence one; malformed stored sequences already return Err above.
        last.checked_add(1)
            .ok_or_else(|| "training submission sequence exhausted".into())
    }

    pub(super) async fn pending_page(
        &self,
        key: &BucketKey,
        after: u64,
        limit: usize,
    ) -> Result<Vec<Submission>, String> {
        let result = self
            .adapter
            .query(
                key.query(Submission::COLLECTION)
                    .filter_eq("isPending", true)
                    .filter("sequence", QueryOperator::Gt(after.into()))
                    .sort_asc("sequence")
                    .limit(limit)
                    .build(),
            )
            .await;
        if !result.success {
            return Err(result
                .error
                .unwrap_or_else(|| "training pending query refused".into())); // The query explicitly failed; this supplies its missing diagnostic, never rows.
        }
        result
            .data
            .ok_or("training pending query omitted rows")?
            .into_iter()
            .map(|row| row.decode().map_err(|error| error.to_string())) // Decode persisted ORM rows into the typed submission owner; no transcript re-encoding.
            .collect()
    }

    /// Every pending submission, in any bucket, whose examples carry this card: the
    /// producer stamps `metadata.cardId` on each example it settles from a card. Pages
    /// the pending index (bounded by what is waiting to train, never by history).
    pub(super) async fn pending_rows_on_card(&self, card: Uuid) -> Result<Vec<Submission>, String> {
        const PAGE: usize = 256;
        let card = card.to_string();
        let mut after = 0u64;
        let mut found = Vec::new();
        loop {
            let result = self
                .adapter
                .query(
                    QueryBuilder::new(Submission::COLLECTION)
                        .filter_eq("isPending", true)
                        .filter("sequence", QueryOperator::Gt(after.into()))
                        .sort_asc("sequence")
                        .limit(PAGE)
                        .build(),
                )
                .await;
            if !result.success {
                return Err(result
                    .error
                    .unwrap_or_else(|| "training pending query refused".into())); // The query explicitly failed; this supplies its missing diagnostic, never rows.
            }
            let rows = result.data.ok_or("training pending query omitted rows")?;
            let page = rows.len();
            for row in rows {
                let row: Submission = row.decode().map_err(|error| error.to_string())?; // Decode persisted ORM rows into the typed submission owner; no transcript re-encoding.
                after = after.max(row.sequence);
                let on_card = row.batch.examples.iter().any(|example| {
                    example
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("cardId"))
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c == card)
                });
                if on_card {
                    found.push(row);
                }
            }
            if page < PAGE {
                return Ok(found);
            }
        }
    }


}

impl Submission {
    pub(super) fn key(&self) -> BucketKey {
        BucketKey {
            persona_id: self.persona_id,
            trait_kind: self.trait_kind.clone(),
            base_model: self.base_model.clone(),
        }
    }
}


impl BucketKey {
    pub(super) fn query(&self, collection: &str) -> QueryBuilder {
        QueryBuilder::new(collection)
            .filter_eq("personaId", self.persona_id.to_string())
            .filter_eq("traitKind", self.trait_kind.clone())
            .filter_eq("baseModel", self.base_model.clone())
    }
}


pub(super) fn entity_id(base: &BaseEntity) -> Result<Uuid, String> {
    Uuid::parse_str(&base.id).map_err(|e| format!("invalid training entity identity: {e}"))
}

impl PendingBatch {
    /// THE fields a bucket pins at first arrival and every later batch must agree on.
    pub(crate) fn policy(&self) -> super::super::BucketPolicy {
        super::super::BucketPolicy {
            source: self.source.clone(),
            lora: self.lora.clone(),
            schedule: self.schedule.clone(),
            validation_split: self.validation_split,
            local_artifact_dir: self.local_artifact_dir.clone(),
            preferred_provider: self.preferred_provider.clone(),
            eval_set: self.eval_set.clone(),
        }
    }

    pub(crate) fn same_policy(&self, other: &Self) -> bool {
        self.policy() == other.policy()
    }

    pub(super) fn same_submission(&self, other: &Self) -> bool {
        self.same_policy(other)
            && self.persona_name == other.persona_name
            && self.examples.len() == other.examples.len()
            && self.examples.iter().zip(&other.examples).all(|(a, b)| {
                a.prompt == b.prompt && a.completion == b.completion && a.metadata == b.metadata
            })
    }
}



/// Her measured verdict surprise when it is BELOW the floor: the room confirms her
/// expectations, memories suffice, and the bucket holds (`Took::Unsurprised`). `None`
/// when she is surprised enough, or not yet judged at all (unknown never halts
/// training), or no mind of hers is resident on this core. One in-memory read of her
/// strip; the same number job-create decides on, read here so a fill that would only
/// come back unsurprised is never dispatched.
pub(super) fn unsurprised(persona: Uuid) -> Option<f32> {
    let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-epoch clock: every tally reads outside its window = not measured
    crate::persona::perception_feed::awareness_of(persona, now_ms)
        .and_then(|a| a.verdict_surprise())
        .map(|v| v.s)
        .filter(|s| *s < crate::genome::competence::SURPRISE_FLOOR)
}
