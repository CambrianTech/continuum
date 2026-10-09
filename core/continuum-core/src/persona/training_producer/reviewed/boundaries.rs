//! Durable delivery of accepted review boundaries to the existing dream owner.
//! This receipt means an episode-consolidation boundary finished, never that a
//! training job ran, weights changed, or a candidate earned adoption.

use super::*;

const RECOVERY_PAGE_BUDGET: std::time::Duration = std::time::Duration::from_secs(1);
const RECOVERY_ROW_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "processed_review_boundary")]
pub struct ProcessedReviewBoundary {
    #[entity(primary_key)]
    pub id: Uuid,
    #[entity(foreign_key("credit_review_acceptance.id", on_delete = "restrict"))]
    pub acceptance_id: Uuid,
    pub persona_id: Uuid,
    pub decision_id: Uuid,
    pub review_id: Uuid,
    pub eligible_clusters: u32,
}

/// A per-pass handle to the existing persona store, with the same caller identity
/// as the original acceptance. It owns neither a scheduler nor an inference path.
#[derive(Clone)]
pub struct ReviewBoundaryStore {
    executor: Arc<CommandExecutor>,
    persona_name: String,
    persona_id: Uuid,
}

#[derive(Debug)]
pub struct BoundaryPage {
    pub pending: Vec<Uuid>,
    pub next: Option<String>,
}

impl ReviewBoundaryStore {
    pub(crate) fn new(
        executor: Arc<CommandExecutor>,
        persona_name: String,
        persona_id: Uuid,
    ) -> Self {
        Self {
            executor,
            persona_name,
            persona_id,
        }
    }

    pub(crate) fn resident(persona_id: Uuid) -> Result<Self, ClientError> {
        let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .and_then(|registry| registry.get(persona_id))
            .ok_or_else(|| {
                ClientError::Transport("review boundary persona is not resident".into())
            })?;
        let executor = EXECUTOR.cloned().ok_or_else(|| {
            ClientError::Transport("review boundary executor is not ready".into())
        })?;
        Ok(Self::new(executor, runtime.agent_name().into(), persona_id))
    }

    fn connection(&self) -> Connection<InProcessTransport> {
        Connection::new(InProcessTransport::new(
            self.executor.clone(),
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(self.persona_id),
            )),
        ))
    }

    pub(crate) async fn prepare(&self) -> Result<(), ClientError> {
        ensure_storage(&self.connection(), &self.persona_name).await
    }

    /// One indexed page; IDs are immutable. Wrap after the end so an acceptance
    /// inserted behind the cursor during a missed event is eventually observed.
    pub(crate) async fn page(
        &self,
        after: Option<String>,
        limit: usize,
    ) -> Result<BoundaryPage, ClientError> {
        let conn = self.connection();
        let value = conn
            .commands()
            .execute_value(
                "data/list",
                json!({
                    "collection": CreditReviewAcceptance::COLLECTION,
                    "dbPath": format!("@persona:{}", self.persona_name),
                    "filter": after.as_ref().map(|id| json!({"id": {"$gt": id}})),
                    "sort": [{"field": "id", "direction": "asc"}], "limit": limit,
                }),
            )
            .await?;
        let result: crate::modules::data::DataListResult = serde_json::from_value(value)?; // Decode the ORM command envelope at this storage boundary.
        let mut pending = Vec::new();
        let mut last = None;
        let count = result.items.len();
        let deadline = tokio::time::Instant::now() + RECOVERY_PAGE_BUDGET;
        for value in result.items {
            if tokio::time::Instant::now() >= deadline {
                // Preserve progress within the page; later governor ticks resume.
                return Ok(BoundaryPage {
                    pending,
                    next: last.or(after),
                });
            }
            let record: crate::orm::types::DataRecord = serde_json::from_value(value)?; // Decode the ORM record envelope before its entity.
            let key = record.id.to_string();
            let checked = async {
                let accepted: CreditReviewAcceptance = serde_json::from_value(record.data)?; // Decode the immutable acceptance at the persistence boundary.
                if accepted.id.to_string() != key {
                    return Err(ClientError::Transport(
                        "acceptance key differs from its identity".into(),
                    ));
                }
                let accepted =
                    validate_acceptance(&conn, &self.persona_name, self.persona_id, accepted.id)
                        .await?;
                let processed =
                    processed_acceptance(&conn, &self.persona_name, self.persona_id, &accepted)
                        .await?;
                Ok::<_, ClientError>((accepted.id, processed))
            };
            // A malformed or slow row remains retained and is revisited after
            // cursor wrap, but cannot starve unrelated accepted IDs behind it.
            match tokio::time::timeout(RECOVERY_ROW_BUDGET, checked).await {
                Ok(Ok((id, false))) => pending.push(id),
                Ok(Ok((_, true))) => {}
                other => {
                    crate::probe!(class = "dream.review_recovery_row_deferred", persona = %self.persona_id,
                        acceptance = %key, error = ?other, "retained row deferred; scan continues and retries on wrap");
                }
            }
            last = Some(key);
        }
        Ok(BoundaryPage {
            pending,
            next: if count < limit { None } else { last },
        })
    }

    pub(crate) async fn finish(
        &self,
        ids: &[Uuid],
        eligible_clusters: usize,
    ) -> Result<(), ClientError> {
        let conn = self.connection();
        for id in ids {
            let accepted =
                validate_acceptance(&conn, &self.persona_name, self.persona_id, *id).await?;
            if processed_acceptance(&conn, &self.persona_name, self.persona_id, &accepted).await? {
                continue;
            }
            let receipt = ProcessedReviewBoundary {
                id: *id,
                acceptance_id: *id,
                persona_id: self.persona_id,
                decision_id: accepted.decision_id,
                review_id: accepted.review_id,
                eligible_clusters: eligible_clusters as u32,
            };
            if let Err(error) = write_batch(
                &conn,
                &self.persona_name,
                vec![create(id.to_string(), &receipt)?],
            )
            .await
            {
                // Lost acknowledgement or a racing replay must read the immutable
                // receipt back; a genuine failure leaves the boundary pending.
                if !processed_acceptance(&conn, &self.persona_name, self.persona_id, &accepted)
                    .await?
                {
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}

async fn validate_acceptance<T: Transport>(
    conn: &Connection<T>,
    name: &str,
    persona: Uuid,
    id: Uuid,
) -> Result<CreditReviewAcceptance, ClientError> {
    let key = id.to_string();
    let binding = read_one::<_, WorkCreditBinding>(conn, name, &key)
        .await?
        .ok_or_else(|| ClientError::Transport("review boundary has no binding".into()))?;
    let status = credit_status(conn, name, id).await?;
    if binding.persona_id != persona
        || binding.selection.submission_id != id
        || status.state != ReviewedCreditState::Accepted
    {
        return Err(ClientError::Transport(
            "review boundary acceptance identity differs".into(),
        ));
    }
    read_one::<_, CreditReviewAcceptance>(conn, name, &key)
        .await?
        .ok_or_else(|| ClientError::Transport("review boundary acceptance disappeared".into()))
}

pub(super) async fn is_processed<T: Transport>(
    conn: &Connection<T>,
    name: &str,
    persona: Uuid,
    id: Uuid,
) -> Result<bool, ClientError> {
    let accepted = validate_acceptance(conn, name, persona, id).await?;
    processed_acceptance(conn, name, persona, &accepted).await
}

async fn processed_acceptance<T: Transport>(
    conn: &Connection<T>,
    name: &str,
    persona: Uuid,
    accepted: &CreditReviewAcceptance,
) -> Result<bool, ClientError> {
    let Some(receipt) =
        read_one::<_, ProcessedReviewBoundary>(conn, name, &accepted.id.to_string()).await?
    else {
        return Ok(false);
    };
    if receipt.id != accepted.id
        || receipt.acceptance_id != accepted.id
        || receipt.persona_id != persona
        || receipt.decision_id != accepted.decision_id
        || receipt.review_id != accepted.review_id
    {
        return Err(ClientError::Transport(
            "processed review boundary identity differs".into(),
        ));
    }
    Ok(true)
}
