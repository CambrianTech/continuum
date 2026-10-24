//! A trained gene's trial row: what she trained, on what base, and what the examiner decided.
//!
//! A gene that passes training's own pre-filter is ADOPTED ([`Adoption`]): registered (so the
//! serving engine loads it, dormant, in place) and a [`GeneTrial`] opened on it. From then on
//! it is in her head on every turn ([`genes_for_turn`]), and the examiner
//! ([`crate::genome::examiner`]) sits the continual-learning acceptance test on it: her own
//! turns on related tasks the gene never trained on and on unrelated ones, with and without
//! the gene, on the same live engine; again after a normal restart. Its verdict promotes or
//! retires the gene ([`apply_decision`]); the trial row is the adoption receipt and a
//! promoted gene's row is what rolls it back.
//!
//! The room's card verdicts no longer judge a gene. They did (an arm drawn per card, a
//! four-checkpoint gate), but since a gene runs on every turn (#4916) the without-gene arm
//! never filled and the gate never decided; it is deleted, not kept beside the examiner.
//!
//! One small JSON file, bounded by the genes ever trialled, not by turns.

use crate::ai::types::ActiveAdapterRequest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialState {
    /// Working a share of her cards; no verdict yet.
    Trial,
    /// Part of her genome: every card runs it.
    Promoted,
    /// Out of her genome and out of the serving catalog.
    Retired,
    /// Legacy: ended by the old no-verdict expiry (a week with no card verdict), before
    /// a gene was hers on load (GENE-REUSE-FORK-MINT §3d). Never written anymore; read for
    /// rows that already carry it. Formerly: she did no card work on this base
    /// in the window (or her cards were judged elsewhere), so the trial is over without a
    /// result. Out of the serving catalog like Retired, but typed apart: the gene was not
    /// found worse, it was not judged. The bucket it held is released.
    Expired,
}

/// How long a gene that just landed holds its competence's bucket: her turns are still
/// telling what it does (GENE-REUSE-FORK-MINT §3d, the world's surprise on the turns it
/// ran), so the next fill for the same competence waits instead of forking a child of a
/// gene nobody has used yet (the fork storm, card 17dc0a7b). Two hours, not a week: the
/// hold is about her next job, never about whether the gene stays in her head. A gene
/// NEVER leaves her head by silence; only a retirement takes it out (Cormac on #4916:
/// with the card arm no longer judging, the old seven-day "no verdict" expiry would have
/// removed every gene a week after it loaded).
///
/// INTERIM (Cormac on #4917): the real condition is "this gene has a fitness reading"
/// (§3d: the world's surprise on the turns it ran, an offline gene-off replay). Until
/// that number exists this clock stands in for it; it is not a tuned constant and must
/// not be tuned. When the reading exists, the hold ends on the reading, not on time.
pub const SETTLE_WINDOW_MS: u64 = 2 * 60 * 60 * 1000;

impl GeneTrial {
    /// Landed and still settling: the only trial that holds a bucket or awaits a decision.
    pub fn is_settling(&self, now_ms: u64) -> bool {
        self.state == TrialState::Trial && now_ms.saturating_sub(self.opened_at_ms) < SETTLE_WINDOW_MS
    }
}

/// What the examiner decided, on what evidence. Old rows written by the deleted card gate
/// carry `candidate`/`stable`; they read into the same two arms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialVerdict {
    #[serde(default, alias = "candidate")]
    pub with_gene: crate::genome::examiner::ArmTally,
    #[serde(default, alias = "stable")]
    pub without_gene: crate::genome::examiner::ArmTally,
    pub reason: String,
    /// The examiner's receipt this verdict was read from (`genome_exam_receipts`).
    #[serde(default)]
    pub receipt: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneTrial {
    pub id: Uuid,
    pub persona_id: Uuid,
    /// The gene's name, as the adapter manifest and the fitness ledger speak it.
    pub alias: String,
    /// The GGUF-lora on disk; also what a receipt's `genes` names.
    pub path: PathBuf,
    /// The continuum id of the base it was trained on: it only rides turns served by it.
    pub base_model_id: String,
    pub opened_at_ms: u64,
    pub state: TrialState,
    #[serde(default)]
    pub decided_at_ms: Option<u64>,
    #[serde(default)]
    pub verdict: Option<TrialVerdict>,
    /// The training job's directory (its `request.json` holds the examples the gene trained
    /// on), so the examiner can hold out exactly what it never saw. `None` for a gene that
    /// was not trained here (a reused hub gene) or a row from before the field.
    #[serde(default)]
    pub trained_from: Option<PathBuf>,
}

/// The adapter request that pages trial `t`'s gene in at the scale she serves it.
pub fn page_of(t: &GeneTrial) -> ActiveAdapterRequest {
    t.page()
}

impl GeneTrial {
    fn page(&self) -> ActiveAdapterRequest {
        ActiveAdapterRequest {
            name: self.alias.clone(),
            path: self.path.to_string_lossy().into_owned(),
            domain: String::new(),
            scale: 1.0,
        }
    }
}

/// PURE: the genes `persona` may run with on `base`: every gene she trained that has not
/// been retired (Joel, 2026-10-10: a gene is hers once trained and follows her mind, never
/// her workload). Which of them shape a turn, and how much, is `genome::composer`
/// (`genome::composer`, GENE-REUSE-FORK-MINT §3d). Each at its trained scale 1.0: the
/// composer sets the scale a turn runs it at. What decides whether a gene STAYS is the examiner's verdict.
pub fn her_genes(trials: &[GeneTrial], persona: Uuid, base: &str) -> Vec<ActiveAdapterRequest> {
    trials
        .iter()
        .filter(|t| t.persona_id == persona && t.base_model_id == base)
        .filter(|t| match t.state {
            TrialState::Promoted | TrialState::Trial => true,
            TrialState::Retired | TrialState::Expired => false,
        })
        .map(GeneTrial::page)
        .collect()
}

/// The effects of a decision the examiner wrote (through [`GeneTrials::decide`]). Promoted:
/// its verdict joins the fitness ledger gene recall ranks by (`progress/<persona>.jsonl`).
/// Retired: out of the adapter manifest, so the serving engine retires it in place, and the
/// same ledger row records the loss. Either way the trial row, already written, is the receipt.
pub fn apply_decision(t: &GeneTrial, now_ms: u64) {
    if matches!(t.state, TrialState::Retired | TrialState::Expired) {
        if let Err(error) = crate::genome::adapter_manifest::unregister(&t.path) {
            continuum_base::probe!(
                class = "genome.trial.unregister_failed",
                gene = t.alias.as_str(),
                error = error.as_str(),
                "a retired gene stayed in the adapter manifest: the engine keeps it loaded (dormant) until it is removed"
            );
        }
    }
    if let Some(dir) = crate::genome::fitness_ledger::GeneFitnessIndex::default_dir() {
        let row = fitness_receipt(t, now_ms);
        let line = format!("{row}\n");
        let path = dir.join(format!("{}.jsonl", t.persona_id));
        let written = std::fs::create_dir_all(&dir).and_then(|_| {
            use std::io::Write;
            std::fs::OpenOptions::new().create(true).append(true).open(&path)?.write_all(line.as_bytes())
        });
        if let Err(error) = written {
            continuum_base::probe!(
                class = "genome.trial.ledger_unwritten",
                gene = t.alias.as_str(),
                error = %error,
                "the verdict did not reach the fitness ledger: recall ranks this gene without it"
            );
        }
    }
    let verdict = t.verdict.as_ref();
    continuum_base::probe!(
        class = "genome.trial.decided",
        persona = %t.persona_id,
        gene = t.alias.as_str(),
        trial = %t.id,
        promoted = t.state == TrialState::Promoted,
        with_gene_passed = verdict.map_or(0, |v| v.with_gene.passed) as u64,
        with_gene_settled = verdict.map_or(0, |v| v.with_gene.settled) as u64,
        without_gene_passed = verdict.map_or(0, |v| v.without_gene.passed) as u64,
        without_gene_settled = verdict.map_or(0, |v| v.without_gene.settled) as u64,
        reason = verdict.map_or("", |v| v.reason.as_str()),
        "the examiner decided a gene: promoted into her genome, or retired out of it"
    );
}

/// Preserve the evidence behind a scalar lift in the existing fitness ledger.
/// A path identifies the trial artifact locally; it is not a content hash or
/// proof of compatibility with another base or combination of adapters.
fn fitness_receipt(t: &GeneTrial, now_ms: u64) -> serde_json::Value {
    use crate::genome::examiner::ArmTally;
    let rate = |a: ArmTally| if a.settled == 0 { 0.0 } else { f64::from(a.passed) / f64::from(a.settled) };
    let (with_gene, without_gene) =
        t.verdict.as_ref().map_or((ArmTally::default(), ArmTally::default()), |v| (v.with_gene, v.without_gene));
    serde_json::json!({
        "geneId": t.alias,
        "lift": rate(with_gene) - rate(without_gene),
        "passRate": rate(with_gene),
        "basePassRate": rate(without_gene),
        "capturedAtMs": now_ms,
        "source": "examiner",
        "trial": t.id,
        "personaId": t.persona_id,
        "baseModelId": t.base_model_id,
        "adapterPath": t.path,
        "openedAtMs": t.opened_at_ms,
        "trialState": t.state,
        "withGene": with_gene,
        "withoutGene": without_gene,
        "verdict": t.verdict,
    })
}

/// Why a gene could not be adopted for trial. Each arm names the thing the caller can
/// act on; none is a string the caller has to grep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptRefusal {
    /// The serving manifest would not take the gene: the engine never loads it.
    NotRegistered(String),
    /// Her work already decided this gene (promoted, retired, or expired unjudged): it is
    /// never adopted again as if new, and never re-registered first (Cormac on #4794: a
    /// retired hub gene came back through the manifest on every fill).
    AlreadyDecided(TrialState),
    /// The trial file could not be read: nothing is adopted blind.
    TrialFileUnreadable(String),
    /// No home directory: the trial file has no place.
    NoHome,
    /// The trial file would not take the row.
    TrialFileRefused(String),
}

impl std::fmt::Display for AdoptRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdoptRefusal::NotRegistered(e) => write!(f, "the gene could not be registered for serving: {e}"),
            AdoptRefusal::AlreadyDecided(state) => write!(f, "her work already decided this gene ({state:?}): it is not adopted again"),
            AdoptRefusal::TrialFileUnreadable(e) => write!(f, "the trial file could not be read, nothing adopted blind: {e}"),
            AdoptRefusal::NoHome => f.write_str("no home directory: the gene trial file has no place"),
            AdoptRefusal::TrialFileRefused(e) => write!(f, "the gene trial file did not take the trial: {e}"),
        }
    }
}

/// The ONE way a gene becomes live in her: registered in the serving manifest (the
/// engine loads it in place, dormant, #4467) and a trial opened on it. The completion
/// sentinel adopts a gene it just trained; a reuse decision adopts a gene that already
/// exists (on this node, or pulled from the hub). Same seam, so a gene never serves
/// without a trial and never trials without serving.
pub struct Adoption {
    manifest: PathBuf,
    trials: GeneTrials,
}

impl Adoption {
    pub fn at(manifest: impl Into<PathBuf>, trials: GeneTrials) -> Self {
        Self { manifest: manifest.into(), trials }
    }

    /// The node's manifest and trial file; `Err` names which is unresolvable.
    pub fn default_paths() -> Result<Self, AdoptRefusal> {
        let manifest = crate::genome::adapter_manifest::manifest_path().map_err(AdoptRefusal::NotRegistered)?;
        let trials = GeneTrials::default_store().ok_or(AdoptRefusal::NoHome)?;
        Ok(Self { manifest, trials })
    }

    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    pub fn trials(&self) -> &GeneTrials {
        &self.trials
    }

    pub fn adopt(
        &self,
        persona_id: Uuid,
        alias: &str,
        path: &Path,
        base_model_id: &str,
        trained_from: Option<&Path>,
        now_ms: u64,
    ) -> Result<GeneTrial, AdoptRefusal> {
        // Her verdicts first, before the manifest: a decided gene never serves again by
        // this path, and an open one is returned as it is.
        let all = self.trials.load().map_err(AdoptRefusal::TrialFileUnreadable)?;
        if let Some(existing) = all.iter().find(|t| t.persona_id == persona_id && t.path == path) {
            return match existing.state {
                TrialState::Trial => Ok(existing.clone()),
                decided => Err(AdoptRefusal::AlreadyDecided(decided)),
            };
        }
        crate::genome::adapter_manifest::register_at(
            &self.manifest,
            crate::genome::adapter_manifest::TrainedAdapter {
                alias: alias.to_string(),
                path: path.to_path_buf(),
                base_model_id: base_model_id.to_string(),
            },
        )
        .map_err(AdoptRefusal::NotRegistered)?;
        let trial = self
            .trials
            .open_from(persona_id, alias, path, base_model_id, trained_from, now_ms)
            .map_err(AdoptRefusal::TrialFileRefused)?;
        // The examiner's wake: one adoption seam, so every trial that opens is examined.
        crate::genome::examiner::wake(crate::genome::examiner::TRIAL_OPENED_EVENT, trial.id);
        Ok(trial)
    }
}

/// The trial file.
pub struct GeneTrials {
    path: PathBuf,
}

impl GeneTrials {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `~/.continuum/genome/trials.json`; `None` without a home directory.
    pub fn default_store() -> Option<Self> {
        crate::paths::ContinuumLayout::home()
            .ok()
            .map(|layout| Self::at(layout.genome().join("trials.json")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every trial. A missing file is no trials; an unreadable one is an error the caller
    /// surfaces (a turn then runs its current genome, never a guessed one).
    pub fn load(&self) -> Result<Vec<GeneTrial>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(body) => serde_json::from_str(&body)
                .map_err(|e| format!("gene trials {} unreadable: {e}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(format!("gene trials {}: {e}", self.path.display())),
        }
    }

    /// Test-only: write rows as the gate would (a test that needs a decided trial writes
    /// the same row the gate writes, through the same save).
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn save_for_test(&self, all: &[GeneTrial]) -> Result<(), String> {
        self.save(all)
    }

    fn save(&self, all: &[GeneTrial]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }

    /// Open a trial for a gene with no recorded training job (a fixture, a reused gene).
    pub fn open(
        &self,
        persona_id: Uuid,
        alias: &str,
        path: &Path,
        base_model_id: &str,
        now_ms: u64,
    ) -> Result<GeneTrial, String> {
        self.open_from(persona_id, alias, path, base_model_id, None, now_ms)
    }

    /// Open a trial for a freshly trained gene. A gene already on file (the same path for
    /// the same persona) is not opened twice: its existing row is returned whatever its
    /// state; [`Adoption::adopt`] is the seam that refuses a decided one before serving it.
    pub fn open_from(
        &self,
        persona_id: Uuid,
        alias: &str,
        path: &Path,
        base_model_id: &str,
        trained_from: Option<&Path>,
        now_ms: u64,
    ) -> Result<GeneTrial, String> {
        let mut all = self.load()?;
        if let Some(existing) = all.iter().find(|t| t.persona_id == persona_id && t.path == path) {
            return Ok(existing.clone());
        }
        let trial = GeneTrial {
            id: Uuid::new_v4(),
            persona_id,
            alias: alias.to_string(),
            path: path.to_path_buf(),
            base_model_id: base_model_id.to_string(),
            opened_at_ms: now_ms,
            state: TrialState::Trial,
            decided_at_ms: None,
            verdict: None,
            trained_from: trained_from.map(Path::to_path_buf),
        };
        all.push(trial.clone());
        self.save(&all)?;
        Ok(trial)
    }

    /// Record the examiner's decision. An open trial is decided once; the one later move is a
    /// promoted gene RETIRED by its retention re-run (it lost what it learned across a
    /// restart). Anything else already decided is left as it is.
    pub fn decide(
        &self,
        id: Uuid,
        state: TrialState,
        verdict: TrialVerdict,
        now_ms: u64,
    ) -> Result<Option<GeneTrial>, String> {
        let mut all = self.load()?;
        let movable = |from: TrialState| {
            from == TrialState::Trial || (from == TrialState::Promoted && state == TrialState::Retired)
        };
        let Some(t) = all.iter_mut().find(|t| t.id == id && movable(t.state)) else {
            return Ok(None);
        };
        t.state = state;
        t.decided_at_ms = Some(now_ms);
        t.verdict = Some(verdict);
        let decided = t.clone();
        self.save(&all)?;
        Ok(Some(decided))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::examiner::ArmTally;

    // what this catches (Cormac on #4916): a gene never leaves her head by silence. A gene
    // that just landed is settling (it holds its competence's bucket while her turns tell
    // what it does); past the window it holds nothing, and its state is untouched: there is
    // no expiry to run.
    #[test]
    fn a_landed_gene_settles_for_a_while_and_is_never_expired() {
        let dir = tempfile::tempdir().unwrap();
        let store = GeneTrials::at(dir.path().join("trials.json"));
        let persona = Uuid::from_u128(0x7);
        let old = store.open(persona, "code", std::path::Path::new("/g/old.gguf"), "base", 1_000).unwrap();
        let young = store.open(persona, "code", std::path::Path::new("/g/young.gguf"), "base", 1_000 + SETTLE_WINDOW_MS).unwrap();
        let now = 1_000 + SETTLE_WINDOW_MS + 1;
        assert!(!old.is_settling(now), "past its window: holds nothing");
        assert!(young.is_settling(now), "just landed: her next fill waits");
        let all = store.load().unwrap();
        assert!(all.iter().all(|t| t.state == TrialState::Trial), "nothing expired anything: both stay hers");
        assert_eq!(her_genes(&all, persona, "base").len(), 2, "both are in her head, settling or not");
    }

    // what this catches: a gene in her head that the examiner retired, or one missing that it
    // kept; a promoted gene on another base; a decision rewritten except by the retention
    // re-run (promoted -> retired); a fitness row that loses the evidence behind its lift; and
    // a trial file written by the deleted card gate that no longer reads.
    #[test]
    fn a_trial_is_decided_once_and_only_retention_retires_a_promoted_gene() {
        let dir = tempfile::tempdir().expect("test: dir");
        let store = GeneTrials::at(dir.path().join("trials.json"));
        let kimi = Uuid::from_u128(0x6b1);
        let job = Path::new("/genome/kimi/code/job-1");
        let t = store.open_from(kimi, "kimi-dream-1", Path::new("/genes/k1.gguf"), "qwen-27b", Some(job), 10).unwrap();
        assert_eq!(t.trained_from.as_deref(), Some(job));
        assert_eq!(store.open(kimi, "again", Path::new("/genes/k1.gguf"), "qwen-27b", 20).unwrap().id, t.id, "one trial per gene");
        let trials = store.load().unwrap();
        // what this catches (Joel, 2026-10-10; gene 1 loaded on the 5090 and served zero turns
        // because she held no card): a trained gene is in her head on every turn.
        assert_eq!(her_genes(&trials, kimi, "qwen-27b").len(), 1, "a turn talks through the gene she trained");
        assert!(her_genes(&trials, kimi, "other-base").is_empty());
        assert!(her_genes(&trials, Uuid::nil(), "qwen-27b").is_empty());

        let verdict = TrialVerdict {
            with_gene: ArmTally { settled: 6, passed: 4 },
            without_gene: ArmTally { settled: 6, passed: 3 },
            reason: "better on unseen tasks".into(),
            receipt: Some(Uuid::from_u128(0xe)),
        };
        let decided = store.decide(t.id, TrialState::Promoted, verdict.clone(), 30).unwrap().expect("decided");
        let receipt = fitness_receipt(&decided, 30);
        assert_eq!(receipt["baseModelId"], "qwen-27b");
        assert_eq!(receipt["personaId"], kimi.to_string());
        assert_eq!(receipt["trial"], t.id.to_string());
        assert_eq!(receipt["adapterPath"], "/genes/k1.gguf");
        assert_eq!(receipt["withGene"]["settled"], 6);
        assert_eq!(receipt["withoutGene"]["passed"], 3);
        assert_eq!(receipt["trialState"], "promoted");
        assert!((receipt["lift"].as_f64().expect("test: numeric lift") - 1.0 / 6.0).abs() < 1e-12);
        assert!(store.decide(t.id, TrialState::Promoted, verdict.clone(), 40).unwrap().is_none(), "decided once");
        let retired = store.decide(t.id, TrialState::Retired, verdict.clone(), 50).unwrap().expect("retention retires");
        assert_eq!(retired.state, TrialState::Retired);
        assert!(store.decide(t.id, TrialState::Promoted, verdict, 60).unwrap().is_none(), "a retired gene stays retired");
        assert!(her_genes(&store.load().unwrap(), kimi, "qwen-27b").is_empty(), "a retired gene never runs");

        // A row the deleted card gate wrote still reads, its arms carried into the new names.
        let legacy = serde_json::json!([{
            "id": Uuid::from_u128(1), "persona_id": kimi, "alias": "g", "path": "/g.gguf",
            "base_model_id": "b", "share_milli": 500, "opened_at_ms": 1, "state": "promoted",
            "verdict": {"candidate": {"settled": 10, "passed": 9}, "stable": {"settled": 10, "passed": 2}, "reason": "old"},
            "candidate": {"settled": 10, "passed": 9}, "stable": {"settled": 10, "passed": 2},
            "credited_cards": [], "looked": 1
        }]);
        std::fs::write(store.path(), legacy.to_string()).expect("test: legacy row");
        let rows = store.load().expect("a legacy trial file reads");
        assert_eq!(rows[0].verdict.as_ref().map(|v| v.with_gene.passed), Some(9));
        assert_eq!(rows[0].trained_from, None);
    }
}
