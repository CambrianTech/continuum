//! Durable attribution for a previously graded gym artifact. This does not grade
//! a solution or manufacture credit: it binds existing evidence before retrying
//! the existing staged-credit transfer owner.

use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct LegacyGradeEvidence {
    #[ts(type = "string")]
    pub room: Uuid,
    #[ts(type = "string")]
    pub card: Uuid,
    #[ts(type = "string")]
    pub owner: Uuid,
    #[ts(type = "string")]
    pub claim: Uuid,
    #[ts(type = "string")]
    pub review_card: Uuid,
    #[ts(type = "string")]
    pub reviewer: Uuid,
    #[ts(type = "string")]
    pub review_claim: Uuid,
    #[ts(type = "string")]
    pub review_post: Uuid,
    pub review_sha256: String,
    pub benchmark: String,
    pub instance: String,
    pub verdict_sha256: String,
    pub artifact_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradedRow {
    pub id: Uuid,
    pub sha256: String,
}

/// Create-only evidence, stored with its maker's existing credit records. The
/// card is the id: a different historical attribution cannot replace a binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "benchmark_credit_attribution")]
pub struct BenchmarkCreditAttribution {
    #[entity(primary_key)]
    pub id: Uuid,
    #[entity(json)]
    pub evidence: LegacyGradeEvidence,
    #[entity(json)]
    pub rows: Vec<GradedRow>,
    pub attested_by: Uuid,
    pub graded_at_ms: u64,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn entity_digest<T: Serialize>(value: &T) -> Result<String, String> {
    // Only this digest is retained in attribution, never another copy of the
    // private staged payload. The verdict and wall use their original raw bytes.
    serde_json::to_vec(value) // Canonical bytes for the persisted attribution's content digest.
        .map(|bytes| digest(&bytes))
        .map_err(|e| e.to_string())
}

pub(crate) fn select_rows(
    evidence: &LegacyGradeEvidence,
    graded_at_ms: u64,
    rows: &[StagedCredit],
) -> Result<Vec<GradedRow>, String> {
    let mut selected = rows
        .iter()
        .filter(|row| {
            row.card_id == evidence.card
                && row.claim_id == Some(evidence.claim)
                && row.owner == Some(evidence.owner)
                && row.staged_at_ms <= graded_at_ms
        })
        .map(|row| {
            Ok(GradedRow {
                id: row.id,
                sha256: row_digest(row)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    selected.sort_by_key(|row| row.id);
    if selected.is_empty() || selected.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err("missing or ambiguous original claimed credit".into());
    }
    Ok(selected)
}

pub(crate) fn row_digest(row: &StagedCredit) -> Result<String, String> {
    entity_digest(row)
}

pub(crate) async fn bound_rows_retained<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    binding: &BenchmarkCreditAttribution,
    mut rows: Vec<StagedCredit>,
) -> Result<Vec<StagedCredit>, ClientError> {
    for expected in &binding.rows {
        let row = rows
            .iter_mut()
            .find(|row| row.id == expected.id)
            .ok_or_else(|| ClientError::Transport("attributed credit is missing".into()))?;
        if row_digest(row).map_err(ClientError::Transport)? == expected.sha256 {
            continue;
        }
        // The only permitted drift is the existing accepted transfer shedding
        // its lived working copy. The durable intent retains the original bytes.
        if row.lived.is_some() || !reviewed::transfer_accepted(conn, persona_name, row.id).await? {
            return Err(ClientError::Transport(
                "unaccepted attributed credit changed".into(),
            ));
        }
        let intent = reviewed::read_one::<_, reviewed::CreditTransferIntent>(
            conn,
            persona_name,
            &row.id.to_string(),
        )
        .await?
        .ok_or_else(|| ClientError::Transport("accepted transfer has no retained intent".into()))?;
        if intent.work_submission_id.is_some() {
            return Err(ClientError::Transport(
                "attributed credit belongs to a different review transfer".into(),
            ));
        }
        let mut restored = row.clone();
        restored.lived = intent.snapshot.lived.clone();
        if row_digest(&intent.snapshot).map_err(ClientError::Transport)? != expected.sha256
            || row_digest(&restored).map_err(ClientError::Transport)? != expected.sha256
        {
            return Err(ClientError::Transport(
                "attributed transfer metadata or payload changed".into(),
            ));
        }
        *row = intent.snapshot;
    }
    bound_rows(binding, rows)
}

pub(crate) fn bound_rows(
    binding: &BenchmarkCreditAttribution,
    rows: Vec<StagedCredit>,
) -> Result<Vec<StagedCredit>, ClientError> {
    let mut selected = Vec::new();
    for expected in &binding.rows {
        let row = rows
            .iter()
            .find(|row| row.id == expected.id)
            .ok_or_else(|| ClientError::Transport("attributed credit is missing".into()))?;
        if row.card_id != binding.id
            || row.claim_id != Some(binding.evidence.claim)
            || row.owner != Some(binding.evidence.owner)
            || row_digest(row).map_err(ClientError::Transport)? != expected.sha256
        {
            return Err(ClientError::Transport("attributed credit changed".into()));
        }
        selected.push(row.clone());
    }
    Ok(selected)
}

pub(crate) async fn persist<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    binding: &BenchmarkCreditAttribution,
) -> Result<(), ClientError> {
    reviewed::ensure_storage(conn, persona_name).await?;
    if let Some(prior) = reviewed::read_one::<_, BenchmarkCreditAttribution>(
        conn,
        persona_name,
        &binding.id.to_string(),
    )
    .await?
    {
        return if prior == *binding {
            Ok(())
        } else {
            Err(ClientError::Transport(
                "grade attribution already has different evidence".into(),
            ))
        };
    }
    let written = reviewed::write_batch(
        conn,
        persona_name,
        vec![reviewed::create(binding.id.to_string(), binding)?],
    )
    .await;
    // A concurrent identical create is a retry; a different winner is refusal.
    let prior = reviewed::read_one::<_, BenchmarkCreditAttribution>(
        conn,
        persona_name,
        &binding.id.to_string(),
    )
    .await?;
    if prior.as_ref() == Some(binding) {
        Ok(())
    } else {
        written.and_then(|_| {
            Err(ClientError::Transport(
                "grade attribution was not retained".into(),
            ))
        })
    }
}
