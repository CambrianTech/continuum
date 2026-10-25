//! Explicit owner attestation for legacy grades without immutable card linkage.
//! No grader, inference, claim mutation, or training flush is invoked here.

use super::*;
use crate::recipe::bench_title::parse_card_title;
use crate::orm::OrmEntity;
use crate::persona::training_producer::{self as credit, graded, reviewed};
use graded::{BenchmarkCreditAttribution, LegacyGradeEvidence};
use uuid::Uuid;

#[derive(Deserialize, JsonSchema, TS)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum CreditRecoveryParams {
    /// Bind the independently reviewed legacy grade; does not transfer credit.
    Attest { evidence: LegacyGradeEvidence },
    /// Retry the existing transfer using an already persisted exact binding.
    Recover {
        #[ts(type = "string")]
        owner: Uuid,
        #[ts(type = "string")]
        attribution_id: Uuid,
        expected_sha256: String,
    },
}

#[derive(Serialize, TS)]
pub struct CreditRecoveryResult {
    #[ts(type = "string")]
    pub attribution_id: Uuid,
    pub sha256: String,
    pub bound_rows: usize,
    pub accepted_rows: usize,
    pub settlement_attempted: bool,
}

#[derive(Default)]
pub struct BenchmarkCreditRecover;
crate::register_stateless_command!(BenchmarkCreditRecover);

fn invalid(message: impl ToString) -> CommandError {
    CommandError::Invalid(message.to_string())
}

#[async_trait]
impl ActionCommand for BenchmarkCreditRecover {
    const NAME: &'static str = "benchmark/credit-recover";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str = "Owner-only exact legacy grade attribution and idempotent staged-credit recovery; never regrades or forces training.";
    type Params = CreditRecoveryParams;
    type Output = CreditRecoveryResult;

    async fn run(&self, ctx: &Ctx, params: Self::Params) -> Result<Self::Output, CommandError> {
        if crate::modules::grid::trust_policy::caller_trust(ctx.caller.as_ref())
            != crate::routing::trust_level::TrustLevel::Owner
        {
            return Err(CommandError::Denied(
                "legacy grade attestation requires the operator".into(),
            ));
        }
        let registry = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .ok_or_else(|| invalid("persona registry unavailable"))?;
        let airc = crate::modules::work::persona_airc(&registry, ctx, Self::NAME)?;
        let (owner, id) = match &params {
            CreditRecoveryParams::Attest { evidence } => (evidence.owner, evidence.card),
            CreditRecoveryParams::Recover {
                owner,
                attribution_id,
                ..
            } => (*owner, *attribution_id),
        };
        let persona_name = registry
            .roster_snapshot()
            .into_iter()
            .find(|(_, peer)| *peer == owner)
            .map(|(name, _)| name)
            .ok_or_else(|| invalid("original solver is not a registered citizen"))?;
        let conn = reviewed::credit_connection(owner)?;
        let attempted = matches!(&params, CreditRecoveryParams::Recover { .. });
        let binding =
            match params {
                CreditRecoveryParams::Attest { evidence } => {
                    let verdict = verify_evidence(&airc, &evidence).await?;
                    let value = conn.commands().execute_value("data/list", serde_json::json!({
                    "collection": credit::StagedCredit::COLLECTION,
                    "dbPath": format!("@persona:{persona_name}"), "filter": {"cardId": id}
                })).await.map_err(invalid)?;
                    let rows = credit::staged_credit_from_list(value).map_err(invalid)?;
                    if let Some(prior) = reviewed::read_one::<_, BenchmarkCreditAttribution>(
                        &conn,
                        &persona_name,
                        &id.to_string(),
                    )
                    .await
                    .map_err(invalid)?
                    {
                        if prior.evidence != evidence
                            || prior.attested_by != airc.peer_id().as_uuid()
                        {
                            return Err(invalid(
                                "grade attribution already has different evidence",
                            ));
                        }
                        graded::bound_rows_retained(&conn, &persona_name, &prior, rows)
                            .await
                            .map_err(invalid)?;
                        prior
                    } else {
                        let binding = BenchmarkCreditAttribution {
                            id,
                            rows: graded::select_rows(&evidence, verdict.graded_at_ms, &rows)
                                .map_err(invalid)?,
                            evidence,
                            attested_by: airc.peer_id().as_uuid(),
                            graded_at_ms: verdict.graded_at_ms,
                        };
                        graded::persist(&conn, &persona_name, &binding)
                            .await
                            .map_err(invalid)?;
                        binding
                    }
                }
                CreditRecoveryParams::Recover {
                    expected_sha256, ..
                } => {
                    let binding = reviewed::read_one::<_, BenchmarkCreditAttribution>(
                        &conn,
                        &persona_name,
                        &id.to_string(),
                    )
                    .await
                    .map_err(invalid)?
                    .ok_or_else(|| invalid("no durable grade attribution"))?;
                    if binding.id != id
                        || binding.evidence.owner != owner
                        || graded::entity_digest(&binding).map_err(invalid)? != expected_sha256
                    {
                        return Err(invalid("grade attribution identity or digest mismatch"));
                    }
                    verify_evidence(&airc, &binding.evidence).await?;
                    credit::settle_card_credit_bound(id, true, Some(&binding))
                        .await
                        .map_err(invalid)?;
                    binding
                }
            };
        let mut accepted_rows = 0;
        for row in &binding.rows {
            if reviewed::transfer_accepted(&conn, &persona_name, row.id)
                .await
                .map_err(invalid)?
            {
                accepted_rows += 1;
            }
        }
        Ok(CreditRecoveryResult {
            attribution_id: id,
            sha256: graded::entity_digest(&binding).map_err(invalid)?,
            bound_rows: binding.rows.len(),
            accepted_rows,
            settlement_attempted: attempted,
        })
    }
}

async fn verify_evidence(
    airc: &std::sync::Arc<airc_lib::Airc>,
    evidence: &LegacyGradeEvidence,
) -> Result<crate::cognition::swe_bench::SweVerdict, CommandError> {
    let room =
        crate::persona::room_resolve::resolve_room(airc, Some(&evidence.room.to_string())).await?;
    let board = airc.work_board_in(&room).await.map_err(invalid)?;
    let card = board
        .card(airc_work::WorkCardId::from_uuid(evidence.card))
        .ok_or_else(|| invalid("graded card absent"))?;
    let review = board
        .card(airc_work::WorkCardId::from_uuid(evidence.review_card))
        .ok_or_else(|| invalid("independent review absent"))?;
    verify_cards(evidence, card, review)?;
    let posts = airc
        .wall_posts_in(&room, Some(crate::experience::ledger::LEDGER_WALL_CATEGORY))
        .await
        .map_err(invalid)?;
    let post = posts
        .iter()
        .find(|post| post.post_id == evidence.review_post)
        .ok_or_else(|| invalid("exact independent review post is unavailable"))?;
    verify_post(evidence, post)?;
    let reference =
        eval_set_reference(&evidence.benchmark).ok_or_else(|| invalid("unknown gym recipe"))?;
    let task = crate::modules::benchmark_grade::normalized_gym_task(&reference, &evidence.instance)
        .map_err(invalid)?;
    if task.test.is_none() || task.dod_shell.is_some() {
        return Err(invalid(
            "legacy recovery requires a single artifact gym oracle",
        ));
    }
    let file = task
        .solution_file
        .unwrap_or_else(|| format!("{}.rs", task.id));
    if std::path::Path::new(&file)
        .components()
        .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(invalid(
            "gym artifact must remain inside its existing workspace",
        ));
    }
    let workspace = crate::persona::card_checkout::gym_workspace(
        &crate::paths::continuum_home().map_err(invalid)?,
        evidence.owner,
    );
    let artifact = std::fs::read(workspace.join(file)).map_err(invalid)?;
    let bytes = std::fs::read(crate::cognition::swe_bench::verdict_path(
        &evidence.instance,
    ))
    .map_err(invalid)?;
    verify_result(evidence, &artifact, &bytes)
}

pub(crate) fn verify_post(
    evidence: &LegacyGradeEvidence,
    post: &airc_core::doctrine::WallPostPublished,
) -> Result<(), CommandError> {
    if post.post_id != evidence.review_post
        || post.room_id.as_uuid() != evidence.room
        || post.published_by.as_uuid() != evidence.reviewer
        || graded::digest(post.body.as_bytes()) != evidence.review_sha256
    {
        return Err(invalid(
            "independent review author, room or digest mismatch",
        ));
    }
    let ledger: crate::experience::ledger::CardLedger =
        serde_json::from_str(&post.body).map_err(invalid)?; // Decode the accepted wall record at its existing ledger boundary.
    if ledger.card_id != evidence.review_card
        || ledger.by != evidence.reviewer
        || !ledger.decided_fix.as_deref().is_some_and(|text| {
            text.to_ascii_lowercase()
                .contains(&evidence.artifact_sha256.to_ascii_lowercase())
        })
    {
        return Err(invalid("review does not bind this exact artifact"));
    }
    Ok(())
}

pub(crate) fn verify_result(
    evidence: &LegacyGradeEvidence,
    artifact: &[u8],
    bytes: &[u8],
) -> Result<crate::cognition::swe_bench::SweVerdict, CommandError> {
    if graded::digest(artifact) != evidence.artifact_sha256
        || graded::digest(bytes) != evidence.verdict_sha256
    {
        return Err(invalid("durable verdict or artifact changed"));
    }
    let verdict: crate::cognition::swe_bench::SweVerdict =
        serde_json::from_slice(&bytes).map_err(invalid)?; // Decode the existing durable verdict file without rewriting or regrading it.
    if verdict.instance_id != evidence.instance
        || verdict.benchmark != evidence.benchmark
        || verdict.solver != evidence.owner.to_string()
        || !verdict.resolved
        || verdict.error.is_some()
        || !verdict.gate_ok
        || verdict.f2p_total == 0
        || verdict.f2p_passed != verdict.f2p_total
    {
        return Err(invalid(
            "legacy verdict is not a complete passing result for this solver and recipe",
        ));
    }
    Ok(verdict)
}

pub(crate) fn verify_cards(
    e: &LegacyGradeEvidence,
    card: &airc_work::WorkCard,
    review: &airc_work::WorkCard,
) -> Result<(), CommandError> {
    if card.card_id.as_uuid() != e.card
        || card.owner.map(|id| id.as_uuid()) != Some(e.owner)
        || card.claim_id.map(|id| id.as_uuid()) != Some(e.claim)
        || card.state != airc_work::CardState::Closed
        || parse_card_title(&card.title) != Some((e.benchmark.clone(), e.instance.clone()))
        || review.card_id.as_uuid() != e.review_card
        || review.reviews != Some(card.card_id)
        || review.owner.map(|id| id.as_uuid()) != Some(e.reviewer)
        || review.claim_id.map(|id| id.as_uuid()) != Some(e.review_claim)
        || review.state != airc_work::CardState::Closed
        || e.reviewer == e.owner
    {
        return Err(invalid(
            "legacy grade requires the exact closed solver and independent reviewer claims",
        ));
    }
    Ok(())
}
