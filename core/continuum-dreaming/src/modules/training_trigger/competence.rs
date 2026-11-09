//! Which gene a competence trains (card 4114a2c6): the signature minted from the examples in hand,
//! and the decision against her surprise, her signature store, her trials and the hub (join, await,
//! reuse, fork, mint; GENE-REUSE-FORK-MINT.md). Moved out of `genome/job-create` so in-place
//! learning decides what a bucket seats by the same rule; job-create calls it until it is deleted.

use crate::genome::competence::Decision;
use crate::genome::fine_tuning::types::TrainingExample;
use crate::genome::gene_trial::{Adoption, TrialState};
use crate::genome::signature::GeneSignature;

/// Mint the embedding-space signature of `examples`, named `name` (an inline dataset is named by
/// its trait). `None` when the mint fails: the learning proceeds and the gene routes by fallback.
pub(crate) async fn mint_signature(name: String, trait_kind: &str, examples: &[TrainingExample]) -> Option<GeneSignature> {
    let texts: Vec<String> = examples.iter().map(|ex| format!("{}\n{}", ex.prompt, ex.completion)).collect();
    let joined = texts.join("\n");
    let corpus = crate::genome::corpus_ref::CorpusRef {
        name,
        content_hash: crate::cognition::inbox_admission::content_hash_sha256(&joined),
        size_bytes: joined.len() as u64,
        source_url: None,
    };
    let embedder = crate::ai::embedding::resolve_recall_embedder_local().await;
    match GeneSignature::mint(&texts, corpus, &embedder, continuum_base::time::EpochMs::now().millis()).await {
        Ok(sig) => Some(sig),
        Err(e) => {
            tracing::warn!(trait_kind = %trait_kind, error = %e, "gene signature mint failed — learning proceeds, gene will route by fallback");
            None
        }
    }
}

/// Whose competence is being decided, and from how many examples.
pub(crate) struct Asking<'a> {
    pub(crate) persona_id: uuid::Uuid,
    pub(crate) persona_name: &'a str,
    pub(crate) trait_kind: &'a str,
    pub(crate) base_model: &'a str,
    pub(crate) examples: usize,
}

/// Decide what `asking`'s competence `sig` trains: `in_flight` is the work of hers already learning
/// nearby (its id and signature). An unreadable trial file refuses: nothing is decided blind.
pub(crate) async fn decide(
    asking: &Asking<'_>,
    sig: &GeneSignature,
    adoption: &Result<Adoption, crate::genome::gene_trial::AdoptRefusal>,
    in_flight: &[(uuid::Uuid, GeneSignature)],
) -> Result<Decision, String> {
    let Asking { persona_id, persona_name, trait_kind, base_model, examples } = *asking;
    use crate::genome::competence::{decide_with_pending, nearest_in_store, nearest_pending, Competence, Surprise, SIM_FORK};
    let competence = Competence {
        centroid: sig.centroid.clone(),
        members: (0..examples).collect(),
        cohesion: 1.0, // one bucket is one competence here; clustering within it is step 2's follow-on
        representative: 0,
    };
    // The signature store lives beside the manifest the adoption serves through: ONE
    // derivation of the path, in tests and in production alike.
    let adoption = match adoption.as_ref() {
        Ok(a) => a,
        Err(refusal) => return Err(format!("her trial file has no place: {refusal}")),
    };
    let store_path = adoption.manifest().with_file_name("signatures.json");
    let store = crate::genome::signature::SignatureStore::load_at(&store_path)
        .unwrap_or_default(); // unwrap_or_default: no store yet = nothing near, which decides "mint" honestly
    // HER TRIALS, read once and LOUD when unreadable (Cormac on #4794): a corrupt
    // trial file read as empty would let a fork or a mint go ahead beside her open
    // trial, and would offer a retired gene back. Nothing is decided blind.
    let trials = match adoption.trials().load() {
        Ok(t) => t,
        Err(e) => return Err(format!("her trial file could not be read: {e}")),
    };
    // A pre-1970 clock reads 0: every tally falls outside its window (not measured,
    // which never halts training) and every open trial reads live (held, never
    // forked beside).
    let now_ms = continuum_base::time::EpochMs::now().millis();
    let hers = |t: &&crate::genome::gene_trial::GeneTrial| t.persona_id == persona_id;
    // Residency is HERS: a gene her work holds open or promoted on this base. The
    // node's manifest carries every citizen's genes; a teammate's promoted gene near
    // her competence is a reuse for her, never a fork (Cormac on #4794, point 7).
    let resident: Vec<std::path::PathBuf> = trials
        .iter()
        .filter(hers)
        .filter(|t| t.base_model_id == base_model && matches!(t.state, TrialState::Trial | TrialState::Promoted))
        .map(|t| t.path.clone())
        .collect();
    // A gene her work already decided against, or left unjudged, is never offered
    // back as a reuse, from the store or the hub.
    let retired: Vec<std::path::PathBuf> = trials
        .iter()
        .filter(hers)
        .filter(|t| matches!(t.state, TrialState::Retired | TrialState::Expired))
        .map(|t| t.path.clone())
        .collect();
    // ONE LOOKUP, THREE SOURCES (GENE-REUSE-FORK-MINT.md §5): her store first; the
    // hub only when the store has nothing within fork distance, since a hub
    // probe costs a network read per candidate and local knowledge, when it is
    // near, is the answer. The mesh (peers' stores over airc) is the source
    // between them, not yet wired. Only a Reuse decision ever pulls weights.
    let local = nearest_in_store(&competence, &store, &sig.embedder, &resident, &retired);
    let (nearest, source) = match local {
        Some(n) if n.similarity >= SIM_FORK => (Some(n), "store"),
        local => match crate::commands::genome_share::nearest_on_hub(&competence, &sig.embedder, &base_model, None, 20, &retired).await {
            Some(hub) if local.as_ref().is_none_or(|l| hub.similarity > l.similarity) => (Some(hub), "hub"),
            _ => (local, "store"),
        },
    };
    // A JOB OF HERS ALREADY TRAINING THIS COMPETENCE is checked before any branch
    // that would train: the board's watched jobs for this persona and base, by the
    // signature each was minted with. Four Mints for one card's credit (the 5090,
    // 2026-10-05) is the falsifier this closes.
    let in_flight_near = nearest_pending(
        &competence,
        &sig.embedder,
        in_flight.iter().map(|(id, signature)| (*id, signature)),
    );
    // A GENE THAT JUST LANDED for this competence and is still settling: ONE
    // matching rule, the bucket's key (her, this trait, this base), the same rule
    // the trigger holds the bucket by (Cormac on #4794, point 4: two rules churned
    // a bucket between them). The signature similarity rides along as a measurement,
    // never as the decision; past its settling window a gene holds nothing and is
    // simply resident (`nearest`).
    let on_trial = trials
        .iter()
        .filter(hers)
        .filter(|t| t.base_model_id == base_model && t.alias == trait_kind && t.is_settling(now_ms))
        .map(|t| crate::genome::competence::Pending {
            id: t.id,
            similarity: store.by_path.get(&t.path.display().to_string()).and_then(|s| s.similarity_in(&sig.embedder, &competence.centroid)).unwrap_or(1.0), // 1.0 = the key already says it is this competence; the number is a measurement when the signature exists
        })
        .max_by(|a, b| a.similarity.total_cmp(&b.similarity));
    // HER SURPRISE, read from her own strip: the verdict surprise (#4774, her stated
    // expectations against the room's verdicts, every activity folded by counts).
    // `S(C)` is this until the model surprise is measured per competence
    // (GENE-REUSE-FORK-MINT.md §3). Not yet judged is not low.
    let surprise = crate::persona::perception_feed::awareness_of(persona_id, now_ms)
        .and_then(|a| a.verdict_surprise());
    let decision = decide_with_pending(
        surprise.map_or(Surprise::NotYetMeasured, |v| Surprise::Measured { s: v.s }),
        nearest.as_ref(),
        in_flight_near.as_ref(),
        on_trial.as_ref(),
    );
    continuum_base::probe!(
        class = "genome.decision",
        persona = %persona_name,
        trait_kind = %trait_kind,
        examples = examples as u64,
        branch = ?decision,
        nearest = %nearest.as_ref().map(|n| n.gene.to_string()).unwrap_or_default(), // "" = nothing in any source
        similarity = nearest.as_ref().map(|n| n.similarity).unwrap_or(0.0), // 0.0 = nothing in any source
        nearest_resident = nearest.as_ref().is_some_and(|n| n.resident),
        source,
        in_flight = %in_flight_near.as_ref().map(|j| j.id.to_string()).unwrap_or_default(), // "" = no job of hers training nearby
        on_trial = %on_trial.as_ref().map(|t| t.id.to_string()).unwrap_or_default(), // "" = no gene of hers on trial nearby
        retired = retired.len() as u64,
        surprise = if surprise.is_some() { "verdict" } else { "not_measured" },
        s = surprise.map(|v| v.s).unwrap_or(0.0), // 0.0 = not measured; read `surprise` first
        judged = surprise.map(|v| v.judged).unwrap_or(0) as u64, // 0 = not measured; read `surprise` first
        "a full bucket decided: join, await, reuse, fork or mint, against her surprise, her store, her trials and the hub"
    );
    Ok(decision)
}

/// Why a reuse adopted nothing: the hub gene did not pull, or the adoption refused it. The examples
/// stay pending either way.
#[derive(Debug)]
pub(crate) enum ReuseRefused {
    Pull(String),
    Adopt(String),
}

/// Reuse `gene` for `asking`'s competence: pull it when it is on the hub, then adopt it for trial
/// on her cards, with no training. The trial's id.
pub(crate) async fn reuse(
    asking: &Asking<'_>,
    gene: &crate::genome::competence::GeneRef,
    similarity: f32,
    adoption: &Result<Adoption, crate::genome::gene_trial::AdoptRefusal>,
) -> Result<uuid::Uuid, ReuseRefused> {
    use crate::genome::competence::GeneRef;
    let path = match gene {
        GeneRef::Local { path } => path.clone(),
        GeneRef::Hub { repo } => {
            let pulled = crate::commands::genome_share::pull_gene(&crate::commands::genome_share::GenomePullParams {
                repo: repo.clone(),
                base_model: asking.base_model.to_owned(),
                alias: Some(asking.trait_kind.to_owned()),
            })
            .await;
            match pulled {
                Ok(r) => std::path::PathBuf::from(r.path),
                Err(e) => {
                    continuum_base::probe!(
                        class = "genome.reuse.refused",
                        persona = %asking.persona_name,
                        trait_kind = %asking.trait_kind,
                        gene = %gene,
                        error = %e,
                        "the hub gene this competence would reuse did not pull: no trial, no learning; the examples stay pending"
                    );
                    return Err(ReuseRefused::Pull(format!("reuse of {gene}: pull failed: {e}")));
                }
            }
        }
    };
    let adopted = adoption
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|a| a.adopt(asking.persona_id, asking.trait_kind, &path, asking.base_model, None, continuum_base::time::EpochMs::now().millis()));
    match adopted {
        Ok(trial) => {
            continuum_base::probe!(
                class = "genome.reused",
                persona = %asking.persona_name,
                trait_kind = %asking.trait_kind,
                examples = asking.examples as u64,
                gene = %gene,
                path = %path.display(),
                similarity = similarity,
                trial = %trial.id,
                "an existing gene carries this competence: adopted, no training; with no learning here the examiner receipts it unexaminable"
            );
            Ok(trial.id)
        }
        Err(refusal) => {
            continuum_base::probe!(
                class = "genome.reuse.refused",
                persona = %asking.persona_name,
                trait_kind = %asking.trait_kind,
                gene = %gene,
                refusal = %refusal,
                "the gene this competence would reuse was not adopted: no trial, no learning; the examples stay pending"
            );
            Err(ReuseRefused::Adopt(format!("reuse of {gene}: {refusal}")))
        }
    }
}
