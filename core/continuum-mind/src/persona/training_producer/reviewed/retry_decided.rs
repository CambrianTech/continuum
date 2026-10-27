//! A decided credit settled again from what is already stored, with no board, review event,
//! registration or turn (card 99b2773a).
//!
//! Once a review's decision is persisted the credit is owed a training submit. When the trigger
//! refused it (bucket recovery, a held bucket at its bound) nothing asked again: registration
//! walks only the cards she still owns and drops an `AwaitingAcceptance` replay without a
//! word. Here the credit is settled from its immutable binding and its recorded decision, the
//! review it was decided on, under the same submission id, so a submit the trigger already
//! holds replays as `AlreadyAccepted`, never a second batch.

use super::*;

/// Settle submission `id`'s decided credit again from its binding and its recorded decision.
pub async fn retry_decided<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    id: Uuid,
) -> Result<ReviewedCredit, CreditBindingError> {
    let Some(binding) = read_one::<_, WorkCreditBinding>(conn, persona_name, &id.to_string()).await? else {
        return Ok(ReviewedCredit::pending(ReviewedCreditState::Unbound));
    };
    if binding.persona_id != persona_id {
        return Err(CreditBindingError::WrongSelection);
    }
    let Some(decision) = read_one::<_, CreditReviewDecision>(conn, persona_name, &id.to_string()).await? else {
        return Ok(ReviewedCredit::pending(ReviewedCreditState::AwaitingReview));
    };
    let mut result = credit_status(conn, persona_name, id).await?;
    if result.state == ReviewedCreditState::Accepted {
        return Ok(result);
    }
    result.state = ReviewedCreditState::AwaitingAcceptance;
    let review_id = decision.review.review_id;
    settle_passed(conn, persona_name, persona_id, id, &binding, &decision.review, review_id, result).await
}

/// One bounded page of her decision ids, in id order after `after`, read from each row's key
/// whatever its data holds, so a malformed row is still a step past it (Codex on #5531). The
/// flag says the page was full: more may follow.
pub async fn decided_page<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    after: Option<&str>,
    page: usize,
) -> Result<(Vec<String>, bool), ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": CreditReviewDecision::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "filter": after.map(|id| json!({"id": {"$gt": id}})),
                "sort": [{"field": "id", "direction": "asc"}], "limit": page
            }),
        )
        .await?;
    let listed: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // Decode the data/list command envelope once at the storage boundary.
    let full = listed.items.len() == page;
    let ids = listed.items.iter().filter_map(|item| item["id"].as_str().map(str::to_string)).collect();
    Ok((ids, full))
}

/// Whether decision `id` is still owed an acceptance: `Ok(Some)` names the submission to settle.
pub async fn owed_acceptance<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    id: &str,
) -> Result<Option<Uuid>, ClientError> {
    let submission = Uuid::parse_str(id).map_err(|e| ClientError::Transport(format!("decision key {id} is not a submission id: {e}")))?;
    let accepted = read_one::<_, CreditReviewAcceptance>(conn, persona_name, id).await?;
    Ok(accepted.is_none().then_some(submission))
}
