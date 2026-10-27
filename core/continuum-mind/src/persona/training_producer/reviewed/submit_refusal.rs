//! A reviewed credit whose training submit was refused, kept until the submit is accepted
//! (card 99b2773a).
//!
//! Kimi's 28087ebb on the 5090 (2026-10-27) was bound, passed by Sahar and its decision
//! persisted at 13:19:04Z, then the training trigger answered without an acceptance. The answer
//! was returned once and kept nowhere, so the credit read `awaiting_acceptance` with no error,
//! and nothing tried again until another review event or her next registration. Here the refusal
//! is kept: its status names why it waits, and staging her next turn asks for one more pass of
//! her reviews while one is held. The submit replays under the same submission id, so a retry
//! the trigger already accepted is `AlreadyAccepted`, never a second batch.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "credit_submit_refusal")]
pub struct CreditSubmitRefusal {
    /// The work submission whose reviewed credit was refused: one row per submission, the latest.
    #[entity(primary_key)]
    pub id: Uuid,
    /// The trigger's answer as it refused (`errorKind`, `error`).
    #[entity(json)]
    pub receipt: SubmitOutcome,
    pub refused_at_ms: u64,
}

fn delete(id: Uuid) -> BatchOperation {
    BatchOperation {
        operation_type: BatchOperationType::Delete,
        collection: CreditSubmitRefusal::COLLECTION.into(),
        id: Some(id.to_string()),
        data: None,
    }
}

/// Keep submission `id`'s refused answer, replacing an earlier one.
pub(super) async fn record_refusal<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    id: Uuid,
    receipt: &SubmitOutcome,
) -> Result<(), ClientError> {
    let row = CreditSubmitRefusal {
        id,
        receipt: receipt.clone(),
        refused_at_ms: continuum_base::time::EpochMs::now().millis(),
    };
    write_batch(conn, persona_name, vec![delete(id), create(id.to_string(), &row)?]).await
}

/// Forget submission `id`'s refusal, in the acceptance's own batch: an accepted credit never
/// keeps one.
pub(super) fn clear_refusal(id: Uuid) -> BatchOperation {
    delete(id)
}

/// Whether a reviewed credit of hers waits on a refused submit. Her staging asks for one more
/// pass of her reviews while one does.
pub async fn refused_credit_waiting<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
) -> Result<bool, ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": CreditSubmitRefusal::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "limit": 1
            }),
        )
        .await?;
    let result: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // Decode the data/list command envelope once at the storage boundary.
    Ok(result.total > 0)
}
