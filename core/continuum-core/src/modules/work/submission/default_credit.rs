//! The credit a resident's submission binds when she names none (card fe8554d2).

use super::*;

/// Her unique newest staged revision of this claim, or None with the reason probed. A failed
/// read is never "no work": it is probed as unreadable, and the submission still publishes
/// unbound, so its reviews stay unresolved and never count as passed.
pub(super) async fn default_revision(
    conn: &Connection<InProcessTransport>,
    persona_name: &str,
    candidate: &airc_work::WorkSubmission,
) -> Option<Uuid> {
    let reason = match reviewed::staged_evidence(conn, persona_name, candidate).await {
        Ok(evidence) => match reviewed::default_selection(&evidence) {
            Ok(revision) => return Some(revision),
            Err(unbound) => unbound.as_str().to_string(),
        },
        Err(e) => format!("evidence_unreadable: {e}"),
    };
    probe_default_unbound(persona_name, candidate.card_id.as_uuid(), candidate.submission_id.as_uuid(), &reason);
    None
}

/// One row per unbound submission, so a resident whose reviews never reach her credit is seen.
pub(super) fn probe_default_unbound(persona_name: &str, card: Uuid, submission: Uuid, reason: &str) {
    continuum_base::probe!(
        class = "training.credit.default_unbound",
        persona = %persona_name,
        card = %card,
        submission = %submission,
        reason = %reason,
        "submission publishes with no credit binding: its reviews cannot reach her credit"
    );
}
