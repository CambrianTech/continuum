//! A persona's genome as one object: her gene trials, the signatures that route her genes, and the
//! residency owner that keeps her current genes on the device. It answers one question per turn,
//! [`Genome::compose`]: which of her genes run for what her mind is on, on the base being served.
//!
//! Before it, composing was a free function that found her trial file and the served base through
//! process globals; the genome had no owner, so nothing could hold it, test it with a real store,
//! or move it with the persona (CONTINUUM-ENGINE 4.14: the genome is part of the persona unit).

use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use crate::ai::types::ActiveAdapterRequest;
use crate::genome::composer::{select, Candidate, Composition, Need, Phenotype};
use crate::genome::gene_residency::GeneResidency;
use crate::genome::gene_trial::{GeneTrials, Standing, TrialState};
use crate::genome::signature::SignatureStore;

/// One persona's genome.
pub struct Genome {
    persona: Uuid,
    /// Her gene trials; `None` when this node keeps no genome store (she has no genes here).
    trials: Option<GeneTrials>,
    /// Where her genes' signatures live, or why that could not be resolved.
    signatures: Result<PathBuf, String>,
    residency: Arc<GeneResidency>,
}

impl Genome {
    /// Her genome in this node's continuum home.
    pub fn of(persona: Uuid) -> Self {
        Self {
            persona,
            trials: GeneTrials::default_store(),
            signatures: crate::genome::signature::signature_store_path(),
            residency: Arc::clone(GeneResidency::global()),
        }
    }

    /// Her genome over given stores (another root, or a test's own directory).
    pub fn at(persona: Uuid, trials: GeneTrials, signatures: PathBuf, residency: Arc<GeneResidency>) -> Self {
        Self { persona, trials: Some(trials), signatures: Ok(signatures), residency }
    }

    /// Her phenotype for the next turn on `base` (the model being served, `None` when not known
    /// yet), composed from `perception` (what her mind is on: the turn's focused line). Unknown only
    /// when her trials do not read, or the base is unknown while she holds genes; no genes is an
    /// answer, never an unknown.
    pub async fn compose(&self, base: Option<&str>, perception: &str) -> Composition {
        let persona = self.persona;
        let Some(store) = &self.trials else {
            return Composition::Genes(Phenotype::default());
        };
        let trials = match store.load() {
            Ok(trials) => trials,
            Err(error) => {
                continuum_base::probe!(
                    class = "genome.trial.unreadable",
                    persona = %persona,
                    error = error.as_str(),
                    "the gene trial file did not read: her genome is unknown this turn"
                );
                return Composition::Unknown("the gene trial file did not read");
            }
        };
        // Genes run only on their base: unknown for a mind that holds genes, simply none for one
        // that holds none (a cloud-served or gene-less persona never waits).
        let Some(base) = base else {
            let holds_genes = trials.iter().any(|t| t.persona_id == persona && matches!(t.state, TrialState::Trial | TrialState::Promoted));
            return if holds_genes { Composition::Unknown("the served base model is not known yet") } else { Composition::Genes(Phenotype::default()) };
        };
        let now_ms = continuum_base::time::EpochMs::now().millis();
        let all = crate::genome::gene_trial::her_genes(&trials, persona, base);
        let pending = all.iter().filter(|(_, standing)| *standing == Standing::Pending).count();
        let genes: Vec<ActiveAdapterRequest> = all.into_iter().filter(|(_, standing)| standing.may_run()).map(|(gene, _)| gene).collect();
        if genes.is_empty() {
            self.residency.record_turn(persona, &Phenotype::default(), now_ms);
            return Composition::Genes(Phenotype::default());
        }
        let signatures = match self.signatures.as_ref().map_err(String::clone).and_then(|path| SignatureStore::load_at(path)) {
            Ok(store) => store,
            Err(error) => {
                continuum_base::probe!(
                    class = "genome.select.signatures_unreadable",
                    persona = %persona,
                    error = error.as_str(),
                    "the signature sidecar did not read: her genes run unrouted this turn"
                );
                SignatureStore::default()
            }
        };
        let candidates: Vec<Candidate<'_>> = genes.into_iter().map(|gene| Candidate { signature: signatures.by_path.get(&gene.path), gene }).collect();
        let embedded = if candidates.iter().any(|c| c.signature.is_some()) && !perception.trim().is_empty() {
            let embedder = crate::ai::embedding::resolve_recall_embedder_local().await;
            Some((embedder.id().to_string(), embedder.embed(perception).await, embedder.unrelated_null()))
        } else {
            None
        };
        let need = embedded.as_ref().map(|(id, vector, null)| Need { embedder_id: id, vector, unrelated_null: *null });
        let previous = self.residency.previous(persona);
        // Capacity: every gene she may run. Each is loaded in the engine (her trial loaded it), and
        // one in host RAM pages back in on demand; the residency owner keeps her current ones on the
        // device and pages the cold ones out under the one device budget.
        let selection = select(need.as_ref(), &candidates, candidates.len(), &previous);
        crate::genome::composer::probe_selection(persona, &selection, pending);
        self.residency.record_turn(persona, &selection.phenotype, now_ms);
        self.residency.schedule_reconcile();
        Composition::Genes(selection.phenotype)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::gene_trial::GeneTrial;

    fn trial(persona: Uuid, base: &str, state: TrialState) -> GeneTrial {
        GeneTrial {
            id: Uuid::new_v4(),
            persona_id: persona,
            alias: "code/owner".into(),
            path: PathBuf::from("/genome/Aris/code_owner/adapter.gguf"),
            base_model_id: base.into(),
            opened_at_ms: 1,
            state,
            decided_at_ms: None,
            verdict: None,
            trained_from: None,
        }
    }

    // what this catches (the M5, 2026-11-17): her genome composed through process globals, so a
    // turn could not be shown to carry her persisted gene, and an unreadable store or an unknown
    // base could read as "no genes". Over a real trial file: her promoted gene composes on its base,
    // an unknown base is unknown while she holds it, another persona's or base's gene is not hers,
    // and an unreadable file is unknown, never empty.
    #[tokio::test]
    async fn her_genome_composes_her_persisted_gene_and_never_reads_unknown_as_empty() {
        let dir = std::env::temp_dir().join(format!("persona-genome-{}", Uuid::new_v4()));
        let store = GeneTrials::at(dir.join("trials.json"));
        let (aris, other) = (Uuid::new_v4(), Uuid::new_v4());
        store
            .save_for_test(&[trial(aris, "qwen-a", TrialState::Promoted), trial(other, "qwen-a", TrialState::Promoted), trial(aris, "qwen-b", TrialState::Retired)])
            .expect("test: the trial file is written");
        let genome = Genome::at(aris, GeneTrials::at(dir.join("trials.json")), dir.join("signatures.json"), Arc::new(GeneResidency::default()));

        let Composition::Genes(phenotype) = genome.compose(Some("qwen-a"), "fix the build").await else {
            panic!("test: a readable store on a known base composes");
        };
        assert_eq!(phenotype.genes.iter().map(|g| g.path.clone()).collect::<Vec<_>>(), vec![PathBuf::from("/genome/Aris/code_owner/adapter.gguf")], "her promoted gene, once");
        assert!(matches!(genome.compose(None, "fix the build").await, Composition::Unknown(_)), "an unknown base while she holds a gene is unknown");
        let Composition::Genes(none) = genome.compose(Some("qwen-b"), "fix the build").await else {
            panic!("test: a known base she holds no live gene on composes");
        };
        assert!(none.genes.is_empty(), "her retired gene on another base does not run");

        std::fs::write(dir.join("trials.json"), "not a trial file").expect("test: corrupt the file");
        assert!(matches!(genome.compose(Some("qwen-a"), "fix the build").await, Composition::Unknown(_)), "an unreadable store is unknown, never empty");
        std::fs::remove_dir_all(&dir).expect("test: clean up");
    }
}
